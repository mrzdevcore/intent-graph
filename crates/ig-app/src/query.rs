//! `ig query` and the `graph_query` tool: a node's neighbourhood in a graph.

use ig_core::model::{Graph, suggest};
use serde_json::{Value, json};
use std::collections::{BTreeSet, HashSet, VecDeque};

/// The nodes within `depth` edges of `id` following `dir` (`in`, `out` or `both`), and those
/// edges; `{error, suggestions}` when `id` is not a node.
pub fn query(g: &Graph, id: &str, dir: &str, depth: usize) -> Value {
    if !g.nodes.iter().any(|n| n.id == id) {
        return json!({"error": format!("unknown node {id}"), "suggestions": suggest(g, id)});
    }
    let mut seen: BTreeSet<&str> = BTreeSet::from([id]);
    let mut edges = HashSet::new();
    let mut q = VecDeque::from([(id, 0usize)]);
    while let Some((cur, d)) = q.pop_front() {
        if d == depth {
            continue;
        }
        for (i, e) in g.edges.iter().enumerate() {
            let next = if (dir == "out" || dir == "both") && e.from == cur {
                Some(e.to.as_str())
            } else if (dir == "in" || dir == "both") && e.to == cur {
                Some(e.from.as_str())
            } else {
                None
            };
            if let Some(n) = next {
                edges.insert(i);
                if seen.insert(n) {
                    q.push_back((n, d + 1));
                }
            }
        }
    }
    let nodes: Vec<Value> = g
        .nodes
        .iter()
        .filter(|n| seen.contains(n.id.as_str()))
        .map(|n| json!({"id": n.id, "kind": n.kind, "file": n.file, "line": n.range.start_line, "intents": n.intents}))
        .collect();
    let mut es: Vec<usize> = edges.into_iter().collect();
    es.sort();
    let edges: Vec<Value> = es
        .iter()
        .map(|i| {
            let e = &g.edges[*i];
            json!({"from": e.from, "to": e.to, "kind": e.kind, "confidence": e.confidence})
        })
        .collect();
    json!({"id": id, "nodes": nodes, "edges": edges})
}
