//! What `ig view` draws: the map with its code level, every layer's marks and the notices.

use super::code::code_boxes;
use super::file::{self, MapBox, MapFile, refresh};
use super::layers::{Mark, file_layers, sort_marks};
use super::{MapArrow, arrows, facts};
use crate::Env;
use ig_core::model::Graph;
use ig_core::ports::Files;
use serde::Serialize;
use std::collections::{BTreeMap, HashSet};

/// A box's place, in its parent's own inner units.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Frame {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum RegionKind {
    Ring,
    Band,
    Group,
}

/// A labelled area of a parent (an onion ring, a layered band, a hexagon part).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Region {
    pub parent: String,
    pub kind: RegionKind,
    pub label: String,
    pub frame: Frame,
}

/// Where everything is drawn (filled by the layout).
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Placed {
    pub frames: BTreeMap<String, Frame>,
    pub inner: BTreeMap<String, (f32, f32)>,
    pub regions: Vec<Region>,
    pub routes: BTreeMap<String, Vec<(f32, f32)>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct View {
    pub version: u32,
    pub boxes: Vec<MapBox>,
    pub arrows: Vec<MapArrow>,
    pub marks: Vec<Mark>,
    pub placed: Placed,
    pub notices: Vec<String>,
}

/// `m` with its code level and the marks of `.ig/layers`; the notices are those layers'.
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
    let (extra, notices) = file_layers(files, &known);
    let mut marks: Vec<Mark> = m.marks.iter().cloned().chain(extra).collect();
    sort_marks(&mut marks);
    View {
        version: m.version,
        boxes,
        arrows,
        marks,
        placed: Placed::default(),
        notices,
    }
}

/// `ig view`: the facts of the working tree, refreshed into `.ig/map.json` (written back unless
/// the file there was rejected: it is left for its owner to fix), then the view of it.
pub fn run(env: &Env) -> crate::Result<View> {
    let config = crate::config::load(env.files)?;
    let built = crate::graph(env, env.files, &config, env.vcs.and_then(|v| v.head()))?;
    let (g, ws) = (&built.graph, &built.workspace);
    let boxes = facts(&built.workspace, g, &config);
    let findings = ig_core::rules::arch(g, &config, &ws.packages);
    let (old, rejected) = file::load(env.files);
    let (m, reset) = refresh(old.as_ref(), &boxes, arrows(&boxes, g), &findings);
    let mut notices: Vec<String> = config
        .notices
        .iter()
        .map(|k| format!("ig.yaml: `{k}` is no longer read; ignored"))
        .collect();
    if rejected.is_empty() {
        file::save(env.files, &m)?;
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
}
