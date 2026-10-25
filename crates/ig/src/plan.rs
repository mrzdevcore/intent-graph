//! Plan files (`plans/*.md` with an `ig-plan` block) and `ig plan check`.

use crate::checks::{self, Layers};
use crate::config::{Config, Severity};
use crate::diff::Status;
use crate::model::{Edge, Finding, Graph, Metrics, Node, Range};
use crate::view::LaneMap;
use crate::{check, graph, mcp, review, workspace};
use anyhow::{Context, Result, anyhow};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PlanStatus {
    New,
    Changed,
    Removed,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PlanNode {
    pub id: String,
    #[serde(default)]
    pub kind: Option<String>,
    pub status: PlanStatus,
    #[serde(default)]
    pub exported: bool,
}

fn calls() -> String {
    "calls".into()
}

#[derive(Debug, Clone, Deserialize)]
pub struct PlanEdge {
    pub from: String,
    pub to: String,
    #[serde(default = "calls")]
    pub kind: String,
}

#[derive(Default, Deserialize)]
struct Block {
    #[serde(default)]
    nodes: Vec<PlanNode>,
    #[serde(default)]
    edges: Vec<PlanEdge>,
    #[serde(default)]
    systems: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct Plan {
    pub path: String,
    pub title: String,
    pub done: bool,
    pub steps: Vec<String>,
    pub nodes: Vec<PlanNode>,
    pub edges: Vec<PlanEdge>,
    pub systems: Vec<String>,
}

pub fn parse(path: &str, text: &str) -> Result<Plan> {
    let lines: Vec<&str> = text.lines().collect();
    let title = lines
        .iter()
        .find_map(|l| l.strip_prefix("# "))
        .unwrap_or_default()
        .trim()
        .to_string();
    let done = lines.iter().any(|l| l.trim() == "Status: done");
    let mut steps = vec![];
    if let Some(i) = lines.iter().position(|l| l.trim() == "## Steps") {
        for l in lines[i + 1..].iter().map(|l| l.trim()) {
            if l.starts_with('#') || l.starts_with("```") {
                break;
            }
            if let Some((n, rest)) = l.split_once(". ")
                && !n.is_empty()
                && n.bytes().all(|b| b.is_ascii_digit())
            {
                steps.push(rest.trim().to_string());
            }
        }
    }
    let open = lines
        .iter()
        .position(|l| l.trim() == "```ig-plan")
        .ok_or_else(|| anyhow!("{path}: no ```ig-plan block"))?;
    let body: Vec<&str> = lines[open + 1..]
        .iter()
        .take_while(|l| l.trim() != "```")
        .copied()
        .collect();
    let block: Block = if body.iter().all(|l| l.trim().is_empty()) {
        Block::default()
    } else {
        serde_yaml_ng::from_str(&body.join("\n"))
            .with_context(|| format!("{path}: ig-plan block"))?
    };
    Ok(Plan {
        path: path.into(),
        title,
        done,
        steps,
        nodes: block.nodes,
        edges: block.edges,
        systems: block.systems,
    })
}

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
            let sugg = mcp::suggest(head, &n.id);
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

/// Overlays `plan` on `run`: the merged graph, `added` status for what the plan adds, `changed`
/// for planned changes to unchanged nodes, and `run.planned`. `run.plan` gets the plan's title,
/// steps and merge findings; the viewer fills in the volume from `check`.
pub fn overlay(run: &mut check::Run, plan: &Plan) {
    let (merged, added, findings) = merge(&run.result.graph, plan);
    for n in merged.nodes.iter().filter(|n| added.contains(&n.id)) {
        run.diff.nodes.insert(n.id.clone(), Status::Added);
        run.diff.touched_files.insert(n.file.clone());
    }
    for e in merged.edges.iter().filter(|e| added.contains(&e.key())) {
        run.diff.edges.insert(e.key(), Status::Added);
        run.diff.new_pairs.insert((e.from.clone(), e.to.clone()));
    }
    for n in plan
        .nodes
        .iter()
        .filter(|n| n.status == PlanStatus::Changed)
    {
        if let Some(s) = run.diff.nodes.get_mut(&n.id)
            && *s == Status::Unchanged
        {
            *s = Status::Changed;
        }
    }
    let r = &mut run.result;
    let changed = |m: &BTreeMap<String, Status>| -> BTreeMap<String, Status> {
        m.iter()
            .filter(|(_, s)| **s != Status::Unchanged)
            .map(|(k, s)| (k.clone(), *s))
            .collect()
    };
    r.changes = changed(&run.diff.nodes);
    r.edge_changes = changed(&run.diff.edges);
    r.counts = check::Counts {
        added: run.diff.count(Status::Added),
        changed: run.diff.count(Status::Changed),
        removed: run.diff.count(Status::Removed),
        unchanged: run.diff.count(Status::Unchanged),
    };
    r.graph = merged;
    run.planned = added;
    run.plan = Some(json!({
        "title": plan.title, "steps": plan.steps, "done": plan.done, "volume": null, "findings": findings,
    }));
}

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

/// `check` on plan text: tests use it so they need not write files.
pub fn check_text(root: &Path, path: &str, text: &str) -> Result<Value> {
    let plan = parse(path, text)?;
    let cfg = Config::load(root)?;
    let built = graph::build(root, &cfg, None)?;
    let (head, packages) = (&built.graph, &built.workspace.packages);
    let (merged, added, merge_findings) = merge(head, &plan);

    let planned: Vec<&Edge> = merged
        .edges
        .iter()
        .filter(|e| added.contains(&e.key()))
        .collect();
    let planned_pairs: HashSet<(&str, &str)> = planned
        .iter()
        .map(|e| (e.from.as_str(), e.to.as_str()))
        .collect();
    let mut findings: Vec<Finding> = checks::arch(&merged, &cfg, packages)
        .into_iter()
        .filter(|f| {
            f.edge
                .as_ref()
                .is_some_and(|e| planned_pairs.contains(&(e.from.as_str(), e.to.as_str())))
        })
        .map(Finding::fingerprinted)
        .collect();
    let tests = workspace::globset(&cfg.tests)?;
    let eps = workspace::globset(&cfg.entry_points)?;
    let ctx = review::Ctx {
        config: &cfg,
        tests: &tests,
        packages,
        entry_points: &eps,
    };
    findings.extend(review::regressions(head, &merged, &ctx));
    findings.extend(merge_findings);

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

    let layers = Layers::new(&cfg, packages);
    let lanes = LaneMap::new(&cfg, &layers);
    let cross_layer = new_pairs(head, &merged, &planned, |n| {
        layers.of(&n.file).map(String::from)
    });
    let cross_lane = new_pairs(head, &merged, &planned, |n| {
        lanes.of(n).map(|i| lanes.name(Some(i)))
    });

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

    let ok = !findings.iter().any(|f| f.severity == Severity::Error);
    Ok(json!({
        "plan": plan.path,
        "title": plan.title,
        "steps": plan.steps,
        "volume": {
            "newFiles": new_files.len(),
            "newNodes": new_nodes,
            "changedNodes": changed.len(),
            "removedNodes": removed.len(),
            "newSystems": new_systems,
            "newCrossLayer": cross_layer,
            "newCrossLane": cross_lane,
            "blastRadius": {"total": blast.len(), "byLayer": by_layer, "sample": sample},
        },
        "findings": findings,
        "ok": ok,
    }))
}

/// The plan at `path` (relative to the working directory): its root-relative path and text.
pub fn read(root: &Path, path: &Path) -> Result<(String, String)> {
    let text = std::fs::read_to_string(path).with_context(|| format!("{}", path.display()))?;
    let rel = path
        .canonicalize()
        .ok()
        .and_then(|p| workspace::rel(root, &p))
        .unwrap_or_else(|| path.to_string_lossy().replace('\\', "/"));
    Ok((rel, text))
}

/// Checks the plan at `path` (relative to the working directory) and records it in
/// `.ig/current-plan`.
pub fn check(root: &Path, path: &Path) -> Result<Value> {
    let (rel, text) = read(root, path)?;
    let v = check_text(root, &rel, &text)?;
    let dir = root.join(".ig");
    std::fs::create_dir_all(&dir)?;
    check::write_atomic(&dir.join("current-plan"), format!("{rel}\n").as_bytes())?;
    Ok(v)
}

/// The plan recorded by the last `ig plan check`, if it still exists.
pub fn current(root: &Path) -> Option<PathBuf> {
    let rel = std::fs::read_to_string(root.join(".ig/current-plan")).ok()?;
    let p = root.join(rel.trim());
    (!rel.trim().is_empty() && p.is_file()).then_some(p)
}

/// The plain-text form of `check`'s result.
pub fn text(v: &Value) -> String {
    let vol = &v["volume"];
    let s = |x: &Value| x.as_str().unwrap_or_default().to_string();
    let pairs = |x: &Value| {
        let p: Vec<String> = x
            .as_array()
            .into_iter()
            .flatten()
            .map(|p| format!("{} -> {}", s(&p[0]), s(&p[1])))
            .collect();
        if p.is_empty() {
            "none".into()
        } else {
            p.join(", ")
        }
    };
    let counts = |x: &Value| {
        let p: Vec<String> = x
            .as_object()
            .into_iter()
            .flatten()
            .map(|(k, n)| format!("{k} {n}"))
            .collect();
        if p.is_empty() {
            "none".into()
        } else {
            p.join(", ")
        }
    };
    let systems: Vec<String> = vol["newSystems"]
        .as_array()
        .into_iter()
        .flatten()
        .map(s)
        .collect();
    let br = &vol["blastRadius"];
    let mut out = format!("{}\n", s(&v["title"]));
    out += &format!("new files: {}\n", vol["newFiles"]);
    out += &format!("new nodes: {}\n", counts(&vol["newNodes"]));
    out += &format!("changed nodes: {}\n", vol["changedNodes"]);
    out += &format!("removed nodes: {}\n", vol["removedNodes"]);
    out += &format!(
        "new systems: {}\n",
        if systems.is_empty() {
            "none".into()
        } else {
            systems.join(", ")
        }
    );
    out += &format!("new cross-layer links: {}\n", pairs(&vol["newCrossLayer"]));
    out += &format!("new cross-lane links: {}\n", pairs(&vol["newCrossLane"]));
    out += &format!(
        "blast radius: {} ({})\n",
        br["total"],
        counts(&br["byLayer"])
    );
    let findings: Vec<Finding> = serde_json::from_value(v["findings"].clone()).unwrap_or_default();
    out += &check::findings_text(&findings);
    out += "run `ig view` to see it\n";
    out
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) const PLAN: &str = "# Export\n\n## Steps\n1. core: add export\n\n```ig-plan\nnodes:\n  - id: packages/core/src/export.ts#exportRows\n    status: new\n    exported: true\n  - id: packages/core/src/repo.ts#Repo.find\n    status: changed\nedges:\n  - from: packages/core/src/export.ts#exportRows\n    to: packages/core/src/repo.ts#Repo.find\n```\n";

    fn fixture_dir(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(name)
    }

    fn fixture_graph(name: &str) -> Graph {
        let dir = fixture_dir(name);
        crate::graph::build(&dir, &crate::config::Config::load(&dir).unwrap(), None)
            .unwrap()
            .graph
    }

    #[test]
    fn parses_and_merges() {
        let p = parse("plans/x.md", PLAN).unwrap();
        assert_eq!(
            (p.title.as_str(), p.steps.len(), p.nodes.len(), p.done),
            ("Export", 1, 2, false)
        );
        let head = fixture_graph("monorepo");
        let (g, added, f) = merge(&head, &p);
        assert!(f.is_empty());
        assert!(
            g.nodes
                .iter()
                .any(|n| n.id == "packages/core/src/export.ts" && n.is_module())
        );
        assert!(added.contains("packages/core/src/export.ts#exportRows"));
        assert!(added.contains(
            "packages/core/src/export.ts#exportRows|calls|packages/core/src/repo.ts#Repo.find"
        ));
    }

    #[test]
    fn unknown_changed_id_suggests() {
        let p = parse(
            "p.md",
            &PLAN.replace(
                "Repo.find\n    status: changed",
                "Repo.fnd\n    status: changed",
            ),
        )
        .unwrap();
        let (g, _, f) = merge(&fixture_graph("monorepo"), &p);
        assert!(
            f.iter()
                .any(|x| x.rule_id == "plan/unknown-node" && x.message.contains("Repo.find"))
        );
        assert!(!g.nodes.iter().any(|n| n.id.ends_with("Repo.fnd")));
    }

    #[test]
    fn planned_system_can_be_used() {
        let text = PLAN.replace(
            "edges:\n",
            "systems: [PostgreSQL]\nedges:\n  - from: packages/core/src/repo.ts\n    to: system:PostgreSQL\n    kind: uses\n",
        );
        let p = parse("p.md", &text).unwrap();
        let (g, added, f) = merge(&fixture_graph("monorepo"), &p);
        assert!(f.is_empty(), "{f:?}");
        assert!(
            g.nodes
                .iter()
                .any(|n| n.id == "system:PostgreSQL" && n.kind == "system" && n.file.is_empty())
        );
        assert!(added.contains("packages/core/src/repo.ts|uses|system:PostgreSQL"));
        let v = check_text(&fixture_dir("monorepo"), "p.md", &text).unwrap();
        let base = check_text(&fixture_dir("monorepo"), "p.md", PLAN).unwrap();
        assert_eq!(v["volume"]["newSystems"], serde_json::json!(["PostgreSQL"]));
        assert_eq!(v["volume"]["newFiles"], base["volume"]["newFiles"]);
        assert_eq!(v["volume"]["newNodes"], base["volume"]["newNodes"]);
    }

    #[test]
    fn check_records_current_plan() {
        let dir = std::env::temp_dir().join(format!("ig-plan-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        assert!(
            std::process::Command::new("cp")
                .arg("-R")
                .arg(fixture_dir("monorepo"))
                .arg(&dir)
                .status()
                .unwrap()
                .success()
        );
        let dir = dir.canonicalize().unwrap();
        std::fs::create_dir_all(dir.join("plans")).unwrap();
        std::fs::write(dir.join("plans/x.md"), PLAN).unwrap();
        assert_eq!(current(&dir), None);
        let v = check(&dir, &dir.join("plans/x.md")).unwrap();
        assert_eq!(v["plan"], "plans/x.md");
        assert_eq!(current(&dir), Some(dir.join("plans/x.md")));
        std::fs::remove_file(dir.join("plans/x.md")).unwrap();
        assert_eq!(current(&dir), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn planned_edge_breaking_a_rule_is_reported() {
        // core -> ui is forbidden in the fixture's ig.yaml
        let bad = PLAN.replace(
            "to: packages/core/src/repo.ts#Repo.find",
            "to: packages/ui/src/index.ts",
        );
        let v = check_text(&fixture_dir("monorepo"), "p.md", &bad).unwrap();
        assert_eq!(v["ok"], false);
        assert!(
            v["findings"]
                .as_array()
                .unwrap()
                .iter()
                .any(|f| f["ruleId"] == "ig/arch")
        );
        assert_eq!(
            v["volume"]["newCrossLayer"][0],
            serde_json::json!(["core", "ui"])
        );
        assert!(v["volume"]["blastRadius"]["total"].as_u64().unwrap() >= 1); // main.tsx calls Repo.find
    }
}
