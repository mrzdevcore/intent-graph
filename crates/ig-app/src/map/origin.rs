//! The spec (`ig/*.kdl`) against the code: each box and arrow is built (in both), to build (in
//! the spec only) or drift (in the code only). Pure: no I/O.

use super::file::{MapBox, MapFile, fresh, globs};
use super::{ArrowKind, FactBox, Level, MapArrow, arrows};
use ig_core::model::Graph;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Origin {
    #[default]
    Built,
    Build,
    Drift,
}

pub fn is_built(o: &Origin) -> bool {
    *o == Origin::Built
}

/// What is left to build and what the code does that the spec does not say, one line each.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    pub build: Vec<String>,
    pub drift: Vec<String>,
}

/// Parent hops from `id` to the root (bounded, so a bad parent chain cannot loop).
fn depth<'a>(parents: &HashMap<&'a str, Option<&'a str>>, id: &'a str) -> usize {
    let mut d = 0;
    let mut at = parents.get(id).copied().flatten();
    while let Some(p) = at.filter(|_| d <= parents.len()) {
        d += 1;
        at = parents.get(p).copied().flatten();
    }
    d
}

/// The spec's boxes and arrows, each with its origin, plus the code's drift. A module file is
/// claimed by the deepest spec box whose `at` matches it; the system's own `at` claims nothing
/// (it is the whole repo). A fact box without globs (an outside system) is claimed by the spec
/// box of the same id.
pub fn compare(spec: &MapFile, facts: &[FactBox], g: &Graph) -> MapFile {
    let mut m = spec.clone();
    let root = m
        .boxes
        .iter()
        .find(|b| b.level == Level::System)
        .map_or("system".to_string(), |b| b.id.clone());
    let parents: HashMap<&str, Option<&str>> = spec
        .boxes
        .iter()
        .map(|b| (b.id.as_str(), b.parent.as_deref()))
        .collect();
    let sets: Vec<_> = spec
        .boxes
        .iter()
        .map(|b| {
            (b.level != Level::System && !b.made_of.is_empty())
                .then(|| globs(&b.made_of).ok())
                .flatten()
        })
        .collect();
    let depths: Vec<usize> = spec.boxes.iter().map(|b| depth(&parents, &b.id)).collect();
    for b in &mut m.boxes {
        b.nodes.clear();
        b.origin = Origin::Built;
    }
    let mut unclaimed: BTreeSet<&str> = BTreeSet::new();
    let mut matched = vec![false; sets.len()];
    for n in g.nodes.iter().filter(|n| n.is_module()) {
        let hits: Vec<usize> = (0..sets.len())
            .filter(|&i| sets[i].as_ref().is_some_and(|s| s.is_match(&n.file)))
            .collect();
        hits.iter().for_each(|&i| matched[i] = true);
        match hits.into_iter().max_by_key(|&i| depths[i]) {
            Some(i) => m.boxes[i].nodes.push(n.id.clone()),
            None => {
                unclaimed.insert(&n.id);
            }
        }
    }
    for f in facts.iter().filter(|f| f.made_of.is_empty()) {
        match m.boxes.iter_mut().find(|b| b.id == f.id) {
            Some(b) => b.nodes.extend(f.nodes.iter().cloned()),
            None => unclaimed.extend(f.nodes.iter().map(String::as_str)),
        }
    }
    // Box origins: own globs matching a file decide; otherwise any built descendant, or a bare
    // declaration.
    let own: Vec<Option<bool>> = (0..m.boxes.len())
        .map(|i| sets[i].as_ref().map(|_| matched[i]))
        .collect();
    for i in 0..m.boxes.len() {
        let id = &m.boxes[i].id;
        let below: Vec<usize> = (0..m.boxes.len())
            .filter(|&j| j != i && is_below(&parents, &m.boxes[j].id, id))
            .collect();
        let built = own[i]
            .unwrap_or_else(|| below.is_empty() || below.iter().any(|&j| own[j] == Some(true)));
        if !built {
            m.boxes[i].origin = Origin::Build;
        }
    }
    m.boxes.extend(drift(facts, &unclaimed, &root, &m.boxes));
    // Arrows: the code's, lifted over these boxes, against the spec's.
    let live: Vec<FactBox> = m.boxes.iter().map(super::code::as_fact).collect();
    let code = arrows(&live, g);
    let mut used = vec![false; code.len()];
    for a in &mut m.arrows {
        let hit = code.iter().position(|c| {
            c.from == a.from && c.to == a.to && (c.kind == a.kind || c.kinds.contains(&a.kind))
        });
        a.origin = match hit {
            Some(k) => {
                used[k] = true;
                let c = &code[k];
                (a.parent, a.count, a.kinds, a.in_loop) =
                    (c.parent.clone(), c.count, c.kinds.clone(), c.in_loop);
                Origin::Built
            }
            None => Origin::Build,
        };
    }
    for (c, _) in code.into_iter().zip(used).filter(|(_, u)| !u) {
        m.arrows.push(MapArrow {
            origin: Origin::Drift,
            ..c
        });
    }
    m
}

