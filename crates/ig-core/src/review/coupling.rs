//! `struct/change-coupling`: files that change together with no dependency between them.

use super::module_graph::ModuleGraph;
use super::{ReviewItem, item};
use crate::config::ReviewConfig;
use std::collections::{HashMap, HashSet};

/// `history`: the files each recent commit changed, newest first (`review.historyCommits` of them).
pub(super) fn change_coupling(
    mg: &ModuleGraph,
    r: &ReviewConfig,
    history: &[Vec<String>],
) -> Vec<ReviewItem> {
    let mods: HashSet<&str> = mg.modules.iter().copied().collect();
    let mut changes: HashMap<&str, usize> = HashMap::new();
    let mut pairs: HashMap<(&str, &str), usize> = HashMap::new();
    for files in history {
        if files.len() > 50 {
            continue;
        }
        let mut fs: Vec<&str> = files
            .iter()
            .filter_map(|f| mods.get(f.as_str()).copied())
            .collect();
        fs.sort();
        fs.dedup();
        for f in &fs {
            *changes.entry(f).or_default() += 1;
        }
        for i in 0..fs.len() {
            for j in i + 1..fs.len() {
                *pairs.entry((fs[i], fs[j])).or_default() += 1;
            }
        }
    }
    let mut out = vec![];
    let mut keys: Vec<_> = pairs.into_iter().collect();
    keys.sort();
    for ((a, b), c) in keys {
        let rarer = changes[a].min(changes[b]);
        if c < r.coupling_min || (c as f64) < r.coupling_ratio * rarer as f64 {
            continue;
        }
        if mg.dir[a] == mg.dir[b] || mg.out[a].contains(b) || mg.out[b].contains(a) {
            continue;
        }
        out.push(item(
            mg,
            "struct/change-coupling",
            2,
            vec![a.to_string(), b.to_string()],
            format!("{a} and {b} change together"),
            format!("{c} shared commits out of {rarer}, with no dependency between them"),
            "Files that always change together but don't reference each other hide a dependency.",
            "Make the shared concept explicit in one module both depend on.",
        ));
    }
    out
}
