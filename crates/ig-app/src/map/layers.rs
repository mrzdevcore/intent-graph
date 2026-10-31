//! Layers: a word (a `Mark`) on a box or an arrow. ig computes two of them, `rules` and `risks`,
//! and stores them in the map; others come from `.ig/layers/*.json` and are only shown.

use super::facts::Role;
use super::file::{MapBox, MapFile, slots};
use super::{ArrowKind, BoxKind, MapArrow, Pattern, arrows};
use ig_core::config::Severity;
use ig_core::model::{Edge, Finding, Graph};
use ig_core::ports::Files;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MarkState {
    Ok,
    Warn,
    Bad,
    Down,
}

/// A layer's word on a box or an arrow.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Mark {
    pub layer: String,
    pub target: String,
    pub state: MarkState,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub badge: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub value: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub note: Option<String>,
}

fn mark(layer: &str, target: &str, state: MarkState, badge: String) -> Mark {
    Mark {
        layer: layer.into(),
        target: target.into(),
        state,
        badge: Some(badge),
        value: None,
        note: None,
    }
}

/// Sorted by layer, target, badge (the map file is committed: its order must not move).
pub fn sort_marks(marks: &mut [Mark]) {
    marks.sort_by(|a, b| (&a.layer, &a.target, &a.badge).cmp(&(&b.layer, &b.target, &b.badge)));
}

/// The `rules` layer: imports that break the parent's pattern (an onion ring reaching outward,
/// a layered band reaching up), and each `ig.yaml` finding on the arrow carrying its edge.
pub fn rules(m: &MapFile, findings: &[Finding]) -> Vec<Mark> {
    let by_id: HashMap<&str, &MapBox> = m.boxes.iter().map(|b| (b.id.as_str(), b)).collect();
    let mut out = vec![];
    // only uses (imports) break a pattern: a call through a port dispatches outward at run time
    for a in m
        .arrows
        .iter()
        .filter(|a| a.kinds.contains(&ArrowKind::Uses))
    {
        let (Some(p), Some(f), Some(t)) = (
            by_id.get(a.parent.as_str()),
            by_id.get(a.from.as_str()),
            by_id.get(a.to.as_str()),
        ) else {
            continue;
        };
        let pattern = p.pattern.as_ref().map(|f| &f.value);
        let order = slots(pattern);
        let rank = |b: &'_ MapBox| -> Option<(usize, String)> {
            let s = b.slot.as_ref()?;
            Some((order.iter().position(|o| *o == s.value)?, s.value.clone()))
        };
        let (Some((x, from)), Some((y, to))) = (rank(f), rank(t)) else {
            continue;
        };
        let bad = match pattern {
            Some(Pattern::Onion) => x < y,
            Some(Pattern::Layered { .. }) => x > y,
            _ => false,
        };
        if bad {
            let badge = format!("breaks: {from} → {to}");
            out.push(mark("rules", &a.id, MarkState::Bad, badge));
        }
    }
    let facts: Vec<_> = m.boxes.iter().map(super::code::as_fact).collect();
    let drawn: HashSet<&str> = m.arrows.iter().map(|a| a.id.as_str()).collect();
    // ponytail: one lift per finding (O(findings × boxes)); batch them if rule findings grow large
    for f in findings {
        let Some(e) = &f.edge else { continue };
        let mut g = Graph::empty();
        g.edges = vec![Edge {
            from: e.from.clone(),
            to: e.to.clone(),
            kind: "imports".into(),
            confidence: "resolved".into(),
            file: f.file.clone(),
            line: f.line,
            namespace: false,
            reexport: false,
            in_loop: false,
        }];
        let Some(a) = arrows(&facts, &g)
            .pop()
            .filter(|a| drawn.contains(a.id.as_str()))
        else {
            continue;
        };
        let state = match f.severity {
            Severity::Error => MarkState::Bad,
            _ => MarkState::Warn,
        };
        let mut k = mark("rules", &a.id, state, f.rule_id.clone());
        k.note = Some(f.message.clone());
        if !out.contains(&k) {
            out.push(k);
        }
    }
    sort_marks(&mut out);
    out
}

