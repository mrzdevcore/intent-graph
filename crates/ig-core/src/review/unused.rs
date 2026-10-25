//! `struct/unused-export`.

use super::module_graph::{ModuleGraph, USES};
use super::{Ctx, ReviewItem, item};
use crate::model::Node;
use std::collections::{HashMap, HashSet};

pub(super) fn unused_exports(mg: &ModuleGraph, ctx: &Ctx) -> Vec<ReviewItem> {
    let g = mg.graph;
    // Skip package entries and everything they re-export (transitively), and entryPoints.
    let mut skip: HashSet<&str> = HashSet::new();
    let mut q: Vec<&str> = ctx
        .packages
        .iter()
        .filter_map(|p| p.source_entry.as_deref())
        .collect();
    q.extend(
        mg.modules
            .iter()
            .copied()
            .filter(|m| ctx.entry_points.is_match(m)),
    );
    let mut reexports: HashMap<&str, Vec<&str>> = HashMap::new();
    for e in g.edges.iter().filter(|e| e.kind == "imports" && e.reexport) {
        reexports
            .entry(e.from.as_str())
            .or_default()
            .push(e.to.as_str());
    }
    while let Some(m) = q.pop() {
        if skip.insert(m) {
            q.extend(reexports.get(m).into_iter().flatten());
        }
    }
    let ns_imported: HashSet<&str> = g
        .edges
        .iter()
        .filter(|e| e.kind == "imports" && e.namespace)
        .map(|e| e.to.as_str())
        .collect();
    let is_test = |id: &str| {
        mg.nodes
            .get(id)
            .is_some_and(|n| n.kind == "test" || ctx.tests.is_match(&n.file))
    };
    let mut used: HashSet<&str> = HashSet::new();
    let mut test_used: HashSet<&str> = HashSet::new();
    for e in g.edges.iter().filter(|e| USES.contains(&e.kind.as_str())) {
        let Some(to) = mg.top_level(&e.to) else {
            continue;
        };
        let (Some(fnode), Some(tnode)) = (mg.nodes.get(e.from.as_str()), mg.nodes.get(to)) else {
            continue;
        };
        if fnode.file == tnode.file {
            continue;
        }
        if is_test(&e.from) {
            test_used.insert(to);
        } else {
            used.insert(to);
        }
    }
    let mut out = vec![];
    for m in &mg.modules {
        if skip.contains(m) || ns_imported.contains(m) {
            continue;
        }
        let unused: Vec<&Node> = mg
            .children
            .get(m)
            .into_iter()
            .flatten()
            .copied()
            .filter(|n| n.exported && !used.contains(n.id.as_str()))
            .collect();
        if unused.is_empty() {
            continue;
        }
        let names: Vec<&str> = unused.iter().map(|n| n.name.as_str()).collect();
        let tests_only: Vec<&str> = unused
            .iter()
            .filter(|n| test_used.contains(n.id.as_str()))
            .map(|n| n.name.as_str())
            .collect();
        let mut ev = format!("unused: {}", names.join(", "));
        if !tests_only.is_empty() {
            ev.push_str(&format!("; used only by tests: {}", tests_only.join(", ")));
        }
        out.push(item(
            mg,
            "struct/unused-export",
            unused.len(),
            vec![m.to_string()],
            format!(
                "{m} exports {} unused symbol{}",
                unused.len(),
                if unused.len() == 1 { "" } else { "s" }
            ),
            ev,
            "Exports nobody uses are dead weight and widen the module's surface.",
            "Remove them, or drop the export keyword if they are used locally.",
        ));
    }
    out
}
