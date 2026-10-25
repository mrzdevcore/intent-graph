//! A plan's volume: what it adds, changes and removes, and what that reaches.

use super::{Plan, PlanStatus};
use crate::model::{Edge, Graph, Node};
use crate::rules::Layers;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};

/// Head nodes that transitively reach `targets` through incoming non-import edges, plus the
/// direct importers of the targets' files. The targets themselves are excluded.
fn blast_radius(head: &Graph, targets: &[&str]) -> BTreeSet<String> {
    let mut incoming: HashMap<&str, Vec<&str>> = HashMap::new();
    for e in head.edges.iter().filter(|e| e.kind != "imports") {
        incoming
            .entry(e.to.as_str())
            .or_default()
            .push(e.from.as_str());
    }
    let files: HashMap<&str, &str> = head
        .nodes
        .iter()
        .map(|n| (n.id.as_str(), n.file.as_str()))
        .collect();
    let changed_files: HashSet<&str> = targets
        .iter()
        .filter_map(|t| files.get(t).copied())
        .collect();
    let mut seen: HashSet<&str> = targets.iter().copied().collect();
    let mut q: VecDeque<&str> = targets.iter().copied().collect();
    while let Some(cur) = q.pop_front() {
        for from in incoming.get(cur).into_iter().flatten() {
            if seen.insert(from) {
                q.push_back(from);
            }
        }
    }
    for e in head.edges.iter().filter(|e| e.kind == "imports") {
        if changed_files.contains(e.to.as_str()) {
            seen.insert(e.from.as_str());
        }
    }
    seen.into_iter()
        .filter(|id| !targets.contains(id))
        .map(String::from)
        .collect()
}

/// Distinct (from, to) group pairs of `planned` edges whose ends no `head` edge already connects.
fn new_pairs(
    head: &Graph,
    merged: &Graph,
    planned: &[&Edge],
    group: impl Fn(&Node) -> Option<String>,
) -> BTreeSet<(String, String)> {
    let nodes: HashMap<&str, &Node> = merged.nodes.iter().map(|n| (n.id.as_str(), n)).collect();
    let linked: HashSet<(&str, &str)> = head
        .edges
        .iter()
        .map(|e| (e.from.as_str(), e.to.as_str()))
        .collect();
    planned
        .iter()
        .filter(|e| !linked.contains(&(e.from.as_str(), e.to.as_str())))
        .filter_map(|e| {
            let a = group(nodes.get(e.from.as_str())?)?;
            let b = group(nodes.get(e.to.as_str())?)?;
            (a != b).then_some((a, b))
        })
        .collect()
}

/// The `volume` of `ig plan check`: `merged` and `added` come from `merge(head, plan)`; `lane`
/// names a node's blueprint lane.
pub fn volume(
    head: &Graph,
    merged: &Graph,
    added: &HashSet<String>,
    plan: &Plan,
    layers: &Layers,
    lane: impl Fn(&Node) -> Option<String>,
) -> Value {
    let planned: Vec<&Edge> = merged
        .edges
        .iter()
        .filter(|e| added.contains(&e.key()))
        .collect();
    let head_ids: HashSet<&str> = head.nodes.iter().map(|n| n.id.as_str()).collect();
    let head_files: HashSet<&str> = head.nodes.iter().map(|n| n.file.as_str()).collect();
    let added_nodes: Vec<&Node> = merged
        .nodes
        .iter()
        .filter(|n| added.contains(&n.id))
        .collect();
    let new_files: BTreeSet<&str> = added_nodes
        .iter()
        .map(|n| n.file.as_str())
        .filter(|f| !f.is_empty() && !head_files.contains(f))
        .collect();
    let declared: HashSet<&str> = plan.nodes.iter().map(|n| n.id.as_str()).collect();
    let mut new_nodes: BTreeMap<&str, usize> = BTreeMap::new();
    for n in added_nodes
        .iter()
        .filter(|n| declared.contains(n.id.as_str()) && n.kind != "system")
    {
        *new_nodes.entry(n.kind.as_str()).or_default() += 1;
    }
    let existing = |s: PlanStatus| {
        plan.nodes
            .iter()
            .filter(|n| n.status == s && head_ids.contains(n.id.as_str()))
            .map(|n| n.id.as_str())
            .collect::<Vec<_>>()
    };
    let (changed, removed) = (existing(PlanStatus::Changed), existing(PlanStatus::Removed));
    let new_systems: Vec<&String> = plan
        .systems
        .iter()
        .filter(|s| !head_ids.contains(format!("system:{s}").as_str()))
        .collect();

    let cross_layer = new_pairs(head, merged, &planned, |n| {
        layers.of(&n.file).map(String::from)
    });
    let cross_lane = new_pairs(head, merged, &planned, lane);

    let targets: Vec<&str> = changed.iter().chain(&removed).copied().collect();
    let blast = blast_radius(head, &targets);
    let file_of: HashMap<&str, &str> = head
        .nodes
        .iter()
        .map(|n| (n.id.as_str(), n.file.as_str()))
        .collect();
    let mut by_layer: BTreeMap<&str, usize> = BTreeMap::new();
    for id in &blast {
        let layer = file_of.get(id.as_str()).and_then(|f| layers.of(f));
        *by_layer.entry(layer.unwrap_or("(none)")).or_default() += 1;
    }
    let sample: Vec<&String> = blast.iter().take(20).collect();

    json!({
        "newFiles": new_files.len(),
        "newNodes": new_nodes,
        "changedNodes": changed.len(),
        "removedNodes": removed.len(),
        "newSystems": new_systems,
        "newCrossLayer": cross_layer,
        "newCrossLane": cross_lane,
        "blastRadius": {"total": blast.len(), "byLayer": by_layer, "sample": sample},
    })
}
