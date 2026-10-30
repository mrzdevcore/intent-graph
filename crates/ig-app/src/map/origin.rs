//! The spec (`ig/*.kdl`) against the code: each box and arrow is built (in both), to build (in
//! the spec only) or drift (in the code only). Pure: no I/O.

use super::arrows::edge_kind;
use super::file::{MapBox, MapFile, fresh};
use super::{ArrowKind, BoxKind, FactBox, Level, MapArrow, arrows};
use globset::{GlobBuilder, GlobSet, GlobSetBuilder};
use ig_core::model::Graph;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

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
    let depths: Vec<usize> = spec.boxes.iter().map(|b| depth(&parents, &b.id)).collect();
    // One glob set for every box's `at`, each glob back to its box: one match per file.
    let mut set = GlobSetBuilder::new();
    let mut glob_box: Vec<usize> = vec![];
    let mut has_globs = vec![false; spec.boxes.len()];
    for (i, b) in spec.boxes.iter().enumerate() {
        if b.level == Level::System || b.made_of.is_empty() {
            continue;
        }
        let built: Option<Vec<_>> = (b.made_of.iter())
            .map(|g| GlobBuilder::new(g).literal_separator(true).build().ok())
            .collect();
        for g in built.into_iter().flatten() {
            set.add(g);
            glob_box.push(i);
            has_globs[i] = true;
        }
    }
    let set = set.build().unwrap_or_else(|_| GlobSet::empty());
    for b in &mut m.boxes {
        b.nodes.clear();
        b.origin = Origin::Built;
    }
    let mut unclaimed: BTreeSet<&str> = BTreeSet::new();
    let mut matched = vec![false; spec.boxes.len()];
    for n in g.nodes.iter().filter(|n| n.is_module()) {
        let hits: Vec<usize> = set
            .matches(&n.file)
            .into_iter()
            .map(|k| glob_box[k])
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
    // Box origins, deepest first: own globs matching a file decide; otherwise any built child,
    // or none at all (a bare declaration).
    let mut built: Vec<bool> = vec![true; m.boxes.len()];
    let mut order: Vec<usize> = (0..m.boxes.len()).collect();
    order.sort_by_key(|&i| std::cmp::Reverse(depths[i]));
    for i in order {
        let kids: Vec<usize> = (0..m.boxes.len())
            .filter(|&j| m.boxes[j].parent.as_deref() == Some(m.boxes[i].id.as_str()))
            .collect();
        built[i] = match has_globs[i] {
            true => matched[i],
            false => kids.is_empty() || kids.iter().any(|&j| built[j]),
        };
        if !built[i] {
            m.boxes[i].origin = Origin::Build;
        }
    }
    m.boxes.extend(drift(facts, &unclaimed, &root, &m.boxes));
    arrows_against(&mut m, g);
    m
}

