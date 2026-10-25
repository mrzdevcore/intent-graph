//! `ig_core::plan` plus `ig plan check`, reading plans and overlaying one on a run.

pub use ig_core::plan::*;

use crate::checks::{self, Layers};
use crate::config::Severity;
use crate::diff::Status;
use crate::model::{Edge, Finding};
use crate::view::LaneMap;
use crate::{check, graph, review, workspace};
use anyhow::{Context, Result};
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};

/// `ig_core::plan::parse` with an `anyhow` error, so `{e:#}` keeps showing the YAML cause.
pub fn parse(path: &str, text: &str) -> Result<Plan> {
    Ok(ig_core::plan::parse(path, text)?)
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

/// `check` on plan text: tests use it so they need not write files.
pub fn check_text(root: &Path, path: &str, text: &str) -> Result<Value> {
    let plan = parse(path, text)?;
    let cfg = crate::config::load(root)?;
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

    let layers = Layers::new(&cfg, packages);
    let lanes = LaneMap::new(&cfg, &layers);
    let volume = volume(head, &merged, &added, &plan, &layers, |n| {
        lanes.of(n).map(|i| lanes.name(Some(i)))
    });

    let ok = !findings.iter().any(|f| f.severity == Severity::Error);
    Ok(json!({
        "plan": plan.path,
        "title": plan.title,
        "steps": plan.steps,
        "volume": volume,
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
    use crate::model::Graph;

    pub(crate) const PLAN: &str = "# Export\n\n## Steps\n1. core: add export\n\n```ig-plan\nnodes:\n  - id: packages/core/src/export.ts#exportRows\n    status: new\n    exported: true\n  - id: packages/core/src/repo.ts#Repo.find\n    status: changed\nedges:\n  - from: packages/core/src/export.ts#exportRows\n    to: packages/core/src/repo.ts#Repo.find\n```\n";

    fn fixture_dir(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(name)
    }

    fn fixture_graph(name: &str) -> Graph {
        let dir = fixture_dir(name);
        crate::graph::build(&dir, &crate::config::load(&dir).unwrap(), None)
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
