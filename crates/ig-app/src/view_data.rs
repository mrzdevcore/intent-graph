//! The viewer's data (`ig view --json`, and the page `ig view` builds from it): nodes with layer,
//! edges with arch violations, and the config's notices.

use crate::Env;
use crate::build::system_category;
use ig_core::model::Node;
use ig_core::rules::{self, Layers};
use ig_core::workspace;
use serde_json::{Value, json};
use std::collections::HashSet;

/// The head graph of the working tree as the viewer's JSON.
pub fn json(env: &Env) -> crate::Result<Value> {
    let config = crate::config::load(env.files)?;
    let built = crate::graph(env, env.files, &config, env.vcs.and_then(|v| v.head()))?;
    let (g, packages) = (&built.graph, &built.workspace.packages);
    let layers = Layers::new(&config, packages);
    let test_files =
        workspace::globset(&config.tests).unwrap_or_else(|_| globset::GlobSet::empty());
    let nodes: Vec<Value> = g
        .nodes
        .iter()
        .map(|n: &Node| {
            let mut v = json!({
                "id": n.id, "kind": n.kind, "name": n.name, "file": n.file, "line": n.range.start_line,
                "endLine": n.range.end_line, "parent": n.parent, "complexity": n.metrics.complexity,
                "loc": n.metrics.loc, "module": n.is_module(), "layer": layers.of(&n.file),
            });
            if n.kind == "test" || test_files.is_match(&n.file) {
                v["test"] = json!(true);
            }
            if n.kind == "system" {
                let none = vec![];
                let pkgs = config.blueprint.systems.iter().find(|(s, _)| *s == n.name).map_or(&none, |(_, p)| p);
                v["category"] = json!(system_category(&n.name, pkgs));
                v["packages"] = json!(pkgs);
            }
            v
        })
        .collect();
    let violations: HashSet<(String, String)> = rules::arch(g, &config, packages)
        .into_iter()
        .filter_map(|f| f.edge.map(|e| (e.from, e.to)))
        .collect();
    let edges: Vec<Value> = g
        .edges
        .iter()
        .map(|e| {
            json!({"from": e.from, "to": e.to, "kind": e.kind, "confidence": e.confidence, "line": e.line,
                   "violation": violations.contains(&(e.from.clone(), e.to.clone()))})
        })
        .collect();
    let layer_names: Vec<&str> = config.layers.iter().map(|(n, _)| n.as_str()).collect();
    Ok(json!({
        "nodes": nodes, "edges": edges, "notices": config.notices,
        "root": env.root.to_string_lossy(), "layers": layer_names,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{Fake, env};
    use ig_core::testing::MemFiles;

    #[test]
    fn test_files_are_flagged() {
        let files = MemFiles::with(&[
            ("src/a.ts", "fn f\n"),
            ("src/a.spec.ts", "fn g\n"),
            ("src/__tests__/b.ts", "fn h\n"),
        ]);
        let fe = Fake;
        let e = env(&files, None, &fe);
        let d = json(&e).unwrap();
        let flag = |file: &str| {
            let ns: Vec<_> = d["nodes"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|n| n["file"] == file)
                .collect();
            assert!(!ns.is_empty(), "{file}");
            ns.iter().map(|n| n["test"] == true).collect::<Vec<_>>()
        };
        assert!(flag("src/a.spec.ts").iter().all(|t| *t));
        assert!(flag("src/__tests__/b.ts").iter().all(|t| *t));
        assert!(flag("src/a.ts").iter().all(|t| !*t));
    }

    #[test]
    fn removed_config_keys_surface_as_notices() {
        let files = MemFiles::with(&[("ig.yaml", "complexityMax: 9\n"), ("src/a.ts", "fn f\n")]);
        let fe = Fake;
        let d = json(&env(&files, None, &fe)).unwrap();
        assert_eq!(d["notices"], json!(["complexityMax"]));
    }
}
