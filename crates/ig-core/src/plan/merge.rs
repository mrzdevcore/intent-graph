//! The plan's new nodes and edges added to a graph.

use super::{Plan, PlanStatus};
use crate::config::Severity;
use crate::model::{Edge, Finding, Graph, Metrics, Node, Range, suggest};
use std::collections::{HashMap, HashSet};

fn synthetic(id: &str, kind: Option<&str>, exported: bool) -> Node {
    if let Some(name) = id.strip_prefix("system:") {
        // Like graph.rs's system nodes: kind `system`, no file.
        let mut n = synthetic("", Some("system"), false);
        (n.id, n.name) = (id.into(), name.into());
        return n;
    }
    let (file, member) = match id.split_once('#') {
        Some((f, m)) => (f, Some(m)),
        None => (id, None),
    };
    let base = file.rsplit('/').next().unwrap_or(file);
    Node {
        id: id.into(),
        kind: kind
            .unwrap_or(if member.is_some() {
                "function"
            } else {
                "module"
            })
            .into(),
        name: member.unwrap_or(base).into(),
        file: file.into(),
        range: Range {
            start_line: 1,
            end_line: 1,
        },
        parent: member.map(|_| file.to_string()),
        package: None,
        exported,
        hash: "planned".into(),
        metrics: Metrics {
            loc: 1,
            complexity: 1,
        },
        intents: vec![],
    }
}

fn plan_finding(rule: &str, id: &str, msg: String) -> Finding {
    let file = id.split('#').next().unwrap_or(id);
    Finding::new(rule, Severity::Error, "ig", file, 0, msg)
        .node(id)
        .fingerprinted()
}

/// `head` plus the plan's new nodes and edges; the ids and edge keys it added; plan findings.
pub fn merge(head: &Graph, plan: &Plan) -> (Graph, HashSet<String>, Vec<Finding>) {
    let mut g = head.clone();
    let mut ids: HashSet<String> = head.nodes.iter().map(|n| n.id.clone()).collect();
    let mut added = HashSet::new();
    let mut findings = vec![];
    for n in &plan.nodes {
        if n.status == PlanStatus::New {
            if ids.contains(&n.id) {
                continue;
            }
            let node = synthetic(&n.id, n.kind.as_deref(), n.exported);
            if !node.file.is_empty() && node.file != node.id && ids.insert(node.file.clone()) {
                added.insert(node.file.clone());
                g.nodes.push(synthetic(&node.file, None, false));
            }
            ids.insert(n.id.clone());
            added.insert(n.id.clone());
            g.nodes.push(node);
        } else if !ids.contains(&n.id) {
            let sugg = suggest(head, &n.id);
            let mut msg = format!("{} is not in the graph", n.id);
            if !sugg.is_empty() {
                msg += &format!("; did you mean {}?", sugg[..sugg.len().min(3)].join(", "));
            }
            findings.push(plan_finding("plan/unknown-node", &n.id, msg));
        }
    }
    for name in &plan.systems {
        let id = format!("system:{name}");
        if ids.insert(id.clone()) {
            g.nodes.push(synthetic(&id, None, false));
            added.insert(id);
        }
    }
    let files: HashMap<String, String> = g
        .nodes
        .iter()
        .map(|n| (n.id.clone(), n.file.clone()))
        .collect();
    let mut keys: HashSet<String> = g.edges.iter().map(Edge::key).collect();
    for e in &plan.edges {
        let missing: Vec<&str> = [&e.from, &e.to]
            .into_iter()
            .filter(|x| !ids.contains(*x))
            .map(String::as_str)
            .collect();
        if !missing.is_empty() {
            for m in missing {
                findings.push(plan_finding(
                    "plan/unknown-edge-end",
                    &e.from,
                    format!(
                        "edge {} {} {}: {m} is neither in the graph nor planned",
                        e.from, e.kind, e.to
                    ),
                ));
            }
            continue;
        }
        let edge = Edge {
            from: e.from.clone(),
            to: e.to.clone(),
            kind: e.kind.clone(),
            confidence: "resolved".into(),
            file: files[&e.from].clone(),
            line: 0,
            namespace: false,
            reexport: false,
        };
        if keys.insert(edge.key()) {
            added.insert(edge.key());
            g.edges.push(edge);
        }
    }
    (g, added, findings)
}
