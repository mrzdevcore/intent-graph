//! `ig/arch`: edges between layers that `rules` forbids.

use super::Layers;
use crate::config::{Config, Severity};
use crate::model::{Finding, Graph};
use crate::workspace::Package;
use std::collections::HashMap;

pub fn arch(graph: &Graph, config: &Config, packages: &[Package]) -> Vec<Finding> {
    if config.rules.is_empty() {
        return vec![];
    }
    let layers = Layers::new(config, packages);
    let files: HashMap<&str, &str> = graph
        .nodes
        .iter()
        .map(|n| (n.id.as_str(), n.file.as_str()))
        .collect();
    let layer_of: HashMap<&str, Option<&str>> =
        files.values().map(|f| (*f, layers.of(f))).collect();
    let layer = |f: &str| layer_of.get(f).copied().flatten();
    let mut out = vec![];
    for e in graph
        .edges
        .iter()
        .filter(|e| e.resolved() && matches!(e.kind.as_str(), "imports" | "calls" | "renders"))
    {
        let (Some(ff), Some(tf)) = (files.get(e.from.as_str()), files.get(e.to.as_str())) else {
            continue;
        };
        let (Some(a), Some(b)) = (layer(ff), layer(tf)) else {
            continue;
        };
        if a == b || !config.rules.iter().any(|(x, y)| x == a && y == b) {
            continue;
        }
        out.push(
            Finding::new(
                "ig/arch",
                Severity::Error,
                "ig",
                &e.file,
                e.line,
                format!(
                    "layer {a} must not depend on {b}: {} {} {}",
                    e.from, e.kind, e.to
                ),
            )
            .node(&e.from)
            .edge(&e.from, &e.to)
            .fix(format!(
                "Remove this dependency, or move the code so {a} no longer {} {b}.",
                e.kind
            )),
        );
    }
    out
}
