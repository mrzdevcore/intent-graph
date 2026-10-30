//! `ig accept`: the code's drift becomes the spec, one person edit per item, written one by one:
//! an error stops it with the items before it accepted (and printed).

use anyhow::{Result, bail};
use ig_app::map::edit::Edit;
use ig_app::map::edit::journal::{By, record};
use ig_app::map::file::MapBox;
use ig_app::map::origin::Origin;
use ig_app::map::{ArrowKind, Level, MapArrow, view};
use ig_core::ports::Files;

/// Records the drift named by `ids` (all of it for `all`); returns the exit code.
pub fn run(files: &dyn Files, v: &view::View, all: bool, ids: &[String]) -> Result<u8> {
    let boxes: Vec<&MapBox> = v
        .boxes
        .iter()
        .filter(|b| b.origin == Origin::Drift)
        .collect();
    let arrows: Vec<&MapArrow> = v
        .arrows
        .iter()
        .filter(|a| a.origin == Origin::Drift)
        .collect();
    let unknown: Vec<&String> = (ids.iter())
        .filter(|i| !boxes.iter().any(|b| &b.id == *i) && !arrows.iter().any(|a| &a.id == *i))
        .collect();
    if !unknown.is_empty() {
        let list: Vec<&str> = unknown.iter().map(|s| s.as_str()).collect();
        bail!(
            "not drift: {} (see `ig plan`; quote arrow ids: ig accept 'web->api')",
            list.join(", ")
        );
    }
    let want = |id: &str| all || ids.iter().any(|i| i == id);
    let Some(root) = v.boxes.iter().find(|b| b.level == Level::System) else {
        bail!("no system box in ig/");
    };
    // a box claiming the whole repo would hide every later drift
    if let Some(b) = boxes
        .iter()
        .find(|b| want(&b.id) && b.made_of.iter().any(|g| g == "**"))
    {
        bail!(
            "refusing to accept {}: it claims `**`, the whole repo",
            b.id
        );
    }
    // each printed as it is recorded: an error part way leaves those before it accepted
    let mut done: Vec<String> = vec![];
    for b in boxes.iter().filter(|b| want(&b.id)) {
        let edit = Edit::Add {
            parent: root.id.clone(),
            kind: b.kind.value,
            id: b.id.clone(),
            title: b.title.value.clone(),
            at: b.made_of.clone(),
            before: None,
        };
        record(files, By::Person, &edit).map_err(|e| anyhow::anyhow!(e.join("\n")))?;
        println!("accepted {}", b.id);
        done.push(b.id.clone());
    }
    // an arrow needs both ends in the spec: the boxes this run added count
    let in_spec = |id: &str| {
        done.iter().any(|d| d == id)
            || v.boxes
                .iter()
                .any(|b| b.id == id && b.origin != Origin::Drift)
    };
    for a in arrows.iter().filter(|a| want(&a.id)) {
        // the spec's own arrow between the pair, of another kind: one arrow per pair
        if let Some(s) = v.arrows.iter().find(|s| {
            s.origin != Origin::Drift && (s.from.as_str(), s.to.as_str()) == (&a.from, &a.to)
        }) {
            println!(
                "skipped {}: ig/ already has a `{}` arrow `{}`; change its kind first",
                a.id,
                name(s.kind),
                s.id
            );
            continue;
        }
        if !in_spec(&a.from) || !in_spec(&a.to) {
            println!("skipped {}: accept its box first", a.id);
            continue;
        }
        // a bundle is written as its main kind
        let kind = a.spec_kind();
        let edit = Edit::Arrow {
            from: a.from.clone(),
            to: a.to.clone(),
            kind,
            // a bundle's label is a derived count, not intent
            label: a.label.clone().filter(|_| a.kind != ArrowKind::Bundle),
        };
        record(files, By::Person, &edit).map_err(|e| anyhow::anyhow!(e.join("\n")))?;
        println!("accepted {}", a.id);
    }
    Ok(0)
}

fn name(k: ArrowKind) -> String {
    let v = serde_json::to_value(k).unwrap_or_default();
    v.as_str().unwrap_or_default().to_string()
}
