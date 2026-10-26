//! `design_edit`: `ig_design::apply` on the design and screen files' bytes, written by
//! `screens::commit` only over those same bytes, then the live design check.

use super::*;
use ig_app::design as app;
use ig_app::screens;
use ig_core::design::{Board, Op, apply};

const MAX_OPS: usize = 200;

/// One retry when the app wrote in between. Screens are read only when an op names one, so a
/// broken screen file never blocks an edit of the architecture.
pub(super) fn edit(root: &Path, args: &Value) -> anyhow::Result<(String, Option<Value>)> {
    let slug = args["slug"].as_str().unwrap_or("");
    if !app::valid_slug(slug) {
        anyhow::bail!("design_edit: bad feature name {slug:?}");
    }
    let ops: Vec<Op> = serde_json::from_value(args["ops"].clone())
        .map_err(|e| anyhow::anyhow!("design_edit: ops: {e}"))?;
    if ops.len() > MAX_OPS {
        anyhow::bail!("design_edit: at most {MAX_OPS} ops per call");
    }
    ig_wire::real_plans(root).map_err(|e| anyhow::anyhow!("design_edit: {e}"))?;
    let path = app::design_path(slug);
    if ops.is_empty() {
        return Ok((format!("no ops: {path} unchanged"), None));
    }
    let with_screens = ops.iter().any(Op::is_screen);
    let w = Wired::new(root);
    // ponytail: read-compare-write without a lock; add a .ig lock file if a conflict is ever seen.
    for _ in 0..2 {
        let (design, bytes) = app::read_raw(w.files(), slug)?;
        let old = if with_screens {
            screens::read_all(w.files(), slug)?
        } else {
            Default::default()
        };
        let board = Board {
            design: design.clone(),
            screens: old.iter().map(|(k, v)| (k.clone(), v.0.clone())).collect(),
        };
        let (next, t) = apply(&board, &ops)?;
        if next == board {
            return Ok((format!("no change: {path} already says so"), None));
        }
        if !screens::commit(w.files(), slug, &design, &bytes, &old, &next, &t.screens)? {
            continue;
        }
        let touched = json!({"elements": t.elements, "links": t.links, "bridges": t.bridges, "screens": t.screens});
        // the files are written: a check that fails is reported, never as a failed edit
        let check = design_check(&w, slug, &next.design)
            .unwrap_or_else(|e| json!({"ok": null, "findings": [], "error": format!("{e:#}")}));
        let mut text = format!("edited {path}\ntouched: {touched}\ncheck: ");
        text += &match (&check["ok"], check["error"].as_str()) {
            (_, Some(e)) => format!("unavailable ({e})"),
            (Value::Bool(true), _) => "ok".into(),
            _ => "errors".into(),
        };
        for f in check["findings"].as_array().into_iter().flatten().take(20) {
            text += &format!(
                "\n- {} {} ({}): {}",
                f["severity"].as_str().unwrap_or(""),
                f["ruleId"].as_str().unwrap_or(""),
                f["target"].as_str().unwrap_or("-"),
                f["message"].as_str().unwrap_or("")
            );
        }
        return Ok((text, Some(json!({"touched": touched, "check": check}))));
    }
    anyhow::bail!("design_edit: {path} keeps changing; try again")
}

/// `ig_app::design::check` of the written design (the app's live check).
fn design_check(w: &Wired, slug: &str, d: &ig_core::design::Design) -> anyhow::Result<Value> {
    let screens = ig_app::screens::pieces(w.files(), slug);
    let head = ig_app::plan::Head::load(&w.env())?;
    let v = ig_app::design::check(&head, d, &screens, &ig_app::design::design_path(slug))?;
    Ok(json!({"ok": v["ok"], "findings": v["findings"]}))
}
