//! Plans: `ig plan check`, the current plan, and overlaying a plan on a check.

pub use ig_core::plan::*;

use crate::Env;
use crate::check::{self, Run};
use crate::lanes::LaneMap;
use ig_core::config::{Config, Severity};
use ig_core::diff::Status;
use ig_core::model::{Edge, Finding, Graph};
use ig_core::review;
use ig_core::rules::{self, Layers};
use ig_core::workspace::{self, Package};
use serde_json::{Value, json};
use std::collections::HashSet;
use std::path::PathBuf;

/// `ig_core::plan::parse` with an `Error::PlanParse`.
pub fn parse(path: &str, text: &str) -> crate::Result<Plan> {
    Ok(ig_core::plan::parse(path, text)?)
}

/// Overlays `plan` on `run`: the merged graph, `added` status for what the plan adds, `changed`
/// for planned changes to unchanged nodes, and `run.planned`. `run.plan` gets the plan's title,
/// steps and merge findings; the viewer fills in the volume from `check`.
pub fn overlay(run: &mut Run, plan: &Plan) {
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
    r.changes = check::changed_only(&run.diff.nodes);
    r.edge_changes = check::changed_only(&run.diff.edges);
    r.counts = check::Counts::of(&run.diff);
    r.graph = merged;
    run.planned = added;
    run.plan = Some(json!({
        "title": plan.title, "steps": plan.steps, "done": plan.done, "volume": null, "findings": findings,
    }));
}

/// What a check compares a plan with: the config, the code graph and the workspace packages.
/// Built once, reused by every check until the code changes.
pub struct Head {
    pub cfg: Config,
    pub graph: Graph,
    pub packages: Vec<Package>,
}

impl Head {
    pub fn load(env: &Env) -> crate::Result<Head> {
        let cfg = crate::config::load(env.files)?;
        let built = crate::graph(env, env.files, &cfg, None)?;
        Ok(Head {
            cfg,
            graph: built.graph,
            packages: built.workspace.packages,
        })
    }
}

/// `ig plan check` on plan text, without recording it. A `plans/<slug>/plan.md` with a sibling
/// design.json also reports what the design has and the plan lacks.
pub fn check_text(env: &Env, path: &str, text: &str) -> crate::Result<Value> {
    let plan = parse(path, text)?;
    let mut v = check_plan(&Head::load(env)?, &plan)?;
    let design = path
        .strip_suffix("/plan.md")
        .map(|d| format!("{d}/design.json"))
        .and_then(|p| env.files.read_bytes(&p))
        .and_then(|b| ig_core::design::parse(std::str::from_utf8(&b).ok()?).ok());
    if let Some(d) = design {
        let gaps = ig_core::design::gaps(&d, &plan);
        v["design"] = json!({
            "gaps": gaps.iter().filter(|f| f.rule_id == "plan/design-gap").count(),
            "omitted": plan.omitted.iter().map(|o| json!({"id": o.id, "reason": o.reason})).collect::<Vec<_>>(),
            "added": ig_core::design::extras(&d, &plan),
        });
        let f = v["findings"].as_array_mut().expect("findings");
        f.extend(
            gaps.iter()
                .map(|g| serde_json::to_value(g).expect("finding")),
        );
        v["ok"] = json!(
            !v["findings"]
                .as_array()
                .expect("findings")
                .iter()
                .any(|f| f["severity"] == "error")
        );
    }
    Ok(v)
}