/// The `risks` layer: a loop calling into a store, a port, an external system or the
/// infrastructure (where the I/O lives), and an app whose requests chain over 3+ network hops.
pub fn risks(m: &MapFile) -> Vec<Mark> {
    let by_id: HashMap<&str, &MapBox> = m.boxes.iter().map(|b| (b.id.as_str(), b)).collect();
    let mut out = vec![];
    // a call into a block that runs on its own crosses the network
    let network = |a: &MapArrow| {
        a.kinds.contains(&ArrowKind::Calls)
            && (by_id.get(a.to.as_str())).is_some_and(|t| t.kind.value.role() == Role::Run)
    };
    for a in m.arrows.iter().filter(|a| a.in_loop) {
        let io = by_id.get(a.to.as_str()).is_some_and(|t| {
            matches!(
                t.kind.value,
                BoxKind::Store | BoxKind::Port | BoxKind::External
            ) || t
                .slot
                .as_ref()
                .is_some_and(|s| matches!(s.value.as_str(), "infrastructure" | "adapters"))
        });
        if io || network(a) {
            let badge = "risk: query in a loop".to_string();
            out.push(mark("risks", &a.id, MarkState::Warn, badge));
        }
    }
    let mut net: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for a in m.arrows.iter().filter(|a| network(a)) {
        net.entry(&a.from).or_default().push(&a.to);
    }
    /// Arrows in the longest chain from `at` that visits no box twice.
    fn hops<'a>(
        net: &BTreeMap<&'a str, Vec<&'a str>>,
        at: &'a str,
        seen: &mut Vec<&'a str>,
    ) -> usize {
        seen.push(at);
        let mut best = 0;
        for to in net.get(at).into_iter().flatten() {
            if !seen.contains(to) {
                best = best.max(1 + hops(net, to, seen));
            }
        }
        seen.pop();
        best
    }
    for b in m.boxes.iter().filter(|b| b.kind.value == BoxKind::App) {
        let n = hops(&net, &b.id, &mut vec![]);
        if n >= 3 {
            out.push(mark(
                "risks",
                &b.id,
                MarkState::Warn,
                format!("{n} network hops"),
            ));
        }
    }
    sort_marks(&mut out);
    out
}

const LAYERS: &str = ".ig/layers";
const MAX_LAYER_BYTES: usize = 512 * 1024;
const MAX_LAYER_MARKS: usize = 5_000;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LayerFile {
    layer: String,
    marks: Vec<serde_json::Value>,
}

/// The marks of `.ig/layers/*.json` (each `{ "layer": "...", "marks": [...] }`) whose target is
/// in `known`; every file or mark left out gives a notice.
pub fn file_layers(files: &dyn Files, known: &HashSet<&str>) -> (Vec<Mark>, Vec<String>) {
    let dir = files.at(Path::new(LAYERS));
    let (mut out, mut notices) = (vec![], vec![]);
    let names = dir.glob(&["*.json".into()]).unwrap_or_default();
    for name in names {
        let path = format!("{LAYERS}/{name}");
        let Some(bytes) = dir.read_bytes(&name) else {
            continue;
        };
        if bytes.len() > MAX_LAYER_BYTES {
            notices.push(format!("{path} is over 512 KB; ignored"));
            continue;
        }
        let f: LayerFile = match serde_json::from_slice(&bytes) {
            Ok(f) => f,
            Err(e) => {
                notices.push(format!("{path}: {e}; ignored"));
                continue;
            }
        };
        if f.marks.len() > MAX_LAYER_MARKS {
            notices.push(format!("{path} has over {MAX_LAYER_MARKS} marks; ignored"));
            continue;
        }
        for mut v in f.marks {
            if let Some(o) = v.as_object_mut() {
                o.insert("layer".into(), f.layer.clone().into());
            }
            match serde_json::from_value::<Mark>(v) {
                Ok(k) if known.contains(k.target.as_str()) => out.push(k),
                Ok(k) => notices.push(format!(
                    "{path}: no box or arrow `{}`; mark dropped",
                    k.target
                )),
                Err(e) => notices.push(format!("{path}: {e}; mark dropped")),
            }
        }
    }
    sort_marks(&mut out);
    (out, notices)
}

#[cfg(test)]
mod tests;
