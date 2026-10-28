//! What `ig view` draws: the map with its code level, every layer's marks and the notices.

use super::agent::{Ask, Evidence, offer};
use super::code::code_boxes;
use super::file::{self, MapBox, MapFile, refresh};
use super::layers::{Mark, file_layers, sort_marks};
use super::layout::layout;
pub use super::layout::{Frame, Placed, Region, RegionKind};
use super::{MapArrow, arrows, facts};
use crate::Env;
use ig_core::model::Graph;
use ig_core::ports::Files;
use serde::Serialize;
use std::collections::HashSet;

#[derive(Debug, Clone, Serialize)]
pub struct View {
    pub version: u32,
    pub boxes: Vec<MapBox>,
    pub arrows: Vec<MapArrow>,
    pub marks: Vec<Mark>,
    pub placed: Placed,
    pub notices: Vec<String>,
}

/// `m` with its code level, the marks of `.ig/layers` and where everything is drawn; the
/// notices are those layers' and the layout's.
pub fn view(files: &dyn Files, m: &MapFile, g: &Graph) -> View {
    let (code, code_arrows) = code_boxes(m, g);
    let mut boxes: Vec<MapBox> = m.boxes.iter().cloned().chain(code).collect();
    boxes.sort_by(|a, b| a.id.cmp(&b.id));
    let mut arrows: Vec<MapArrow> = m.arrows.iter().cloned().chain(code_arrows).collect();
    arrows.sort_by(|a, b| a.id.cmp(&b.id));
    let known: HashSet<&str> = boxes
        .iter()
        .map(|b| b.id.as_str())
        .chain(arrows.iter().map(|a| a.id.as_str()))
        .collect();
    let (extra, mut notices) = file_layers(files, &known);
    let mut marks: Vec<Mark> = m.marks.iter().cloned().chain(extra).collect();
    sort_marks(&mut marks);
    let (placed, mut unrouted) = layout(&boxes, &arrows);
    notices.append(&mut unrouted);
    View {
        version: m.version,
        boxes,
        arrows,
        marks,
        placed,
        notices,
    }
}

/// `ig view` without an agent: see `run_with`.
pub fn run(env: &Env) -> crate::Result<View> {
    run_with(env, None)
}

/// `ig view`: the facts of the working tree, refreshed into `.ig/map.json` and, with `ask`, read
/// by an AI agent (written back unless the file there was rejected: it is left for its owner to
/// fix), then the view of it.
pub fn run_with(env: &Env, ask: Option<&mut Ask>) -> crate::Result<View> {
    let config = crate::config::load(env.files)?;
    let built = crate::graph(env, env.files, &config, env.vcs.and_then(|v| v.head()))?;
    let (g, ws) = (&built.graph, &built.workspace);
    let boxes = facts(&built.workspace, g, &config);
    let findings = ig_core::rules::arch(g, &config, &ws.packages);
    let (old, rejected) = file::load(env.files);
    let (mut m, reset) = refresh(old.as_ref(), &boxes, arrows(&boxes, g), &findings);
    let mut notices: Vec<String> = config
        .notices
        .iter()
        .map(|k| format!("ig.yaml: `{k}` is no longer read; ignored"))
        .collect();
    if rejected.is_empty() {
        if let Some(a) = ask {
            let ev = Evidence::new(&boxes, &built.extras.imports, &config);
            notices.extend(offer(a, &mut m, &ev, &config, env.files, env.progress));
        }
        notices.extend(save_valid(env.files, &m)?);
    } else {
        notices.extend(rejected);
        notices.push(format!(
            "{} left as is; fix or delete it to let ig rewrite it",
            file::MAP
        ));
    }
    notices.extend(reset);
    let mut v = view(env.files, &m, g);
    notices.append(&mut v.notices);
    v.notices = notices;
    Ok(v)
}

/// Saves `m` unless `validate` rejects it (the next run would drop the whole file): then the
/// notice why.
fn save_valid(files: &dyn Files, m: &MapFile) -> crate::Result<Option<String>> {
    if let Err(e) = file::validate(m) {
        return Ok(Some(format!(
            "{} not written, ig's map is not valid: {}",
            file::MAP,
            e.join("; ")
        )));
    }
    file::save(files, m)?;
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{Fake, env};
    use ig_core::testing::MemFiles;

    #[test]
    fn notices_reach_the_view_and_a_rejected_map_is_kept() {
        let files = MemFiles::with(&[
            ("ig.yaml", "complexityMax: 9\n"),
            ("src/a.ts", "export function f() {}\n"),
            (file::MAP, "{ not json"),
        ]);
        let fe = Fake;
        let v = run(&env(&files, None, &fe)).unwrap();
        assert!(v.notices[0].contains("complexityMax"), "{:?}", v.notices);
        assert!(
            v.notices.iter().any(|n| n.contains("left as is")),
            "{:?}",
            v.notices
        );
        assert_eq!(files.read(file::MAP).as_deref(), Some("{ not json"));
        let fresh = MemFiles::with(&[("src/a.ts", "export function f() {}\n")]);
        run(&env(&fresh, None, &fe)).unwrap();
        assert!(file::load(&fresh).0.is_some(), "a missing map is written");
    }

    #[test]
    fn an_invalid_map_is_never_written() {
        let files = MemFiles::with(&[]);
        let mut m = MapFile {
            version: 2,
            boxes: vec![],
            arrows: vec![],
            marks: vec![],
        };
        let n = save_valid(&files, &m).unwrap().unwrap();
        assert!(n.contains("not written") && n.contains("version 2"), "{n}");
        assert!(files.read(file::MAP).is_none());
        m.version = 1;
        assert_eq!(save_valid(&files, &m).unwrap(), None);
        assert!(files.read(file::MAP).is_some());
    }
}
