//! `ig plan check` on the fixtures.

use super::{PLAN, build, env, fixture_dir, load, plan_check};
use ig_app::plan::{check_text, current, merge, parse};
use ig_core::model::Graph;

fn fixture_graph(name: &str) -> Graph {
    let dir = fixture_dir(name);
    build(&dir, &load(&dir).unwrap(), None).unwrap().graph
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
    let v = check_text(&env(&fixture_dir("monorepo")), "p.md", &text).unwrap();
    let base = check_text(&env(&fixture_dir("monorepo")), "p.md", PLAN).unwrap();
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
    assert_eq!(current(&env(&dir)), None);
    let v = plan_check(&dir, &dir.join("plans/x.md")).unwrap();
    assert_eq!(v["plan"], "plans/x.md");
    assert_eq!(current(&env(&dir)), Some(dir.join("plans/x.md")));
    std::fs::remove_file(dir.join("plans/x.md")).unwrap();
    assert_eq!(current(&env(&dir)), None);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn planned_edge_breaking_a_rule_is_reported() {
    // core -> ui is forbidden in the fixture's ig.yaml
    let bad = PLAN.replace(
        "to: packages/core/src/repo.ts#Repo.find",
        "to: packages/ui/src/index.ts",
    );
    let v = check_text(&env(&fixture_dir("monorepo")), "p.md", &bad).unwrap();
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
