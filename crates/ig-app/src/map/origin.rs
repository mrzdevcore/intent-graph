//! The spec (`ig/*.kdl`) against the code: each box and arrow is built (in both), to build (in
//! the spec only) or drift (in the code only). Pure: no I/O.

use super::arrows::{SEEN, edge_kind};
use super::facts::slug;
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
    /// In the spec at a new place (`at` changed since the last commit), its code still at the old.
    Moved,
    /// Running, on a connector's word, with nothing in `ig/` describing it.
    Unmanaged,
}

pub fn is_built(o: &Origin) -> bool {
    *o == Origin::Built
}

pub(super) fn glob(g: &str) -> Option<globset::Glob> {
    GlobBuilder::new(g).literal_separator(true).build().ok()
}

/// Some file of `files` under `globs`.
fn holds(globs: &[String], files: &[&str]) -> bool {
    let mut b = GlobSetBuilder::new();
    globs.iter().filter_map(|g| glob(g)).for_each(|g| {
        b.add(g);
    });
    let set = b.build().unwrap_or_else(|_| GlobSet::empty());
    files.iter().any(|f| set.is_match(f))
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
/// box of the same id. A box whose `at` differs from `prior`'s (the last commit's spec), with no
/// code at its new place and some at its old, is moved: it claims the code at its old place.
pub fn compare(spec: &MapFile, facts: &[FactBox], g: &Graph, prior: Option<&MapFile>) -> MapFile {
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
    let modules: Vec<&str> = (g.nodes.iter().filter(|n| n.is_module()))
        .map(|n| n.file.as_str())
        .collect();
    let moved: Vec<Option<&Vec<String>>> = (spec.boxes.iter())
        .map(|b| {
            let old = prior?.boxes.iter().find(|p| p.id == b.id)?;
            let moved = b.level != Level::System
                && !b.made_of.is_empty()
                && !old.made_of.is_empty()
                && old.made_of != b.made_of
                && !holds(&b.made_of, &modules)
                && holds(&old.made_of, &modules);
            moved.then_some(&old.made_of)
        })
        .collect();
    // One glob set for every box's `at`, each glob back to its box: one match per file.
    let mut set = GlobSetBuilder::new();
    let mut glob_box: Vec<usize> = vec![];
    let mut has_globs = vec![false; spec.boxes.len()];
    for (i, b) in spec.boxes.iter().enumerate() {
        if b.level == Level::System || b.made_of.is_empty() {
            continue;
        }
        // a moved box's code is at its old place
        let at = moved[i].unwrap_or(&b.made_of);
        let built: Option<Vec<_>> = at.iter().map(|g| glob(g)).collect();
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
        if let Some(old) = moved[i] {
            m.boxes[i].origin = Origin::Moved;
            m.boxes[i].moved_from = old.clone();
        } else if !built[i] {
            m.boxes[i].origin = Origin::Build;
        }
    }
    m.boxes.extend(drift(facts, &unclaimed, &root, &m.boxes));
    arrows_against(&mut m, g);
    m
}

type Between = (u32, BTreeSet<ArrowKind>, bool, Vec<String>);

/// Each spec arrow is built when a graph edge of its kind (any kind for a `bundle`) runs from
/// the code under its tail into the code under its head; an arrow touching a person or a box
/// that `runs` is a declaration. Each code arrow (lifted over the boxes) that no spec arrow
/// covers, from its ends or their ancestors, is drift.
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
    // shallowest first, so the deepest box claims a node (as `arrows` does)
    let mut owner: HashMap<&str, usize> = HashMap::new();
    let mut shallow_first: Vec<usize> = (0..boxes.len()).collect();
    shallow_first.sort_by_key(|&i| chain(i).len());
    for i in shallow_first {
        owner.extend(boxes[i].nodes.iter().map(|n| (n.as_str(), i)));
    }
    let box_of = |id: &str| {
        let module = id.split('#').next().unwrap_or(id);
        owner.get(id).or_else(|| owner.get(module)).copied()
    };
    // (tail, head) → edges, their kinds, any in a loop, where: every pair of ancestors of the ends
    let mut between: HashMap<(usize, usize), Between> = HashMap::new();
    // each edge's ends' chains and kind
    let mut edges: Vec<(Vec<usize>, Vec<usize>, ArrowKind)> = vec![];
    for e in &g.edges {
        let (Some(a), Some(b)) = (box_of(&e.from), box_of(&e.to)) else {
            continue;
        };
        let Some(kind) = edge_kind(&e.kind, boxes[b].kind.value) else {
            continue;
        };
        let (ca, cb) = (chain(a), chain(b));
        let at = format!("{}:{}", e.file, e.line);
        edges.push((ca.clone(), cb.clone(), kind));
        for &x in &ca {
            // never a pair where one end holds the other (or the system root)
            for &y in cb.iter().filter(|&&y| !ca.contains(&y) && !cb.contains(&x)) {
                let s = between.entry((x, y)).or_default();
                s.0 += 1;
                s.1.insert(kind);
                s.2 |= e.in_loop;
                if s.3.len() < SEEN && !s.3.contains(&at) {
                    s.3.push(at.clone());
                }
            }
        }
    }
    let declared = |id: &str| {
        index.get(id).is_some_and(|&i| {
            boxes[i].kind.value == BoxKind::Person || boxes[i].props.contains_key("runs")
        })
    };
    let mut said: HashSet<(usize, usize, ArrowKind)> = HashSet::new();
    // the arrows of a moved box: its code, at the old place, is lifted over the new one
    let mut moved: HashSet<(usize, usize, ArrowKind)> = HashSet::new();
    let is_moved = |i: usize| boxes[i].origin == Origin::Moved;
    for a in &mut m.arrows {
        let ends = index.get(a.from.as_str()).zip(index.get(a.to.as_str()));
        if let Some((&f, &t)) = ends {
            said.insert((f, t, a.kind));
            if is_moved(f) || is_moved(t) {
                moved.insert((f, t, a.kind));
            }
        }
        if declared(&a.from) || declared(&a.to) {
            a.origin = Origin::Built;
            continue;
        }
        let hit = ends
            .and_then(|(f, t)| between.get(&(*f, *t)))
            .filter(|s| a.kind == ArrowKind::Bundle || s.1.contains(&a.kind));
        a.origin = match hit {
            Some((count, kinds, in_loop, at)) => {
                (a.count, a.kinds, a.in_loop) = (*count, kinds.iter().copied().collect(), *in_loop);
                a.seen = at.clone();
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
        let says = |set: &HashSet<_>, cf: &[usize], ct: &[usize], kinds: &[ArrowKind]| {
            cf.iter().any(|&x| {
                ct.iter()
                    .filter(|&&y| !cf.contains(&y) && !ct.contains(&x))
                    .any(|&y| {
                        set.contains(&(x, y, ArrowKind::Bundle))
                            || kinds.iter().any(|&k| set.contains(&(x, y, k)))
                    })
            })
        };
        if says(&said, &cf, &ct, &c.kinds) {
            continue;
        }
        if moved.is_empty() {
            drift.push(c);
            continue;
        }
        // lifted over a moved box's new place: the code under it, box pair by box pair, is
        // what that box's arrows say (any of a pair's kinds, as for the arrow itself)
        let mut under: BTreeMap<(usize, usize), Vec<ArrowKind>> = BTreeMap::new();
        for (ca, cb, k) in edges
            .iter()
            .filter(|(a, b, _)| a.contains(&f) && b.contains(&t))
        {
            under.entry((ca[0], cb[0])).or_default().push(*k);
        }
        let of_a_move = !under.is_empty()
            && (under.iter()).all(|(&(a, b), kinds)| says(&moved, &chain(a), &chain(b), kinds));
        if !of_a_move {
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
        let base = slug(&b.id, "drift");
        let mut id = base.clone();
        let mut k = 1;
        while taken(&id) {
            let suffix = if k == 1 {
                "-drift".to_string()
            } else {
                format!("-drift-{k}")
            };
            // the base gives way, so the id stays a valid slug (at most 64 chars)
            let mut head = base.clone();
            head.truncate(64 - suffix.len());
            id = format!("{}{suffix}", head.trim_end_matches('-'));
            k += 1;
        }
        b.id = id;
        // a long file name makes a long title; the map allows 60
        b.title.value = b.title.value.chars().take(60).collect();
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
    // Left over: where the spec claims nothing, the highest folder with no claimed code.
    let claimed: BTreeSet<&str> = spec
        .iter()
        .flat_map(|b| b.nodes.iter().map(String::as_str))
        .collect();
    let left: Vec<&str> = (unclaimed.iter().copied())
        .filter(|n| !covered.contains(n))
        .collect();
    for (id, title, at, files) in left_over(&claimed, &left) {
        let f = FactBox {
            id: id.clone(),
            parent: Some(root.into()),
            level: Level::Container,
            made_of: at,
            package: None,
            files: files.len() as u32,
            nodes: files,
            guess: super::Guess {
                kind: super::BoxKind::Group,
                title,
                pattern: None,
                slot: None,
                logo: None,
            },
        };
        place(fresh(&f), &mut out);
    }
    out
}

/// A drift box for left-over files: (id, title, `at`, files).
type Piece = (String, String, Vec<String>, Vec<String>);

/// Groups the unclaimed `left` files by the highest folder holding no claimed code (`dir/**`).
/// A file whose every folder holds claimed code is a loose file: its folder's direct files go
/// in one `dir/*` box when none of them is claimed, else each is listed by its own path.
fn left_over(claimed: &BTreeSet<&str>, left: &[&str]) -> Vec<Piece> {
    let holds = |dir: &str| {
        let p = format!("{dir}/");
        claimed.iter().any(|c| c.starts_with(&p))
    };
    let mut folders: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut loose: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for f in left {
        let parts: Vec<&str> = f.split('/').collect();
        let dirs = &parts[..parts.len() - 1];
        match (1..=dirs.len())
            .map(|i| dirs[..i].join("/"))
            .find(|d| !holds(d))
        {
            Some(d) => folders.entry(d).or_default().push(f.to_string()),
            None => loose.entry(dirs.join("/")).or_default().push(f.to_string()),
        }
    }
    let mut out: Vec<Piece> = folders
        .into_iter()
        .map(|(d, files)| {
            let title = d.rsplit('/').next().unwrap_or(&d).to_string();
            (
                d.clone(),
                title,
                vec![format!("{}/**", globset::escape(&d))],
                files,
            )
        })
        .collect();
    for (dir, files) in loose {
        let direct = |c: &&str| match c.rsplit_once('/') {
            Some((d, _)) => d == dir,
            None => dir.is_empty(),
        };
        let prefix = if dir.is_empty() {
            String::new()
        } else {
            format!("{dir}/")
        };
        if !claimed.iter().any(direct) {
            let (id, title) = match dir.is_empty() {
                true => ("root-files".to_string(), "root files".to_string()),
                false => (
                    dir.clone(),
                    format!("{} files", dir.rsplit('/').next().unwrap_or(&dir)),
                ),
            };
            out.push((
                id,
                title,
                vec![format!("{}*", globset::escape(&prefix))],
                files,
            ));
            continue;
        }
        for f in files {
            let file = f.rsplit('/').next().unwrap_or(&f);
            let title = file.rsplit_once('.').map_or(file, |(t, _)| t);
            let title = if title.is_empty() { file } else { title }.to_string();
            out.push((f.clone(), title, vec![globset::escape(&f)], vec![f]));
        }
    }
    out
}

/// The `at` of each group of `unread` files (the code ig does not read), by the highest folder
/// holding none of the `read` ones, as for left-over drift.
pub fn not_read(read: &BTreeSet<&str>, unread: &[&str]) -> Vec<String> {
    (left_over(read, unread).into_iter())
        .map(|(_, _, at, _)| at.join(", "))
        .collect()
}

mod plan;
pub use plan::{Item, Plan, Step, plan, plan_of};

#[cfg(test)]
mod tests;