/// Each spec arrow is built when a graph edge of its kind (any kind for a `bundle`) runs from
/// the code under its tail into the code under its head; an arrow touching a person is a
/// declaration. Each code arrow (lifted over the boxes) that no spec arrow covers, from its
/// ends or their ancestors, is drift.
fn arrows_against(m: &mut MapFile, g: &Graph) {
    let boxes = &m.boxes;
    let index: HashMap<&str, usize> = (boxes.iter().enumerate())
        .map(|(i, b)| (b.id.as_str(), i))
        .collect();
    let up: Vec<Option<usize>> = (boxes.iter())
        .map(|b| b.parent.as_deref().and_then(|p| index.get(p).copied()))
        .collect();
    // `i` and its ancestors, innermost first (bounded, so a bad parent chain cannot loop)
    let chain = |i: usize| {
        let mut out = vec![i];
        while let Some(p) = up[*out.last().unwrap()].filter(|_| out.len() <= boxes.len()) {
            out.push(p);
        }
        out
    };
    let mut owner: HashMap<&str, usize> = HashMap::new();
    for (i, b) in boxes.iter().enumerate() {
        owner.extend(b.nodes.iter().map(|n| (n.as_str(), i)));
    }
    let box_of = |id: &str| {
        let module = id.split('#').next().unwrap_or(id);
        owner.get(id).or_else(|| owner.get(module)).copied()
    };
    // (tail, head) → edges, their kinds, any in a loop: every pair of ancestors of the ends
    let mut between: HashMap<(usize, usize), (u32, BTreeSet<ArrowKind>, bool)> = HashMap::new();
    for e in &g.edges {
        let (Some(a), Some(b)) = (box_of(&e.from), box_of(&e.to)) else {
            continue;
        };
        let Some(kind) = edge_kind(&e.kind, boxes[b].kind.value) else {
            continue;
        };
        let (ca, cb) = (chain(a), chain(b));
        for &x in &ca {
            for &y in cb.iter().filter(|&&y| y != x) {
                let s = between.entry((x, y)).or_default();
                s.0 += 1;
                s.1.insert(kind);
                s.2 |= e.in_loop;
            }
        }
    }
    let person = |id: &str| {
        index
            .get(id)
            .is_some_and(|&i| boxes[i].kind.value == BoxKind::Person)
    };
    let mut said: HashSet<(usize, usize, ArrowKind)> = HashSet::new();
    for a in &mut m.arrows {
        let ends = index.get(a.from.as_str()).zip(index.get(a.to.as_str()));
        if let Some((&f, &t)) = ends {
            said.insert((f, t, a.kind));
        }
        if person(&a.from) || person(&a.to) {
            a.origin = Origin::Built;
            continue;
        }
        let hit = ends
            .and_then(|(f, t)| between.get(&(*f, *t)))
            .filter(|s| a.kind == ArrowKind::Bundle || s.1.contains(&a.kind));
        a.origin = match hit {
            Some((count, kinds, in_loop)) => {
                (a.count, a.kinds, a.in_loop) = (*count, kinds.iter().copied().collect(), *in_loop);
                Origin::Built
            }
            None => Origin::Build,
        };
    }
    let live: Vec<FactBox> = boxes.iter().map(super::code::as_fact).collect();
    let mut drift = vec![];
    for c in arrows(&live, g) {
        let (Some(&f), Some(&t)) = (index.get(c.from.as_str()), index.get(c.to.as_str())) else {
            continue;
        };
        let (cf, ct) = (chain(f), chain(t));
        let covered = cf.iter().any(|&x| {
            ct.iter().any(|&y| {
                said.contains(&(x, y, ArrowKind::Bundle))
                    || c.kinds.iter().any(|&k| said.contains(&(x, y, k)))
            })
        });
        if !covered {
            drift.push(c);
        }
    }
    for c in drift {
        // the spec's own arrow between the same boxes, of another kind, keeps the plain id
        let id = if m.arrows.iter().any(|a| a.id == c.id) {
            format!("{}~drift", c.id)
        } else {
            c.id.clone()
        };
        m.arrows.push(MapArrow {
            id,
            origin: Origin::Drift,
            ..c
        });
    }
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
    let mut out: Vec<MapBox> = vec![];
    // A container under the root, with a slug id no other box has.
    let place = |mut b: MapBox, out: &mut Vec<MapBox>| {
        let taken = |id: &str| spec.iter().chain(out.iter()).any(|b| b.id == id);
        let base = b.id.replace('/', "-");
        let mut id = base.clone();
        let mut k = 1;
        while taken(&id) {
            id = if k == 1 {
                format!("{base}-drift")
            } else {
                format!("{base}-drift-{k}")
            };
            k += 1;
        }
        b.id = id;
        b.parent = Some(root.into());
        b.level = Level::Container;
        b.slot = None;
        b.origin = Origin::Drift;
        out.push(b);
    };
    let mut covered: BTreeSet<&str> = BTreeSet::new();
    for f in &order {
        let all = !f.nodes.is_empty() && f.nodes.iter().all(|n| unclaimed.contains(n.as_str()));
        if !all || f.nodes.iter().any(|n| covered.contains(n.as_str())) {
            continue;
        }
        covered.extend(f.nodes.iter().map(String::as_str));
        place(fresh(f), &mut out);
    }
    // Left over: grouped by the deepest fact box holding each file.
    let mut left: BTreeMap<Option<&str>, Vec<&str>> = BTreeMap::new();
    for n in unclaimed.iter().filter(|n| !covered.contains(*n)) {
        let home = order.iter().rev().find(|f| f.nodes.iter().any(|x| x == n));
        left.entry(home.map(|f| f.id.as_str())).or_default().push(n);
    }
    for (home, files) in left {
        let folder = common_folder(&files);
        let id = home.unwrap_or("code").to_string();
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
        place(fresh(&f), &mut out);
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
    plan_of(&m.boxes, &m.arrows)
}

/// `plan` over a view's boxes and arrows.
pub fn plan_of(boxes: &[MapBox], arrows: &[MapArrow]) -> Plan {
    let mut p = Plan {
        build: vec![],
        drift: vec![],
    };
    for b in boxes {
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
                Some(g) => format!("drift: code under {g} is in no box ({})", b.id),
                None => format!(
                    "drift: {} {} is in the code, not in ig/",
                    name(&b.kind.value),
                    b.id
                ),
            }),
        }
    }
    for a in arrows {
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
