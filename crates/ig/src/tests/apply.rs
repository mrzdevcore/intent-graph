//! `ig apply` on git repos.

use super::{PLAN, check_run, env, git_fixture, write};
use ig_app::{apply, baseline};
use serde_json::json;
use std::path::Path;

fn start(dir: &Path, plan: &Path) -> anyhow::Result<()> {
    let (rel, text) = crate::read_plan(dir, plan)?;
    Ok(apply::start(&env(dir), &rel, &text)?)
}

fn finish(dir: &Path) -> anyhow::Result<u8> {
    Ok(apply::finish(&env(dir), &mut String::new())?)
}

#[test]
fn unplanned_missing_and_tamper() {
    let dir = git_fixture("monorepo");
    write(&dir, "plans/x.md", PLAN);
    start(&dir, &dir.join("plans/x.md")).unwrap();
    // an unplanned exported function in a new file
    write(
        &dir,
        "packages/core/src/extra.ts",
        "export const extra = () => 1;\n",
    );
    let run = check_run(&dir, &Default::default()).unwrap();
    let ids: Vec<_> = run
        .result
        .findings
        .iter()
        .map(|f| (f.rule_id.as_str(), f.message.as_str()))
        .collect();
    assert!(
        ids.iter()
            .any(|(r, m)| *r == "ig/plan" && m.contains("packages/core/src/extra.ts"))
    );
    assert!(
        !ids.iter()
            .any(|(_, m)| m.starts_with("planned but not built"))
    );
    assert_eq!(finish(&dir).unwrap(), 1);
    // the user edits the plan mid-apply (Review Focus 4)
    write(&dir, "plans/x.md", &PLAN.replace("# Export", "# Export v2"));
    let run = check_run(&dir, &Default::default()).unwrap();
    assert!(
        run.result
            .findings
            .iter()
            .any(|f| f.rule_id == "ig/tamper" && f.message.contains("plans/x.md"))
    );
    apply::accept(&env(&dir)).unwrap();
    let run = check_run(&dir, &Default::default()).unwrap();
    assert!(!run.result.findings.iter().any(|f| f.rule_id == "ig/tamper"));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn finish_clears_state_and_marks_done() {
    let dir = git_fixture("monorepo");
    let plan = "# Tiny\n\n```ig-plan\nnodes:\n  - id: packages/core/src/extra.ts#extra\n    status: new\n    exported: true\n```\n";
    write(&dir, "plans/t.md", plan);
    start(&dir, &dir.join("plans/t.md")).unwrap();
    write(
        &dir,
        "packages/core/src/extra.ts",
        "export const extra = () => 1;\n",
    );
    let run = check_run(&dir, &Default::default()).unwrap();
    assert!(!run.result.blocking);
    let review = json!({"change": apply::change_hash(&run)}).to_string();
    write(&dir, ".ig/agent-review.json", &review);
    assert_eq!(finish(&dir).unwrap(), 0);
    assert!(apply::load(&env(&dir)).is_none());
    let text = std::fs::read_to_string(dir.join("plans/t.md")).unwrap();
    assert!(text.starts_with("# Tiny\nStatus: done\n"));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn tests_helpers_and_routes() {
    let dir = git_fixture("monorepo");
    let plan = "# R\n\n```ig-plan\nnodes:\n  - id: apps/web/src/routes.ts\n    status: changed\n  - id: packages/core/src/repo.ts\n    status: changed\n```\n";
    write(&dir, "plans/r.md", plan);
    start(&dir, &dir.join("plans/r.md")).unwrap();
    write(
        &dir,
        "packages/core/src/repo2.test.ts",
        "import { Repo } from './repo';\ntest('find', () => new Repo().find('a'));\n",
    );
    let repo = std::fs::read_to_string(dir.join("packages/core/src/repo.ts")).unwrap();
    write(
        &dir,
        "packages/core/src/repo.ts",
        &(repo + "function helper() { return 3; }\n"),
    );
    let routes = std::fs::read_to_string(dir.join("apps/web/src/routes.ts")).unwrap();
    write(
        &dir,
        "apps/web/src/routes.ts",
        &(routes + "app.get('/secret', handler);\n"),
    );
    let run = check_run(&dir, &Default::default()).unwrap();
    let plan_msgs: Vec<&str> = run
        .result
        .findings
        .iter()
        .filter(|f| f.rule_id == "ig/plan")
        .map(|f| f.message.as_str())
        .collect();
    assert!(
        !plan_msgs.iter().any(|m| m.contains("repo2.test.ts")),
        "{plan_msgs:?}"
    );
    assert!(
        !plan_msgs.iter().any(|m| m.contains("helper")),
        "{plan_msgs:?}"
    );
    assert!(
        plan_msgs
            .iter()
            .any(|m| m.contains("apps/web/src/routes.ts#") && m.contains("/secret")),
        "{plan_msgs:?}"
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn except_keeps_other_tamper_visible() {
    let dir = git_fixture("monorepo");
    write(&dir, "plans/x.md", PLAN);
    start(&dir, &dir.join("plans/x.md")).unwrap();
    write(&dir, "plans/x.md", &PLAN.replace("# Export", "# Export v2"));
    apply::except(&env(&dir), "b", "ok").unwrap();
    let run = check_run(&dir, &Default::default()).unwrap();
    let tamper: Vec<&str> = run
        .result
        .findings
        .iter()
        .filter(|f| f.rule_id == "ig/tamper")
        .map(|f| f.file.as_str())
        .collect();
    assert_eq!(tamper, vec!["plans/x.md"]);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn except_refuses_apply_findings_and_finish_lists_exceptions() {
    let dir = git_fixture("monorepo");
    write(&dir, "plans/x.md", PLAN);
    start(&dir, &dir.join("plans/x.md")).unwrap();
    write(
        &dir,
        "packages/core/src/extra.ts",
        "export const extra = () => 1;\n",
    );
    let run = check_run(&dir, &Default::default()).unwrap();
    let fp = run
        .result
        .findings
        .iter()
        .find(|f| f.rule_id == "ig/plan")
        .unwrap()
        .fingerprint
        .clone();
    let e = apply::except(&env(&dir), &fp, "nope")
        .unwrap_err()
        .to_string();
    assert!(e.contains("ig apply accept"), "{e}");
    assert!(!baseline::load(&env(&dir)).contains(&fp));
    apply::except(&env(&dir), "tool-fp", "user said fine").unwrap();
    let st = apply::load(&env(&dir)).unwrap();
    assert_eq!(
        apply::exceptions_since(&env(&dir), &st),
        vec![("tool-fp".to_string(), "user said fine".to_string())]
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn abort_clears_state_and_finish_on_stale_points_to_it() {
    let dir = git_fixture("monorepo");
    write(&dir, "plans/x.md", PLAN);
    start(&dir, &dir.join("plans/x.md")).unwrap();
    std::fs::remove_file(dir.join("plans/x.md")).unwrap();
    let e = finish(&dir).unwrap_err().to_string();
    assert!(
        e.contains("stale apply state (plan plans/x.md is missing): run ig apply abort"),
        "{e}"
    );
    assert_eq!(apply::abort(&env(&dir)).unwrap(), "plans/x.md");
    assert!(apply::load(&env(&dir)).is_none());
    assert!(apply::abort(&env(&dir)).is_err());
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn except_refuses_corrupt_baseline() {
    let dir = git_fixture("monorepo");
    write(&dir, ".ig/baseline.json", "{not json");
    assert!(apply::except(&env(&dir), "b", "r").is_err());
    assert_eq!(
        std::fs::read_to_string(dir.join(".ig/baseline.json")).unwrap(),
        "{not json"
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn stale_state_is_ignored() {
    let dir = git_fixture("monorepo");
    std::fs::create_dir_all(dir.join(".ig")).unwrap();
    std::fs::write(
        dir.join(".ig/apply.json"),
        r#"{"plan":"plans/gone.md","base":"0000000000000000000000000000000000000000","hashes":{}}"#,
    )
    .unwrap();
    let run = check_run(&dir, &Default::default()).unwrap();
    assert!(
        run.result
            .notices
            .iter()
            .any(|n| n.contains("apply state is stale"))
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn exceptions_keep_reason() {
    let dir = git_fixture("monorepo");
    baseline::write(&env(&dir), ["a".to_string()]).unwrap();
    apply::except(&env(&dir), "b", "user accepted: legacy import").unwrap();
    let b = baseline::load(&env(&dir));
    assert!(b.contains("a") && b.contains("b"));
    assert!(
        std::fs::read_to_string(dir.join(".ig/baseline.json"))
            .unwrap()
            .contains("legacy import")
    );
    apply::except(&env(&dir), "b", "again").unwrap();
    let items = baseline::read(&env(&dir)).unwrap();
    assert_eq!(items.len(), 2, "{items:?}");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn reviewer_findings_block_until_the_change_moves() {
    let dir = git_fixture("monorepo");
    write(&dir, "plans/x.md", PLAN);
    start(&dir, &dir.join("plans/x.md")).unwrap();
    write(
        &dir,
        "packages/core/src/export.ts",
        "export const exportRows = () => 1;\n",
    );
    let n = apply::review_ingest(&env(&dir), "Here you go:\n[{\"rule\":\"R-PURE\",\"file\":\"packages/core/src/export.ts\",\"line\":1,\"message\":\"side effect\",\"severity\":\"error\"}]").unwrap();
    assert_eq!(n, 1);
    let run = check_run(&dir, &Default::default()).unwrap();
    assert!(
        run.result.blocking
            && run
                .result
                .findings
                .iter()
                .any(|f| f.rule_id == "review/R-PURE")
    );
    write(
        &dir,
        "packages/core/src/export.ts",
        "export const exportRows = () => 2;\n",
    );
    let run = check_run(&dir, &Default::default()).unwrap();
    assert!(
        !run.result
            .findings
            .iter()
            .any(|f| f.rule_id == "review/R-PURE")
    );
}

#[test]
fn excepted_review_finding_disappears_and_bad_input_errors() {
    let dir = git_fixture("monorepo");
    write(&dir, "plans/x.md", PLAN);
    start(&dir, &dir.join("plans/x.md")).unwrap();
    write(
        &dir,
        "packages/core/src/export.ts",
        "export const exportRows = () => 1;\n",
    );
    apply::review_ingest(&env(&dir), "[{\"rule\":\"R-PURE\",\"file\":\"packages/core/src/export.ts\",\"line\":1,\"message\":\"m\",\"severity\":\"error\"}]").unwrap();
    let run = check_run(&dir, &Default::default()).unwrap();
    let f = run
        .result
        .findings
        .iter()
        .find(|f| f.rule_id == "review/R-PURE")
        .unwrap();
    apply::except(&env(&dir), &f.fingerprint.clone(), "false alarm").unwrap();
    let run = check_run(&dir, &Default::default()).unwrap();
    assert!(
        !run.result
            .findings
            .iter()
            .any(|f| f.rule_id == "review/R-PURE")
    );
    assert_eq!(
        apply::review_ingest(&env(&dir), "all clean: []").unwrap(),
        0
    );
    assert!(apply::review_ingest(&env(&dir), "no array here").is_err());
    assert!(apply::review_ingest(&env(&dir), "] oops [").is_err());
    assert!(apply::review_ingest(&env(&dir), "[1]").is_err());
    assert!(apply::review_ingest(&env(&dir), "[{\"rule\":\"R\"}]").is_err());
    assert!(apply::review_ingest(&env(&dir), "[{\"rule\":3,\"message\":\"m\"}]").is_err());
}
