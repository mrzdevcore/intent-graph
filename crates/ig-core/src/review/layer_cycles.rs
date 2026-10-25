//! `struct/cycle` between declared layers.

use super::module_graph::{ModuleGraph, tarjan};
use super::{Ctx, ReviewItem, item};
use crate::rules::Layers;
use std::collections::{BTreeMap, HashMap, HashSet};

pub(super) fn layer_cycles(mg: &ModuleGraph, ctx: &Ctx) -> Vec<ReviewItem> {
    let layers = Layers::new(ctx.config, ctx.packages);
    let names: Vec<&str> = ctx.config.layers.iter().map(|(n, _)| n.as_str()).collect();
    let li: HashMap<&str, usize> = names.iter().enumerate().map(|(i, n)| (*n, i)).collect();
    let mut edges: BTreeMap<(usize, usize), Vec<(&str, &str)>> = BTreeMap::new();
    for (f, ts) in &mg.out {
        let Some(a) = layers.of(f) else { continue };
        for t in ts {
            let Some(b) = layers.of(t) else { continue };
            if a != b {
                edges.entry((li[a], li[b])).or_default().push((f, t));
            }
        }
    }
    let mut adj = vec![vec![]; names.len()];
    for (a, b) in edges.keys() {
        adj[*a].push(*b);
    }
    let mut out = vec![];
    for comp in tarjan(&adj).into_iter().filter(|c| c.len() >= 2) {
        let set: HashSet<usize> = comp.iter().copied().collect();
        let dirs: Vec<(usize, usize)> = edges
            .keys()
            .copied()
            .filter(|(a, b)| set.contains(a) && set.contains(b))
            .collect();
        let cnt = |d: &(usize, usize)| edges[d].len();
        // Back edges: in each two-way pair the cheaper direction (tie: later-declared source layer).
        let mut back: Vec<(usize, usize)> = vec![];
        for &(a, b) in &dirs {
            if dirs.contains(&(b, a)) {
                let (ca, cb) = (cnt(&(a, b)), cnt(&(b, a)));
                if ca < cb || (ca == cb && a > b) {
                    back.push((a, b));
                }
            }
        }
        if back.is_empty()
            && let Some(w) = dirs.iter().min_by_key(|d| (cnt(d), std::cmp::Reverse(d.0)))
        {
            back.push(*w);
        }
        let mut sorted = dirs.clone();
        sorted.sort_by_key(|d| (!back.contains(d), cnt(d), *d));
        let shown: Vec<(usize, usize)> = sorted.into_iter().take(3).collect();
        let mut modules = vec![];
        let ev: Vec<String> = shown
            .iter()
            .map(|d| {
                let ex: Vec<String> = edges[d]
                    .iter()
                    .take(3)
                    .map(|(f, t)| {
                        modules.push(f.to_string());
                        modules.push(t.to_string());
                        format!("{f} → {t}")
                    })
                    .collect();
                format!(
                    "{} → {} ({} edges{}): {}",
                    names[d.0],
                    names[d.1],
                    cnt(d),
                    if back.contains(d) { ", back edge" } else { "" },
                    ex.join(", ")
                )
            })
            .collect();
        let mut lnames: Vec<&str> = comp.iter().map(|i| names[*i]).collect();
        lnames.sort();
        let it = item(
            mg,
            "struct/cycle",
            comp.len(),
            modules,
            format!("Layer cycle between {}", lnames.join(", ")),
            ev.join("; "),
            "Layers that depend on each other are no longer layers.",
            "Remove the back edges listed first; they are the cheapest to cut.",
        );
        out.push(it);
    }
    out
}