fn is_below(parents: &HashMap<&str, Option<&str>>, id: &str, of: &str) -> bool {
    let mut at = parents.get(id).copied().flatten();
    let mut hops = 0;
    while let Some(p) = at.filter(|_| hops <= parents.len()) {
        if p == of {
            return true;
        }
        hops += 1;
        at = parents.get(p).copied().flatten();
    }
    false
}

/// Drift boxes under `root`: the highest fact boxes holding only unclaimed nodes; unclaimed
/// files left over (in a box that also holds claimed ones) by their common folder.
fn drift(
    facts: &[FactBox],
    unclaimed: &BTreeSet<&str>,
    root: &str,
    spec: &[MapBox],
) -> Vec<MapBox> {
    let parents: HashMap<&str, Option<&str>> = facts
        .iter()
        .map(|f| (f.id.as_str(), f.parent.as_deref()))
        .collect();
    let mut order: Vec<&FactBox> = facts.iter().filter(|f| f.level != Level::System).collect();
    order.sort_by_key(|f| depth(&parents, &f.id));
    let taken = |id: &str, out: &[MapBox]| spec.iter().chain(out).any(|b| b.id == id);
    let mut out: Vec<MapBox> = vec![];
    let mut covered: BTreeSet<&str> = BTreeSet::new();
    for f in &order {
        let all = !f.nodes.is_empty() && f.nodes.iter().all(|n| unclaimed.contains(n.as_str()));
        if !all || f.nodes.iter().any(|n| covered.contains(n.as_str())) {
            continue;
        }
        covered.extend(f.nodes.iter().map(String::as_str));
        let mut b = fresh(f);
        if taken(&b.id, &out) {
            b.id = format!("{}-drift", b.id);
        }
        b.parent = Some(root.into());
        b.slot = None;
        b.origin = Origin::Drift;
        out.push(b);
    }
    // Left over: grouped by the deepest fact box holding each file.
    let mut left: BTreeMap<Option<&str>, Vec<&str>> = BTreeMap::new();
    for n in unclaimed.iter().filter(|n| !covered.contains(*n)) {
        let home = order.iter().rev().find(|f| f.nodes.iter().any(|x| x == n));
        left.entry(home.map(|f| f.id.as_str())).or_default().push(n);
    }
    for (home, files) in left {
        let folder = common_folder(&files);
        let id = format!("{}-drift", home.unwrap_or("code"));
        let f = FactBox {
            id: id.clone(),
            parent: Some(root.into()),
            level: Level::Container,
            made_of: vec![if folder.is_empty() {
                "**".into()
            } else {
                format!("{folder}/**")
            }],
            package: None,
            files: files.len() as u32,
            nodes: files.iter().map(|s| s.to_string()).collect(),
            guess: super::Guess {
                kind: super::BoxKind::Group,
                title: id,
                pattern: None,
                slot: None,
                logo: None,
            },
        };
        let mut b = fresh(&f);
        b.origin = Origin::Drift;
        out.push(b);
    }
    out
}

/// The deepest folder holding every one of `files`.
fn common_folder(files: &[&str]) -> String {
    let dirs: Vec<Vec<&str>> = files
        .iter()
        .map(|f| {
            let mut p: Vec<&str> = f.split('/').collect();
            p.pop();
            p
        })
        .collect();
    let Some(first) = dirs.first() else {
        return String::new();
    };
    let n = (0..first.len())
        .take_while(|&i| dirs.iter().all(|d| d.get(i) == Some(&first[i])))
        .count();
    first[..n].join("/")
}

fn name<T: Serialize>(v: &T) -> String {
    serde_json::to_value(v)
        .ok()
        .and_then(|v| v.as_str().map(String::from))
        .unwrap_or_default()
}

/// One line per box or arrow not built, sorted.
pub fn plan(m: &MapFile) -> Plan {
    let mut p = Plan {
        build: vec![],
        drift: vec![],
    };
    for b in &m.boxes {
        match b.origin {
            Origin::Built => {}
            Origin::Build if b.made_of.is_empty() => {
                p.build
                    .push(format!("to build: {} {}", name(&b.kind.value), b.id))
            }
            Origin::Build => p.build.push(format!(
                "to build: {} {} ({})",
                name(&b.kind.value),
                b.id,
                b.made_of.join(", ")
            )),
            Origin::Drift => p.drift.push(match b.made_of.first() {
                Some(g) => format!("drift: code under {g} is in no box"),
                None => format!(
                    "drift: {} {} is in the code, not in ig/",
                    name(&b.kind.value),
                    b.id
                ),
            }),
        }
    }
    for a in &m.arrows {
        let (from, kind, to) = (&a.from, name::<ArrowKind>(&a.kind), &a.to);
        match a.origin {
            Origin::Built => {}
            Origin::Build => p.build.push(format!("to build: {from} {kind} {to}")),
            Origin::Drift => p
                .drift
                .push(format!("drift: {from} {kind} {to}, not in ig/")),
        }
    }
    p.build.sort();
    p.drift.sort();
    p
}

#[cfg(test)]
mod tests;