/// The check proper: `plan` laid over `head`.
pub fn check_plan(h: &Head, plan: &Plan) -> crate::Result<Value> {
    let (cfg, head, packages) = (&h.cfg, &h.graph, &h.packages[..]);
    let (merged, added, merge_findings) = merge(head, plan);

    let planned: Vec<&Edge> = merged
        .edges
        .iter()
        .filter(|e| added.contains(&e.key()))
        .collect();
    let planned_pairs: HashSet<(&str, &str)> = planned
        .iter()
        .map(|e| (e.from.as_str(), e.to.as_str()))
        .collect();
    let mut findings: Vec<Finding> = rules::arch(&merged, cfg, packages)
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
        config: cfg,
        tests: &tests,
        packages,
        entry_points: &eps,
    };
    findings.extend(review::regressions(head, &merged, &ctx));
    findings.extend(merge_findings);

    let layers = Layers::new(cfg, packages);
    let lanes = LaneMap::new(cfg, &layers);
    let volume = volume(head, &merged, &added, plan, &layers, |n| {
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

/// Checks the plan at `rel` (root-relative) with `text` and records it in `.ig/current-plan`.
pub fn check(env: &Env, rel: &str, text: &str) -> crate::Result<Value> {
    let v = check_text(env, rel, text)?;
    env.files
        .write_atomic(".ig/current-plan", format!("{rel}\n").as_bytes())?;
    Ok(v)
}

/// The plan recorded by the last `ig plan check`, if it still exists.
pub fn current(env: &Env) -> Option<PathBuf> {
    let rel = String::from_utf8(env.files.read_bytes(".ig/current-plan")?).ok()?;
    let rel = rel.trim();
    (!rel.is_empty() && env.files.exists(rel)).then(|| env.root.join(rel))
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
mod tests {
    use super::*;
    use crate::testing::{Fake, env};
    use ig_core::ports::Files;
    use ig_core::testing::{FakeRunner, MemFiles};
    const PINNED: &str = r#"{"findings":[{"edge":{"from":"core/n.ts#n","to":"ui/b.ts#b"},"file":"core/n.ts","fingerprint":"11dac55aadfae88f0ad55da356b2c311832513db","fix":"Remove this dependency, or move the code so core no longer calls ui.","line":0,"message":"layer core must not depend on ui: core/n.ts#n calls ui/b.ts#b","nodeId":"core/n.ts#n","ruleId":"ig/arch","severity":"error","tool":"ig"}],"ok":false,"plan":"plans/l.md","steps":["link"],"title":"Link","volume":{"blastRadius":{"byLayer":{},"sample":[],"total":0},"changedNodes":0,"newCrossLane":[],"newCrossLayer":[["core","ui"]],"newFiles":1,"newNodes":{"function":1},"newSystems":[],"removedNodes":0}}"#;

    #[test]
    fn check_reports_volume_and_a_forbidden_planned_edge_and_records_the_plan() {
        let cfg =
            "layers:\n  core: ['core/**']\n  ui: ['ui/**']\nrules:\n  - forbid: \"core -> ui\"\n";
        let files = MemFiles::with(&[
            ("ig.yaml", cfg),
            ("core/a.ts", "fn a\n"),
            ("ui/b.ts", "fn b\n"),
        ]);
        let plan = "# Link\n\n## Steps\n1. link\n\n```ig-plan\nnodes:\n  - id: core/n.ts#n\n    status: new\n    exported: true\nedges:\n  - from: core/n.ts#n\n    to: ui/b.ts#b\n```\n";
        let (runner, fe) = (FakeRunner::default(), Fake::default());
        let e = env(&files, None, &runner, &fe);
        assert_eq!(current(&e), None);
        let v = check(&e, "plans/l.md", plan).unwrap();
        assert_eq!(
            (v["plan"].as_str(), v["ok"].as_bool()),
            (Some("plans/l.md"), Some(false))
        );
        assert_eq!(v["volume"]["newFiles"], 1);
        assert_eq!(v["volume"]["newCrossLayer"][0], json!(["core", "ui"]));
        assert!(
            v["findings"]
                .as_array()
                .unwrap()
                .iter()
                .any(|f| f["ruleId"] == "ig/arch")
        );
        assert!(text(&v).starts_with("Link\nnew files: 1\n"), "{}", text(&v));
        // Recorded, but only reported while the plan file exists.
        assert_eq!(current(&e), None);
        files.write_atomic("plans/l.md", plan.as_bytes()).unwrap();
        assert_eq!(current(&e), Some(PathBuf::from("./plans/l.md")));
    }

    const CFG: &str =
        "layers:\n  core: ['core/**']\n  ui: ['ui/**']\nrules:\n  - forbid: \"core -> ui\"\n";

    /// `check_text` as it was before `Head` and `check_plan` were split out.
    #[test]
    fn check_text_json_is_pinned_for_a_flat_plan() {
        let files = MemFiles::with(&[
            ("ig.yaml", CFG),
            ("core/a.ts", "fn a\n"),
            ("ui/b.ts", "fn b\n"),
        ]);
        let (runner, fe) = (FakeRunner::default(), Fake::default());
        let e = env(&files, None, &runner, &fe);
        let plan = "# Link\n\n## Steps\n1. link\n\n```ig-plan\nnodes:\n  - id: core/n.ts#n\n    status: new\n    exported: true\nedges:\n  - from: core/n.ts#n\n    to: ui/b.ts#b\n```\n";
        let v = check_text(&e, "plans/l.md", plan).unwrap();
        // Compared as values: key order depends on serde_json features other crates switch on.
        assert_eq!(v, serde_json::from_str::<Value>(PINNED).unwrap());
    }

    #[test]
    fn plan_check_reports_design_gaps_for_folder_plans() {
        let design = r#"{"version":1,"title":"D","scope":{"kind":"layer","name":"x"},"elements":[{"id":"mod-a","kind":"module","name":"a","file":"core/z.ts"}]}"#;
        let files = MemFiles::with(&[
            ("ig.yaml", CFG),
            ("core/a.ts", "fn a\n"),
            ("plans/f/design.json", design),
        ]);
        let (runner, fe) = (FakeRunner::default(), Fake::default());
        let e = env(&files, None, &runner, &fe);
        let plan = |omit: &str| format!("# F\n\n```ig-plan\nnodes: []\n{omit}```\n");
        let v = check_text(&e, "plans/f/plan.md", &plan("")).unwrap();
        assert_eq!(
            (v["ok"].as_bool(), v["design"]["gaps"].as_u64()),
            (Some(false), Some(1))
        );
        assert_eq!(v["findings"][0]["ruleId"], "plan/design-gap");
        let v = check_text(
            &e,
            "plans/f/plan.md",
            &plan("omitted:\n  - id: mod-a\n    reason: later\n"),
        )
        .unwrap();
        assert_eq!(
            (v["ok"].as_bool(), v["design"]["gaps"].as_u64()),
            (Some(true), Some(0))
        );
        assert_eq!(v["design"]["omitted"][0]["id"], "mod-a");
    }
}
