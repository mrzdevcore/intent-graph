//! The viewer data and page over the fixtures.

use super::{PLAN, check_run, env, plan_check, scratch};
use crate::Wired;
use ig_app::check::Opts;
use ig_app::view_data::{self, FOCUS, Picked};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

fn pick_plan(dir: &Path, explicit: Option<&Path>, no_plan: bool) -> Picked {
    view_data::pick_plan(&env(dir), explicit, no_plan)
}

fn json(dir: &Path, opts: &Opts, p: &Picked) -> anyhow::Result<Value> {
    Ok(view_data::json(&env(dir), opts, crate::shown_plan(dir, p))?)
}

fn write(dir: &Path, opts: &Opts, p: &Picked) -> anyhow::Result<PathBuf> {
    crate::render::write(&Wired::new(dir), opts, p)
}

fn state(dir: &Path) -> (Picked, Value) {
    let p = pick_plan(dir, None, false);
    let d = json(dir, &Opts::default(), &p).unwrap();
    (p, d["state"].clone())
}

#[test]
fn state_and_focus() {
    let dir = scratch("state");
    let o = Opts::default();
    assert_eq!(state(&dir).1, "plain");
    assert!(
        json(&dir, &o, &pick_plan(&dir, None, false))
            .unwrap()
            .get("focus")
            .is_none()
    );
    std::fs::create_dir_all(dir.join("plans")).unwrap();
    std::fs::write(dir.join("plans/export.md"), PLAN).unwrap();
    plan_check(&dir, &dir.join("plans/export.md")).unwrap();
    assert_eq!(state(&dir).1, "planned");
    assert_eq!(pick_plan(&dir, None, true), (None, false));
    std::fs::create_dir_all(dir.join(".ig")).unwrap();
    std::fs::write(dir.join(FOCUS), r#"["a.ts","b.ts"]"#).unwrap();
    let p = pick_plan(&dir, None, false);
    assert_eq!(
        json(&dir, &o, &p).unwrap()["focus"],
        json!(["a.ts", "b.ts"])
    );
    write(&dir, &o, &p).unwrap();
    assert!(!dir.join(FOCUS).exists());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn apply_state_picks_its_plan_until_stale_and_done_follows_the_plan() {
    let dir = scratch("apply");
    let git = |a: &[&str]| {
        assert!(
            std::process::Command::new("git")
                .args(a)
                .current_dir(&dir)
                .status()
                .unwrap()
                .success()
        )
    };
    git(&["init", "-q"]);
    git(&["add", "-A"]);
    git(&[
        "-c",
        "user.name=t",
        "-c",
        "user.email=t@t",
        "commit",
        "-qm",
        "i",
    ]);
    let head = ig_git::git(&dir, &["rev-parse", "HEAD"])
        .unwrap()
        .trim()
        .to_string();
    std::fs::create_dir_all(dir.join("plans")).unwrap();
    std::fs::write(dir.join("plans/export.md"), PLAN).unwrap();
    std::fs::write(dir.join("plans/other.md"), PLAN).unwrap();
    plan_check(&dir, &dir.join("plans/export.md")).unwrap();
    std::fs::write(
        dir.join(".ig/apply.json"),
        json!({"plan": "plans/other.md", "base": head, "hashes": {}}).to_string(),
    )
    .unwrap();
    let (p, s) = state(&dir);
    assert_eq!(
        (p, s),
        ((Some(dir.join("plans/other.md")), true), json!("applying"))
    );
    assert_eq!(pick_plan(&dir, None, true), (None, false));
    let nop = pick_plan(&dir, None, true);
    assert_eq!(
        json(&dir, &Opts::default(), &nop).unwrap()["state"],
        "plain"
    );
    let other = pick_plan(&dir, Some(&dir.join("plans/export.md")), false);
    assert_eq!(
        json(&dir, &Opts::default(), &other).unwrap()["state"],
        "planned"
    );
    std::fs::remove_file(dir.join("plans/other.md")).unwrap();
    let (p, s) = state(&dir);
    assert_eq!(
        (p, s),
        ((Some(dir.join("plans/export.md")), false), json!("planned"))
    );
    std::fs::remove_file(dir.join(".ig/apply.json")).unwrap();
    std::fs::write(
        dir.join("plans/export.md"),
        PLAN.replace(
            "# Export
",
            "# Export
Status: done
",
        ),
    )
    .unwrap();
    assert_eq!(state(&dir).1, "done");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn system_nodes_carry_category_and_packages() {
    let dir = scratch("systems");
    let mut cfg = std::fs::read_to_string(dir.join("ig.yaml")).unwrap();
    cfg.push_str("blueprint:\n  systems:\n    PostgreSQL: [pg]\n");
    std::fs::write(dir.join("ig.yaml"), cfg).unwrap();
    std::fs::write(
        dir.join("packages/core/src/db.ts"),
        "import { Pool } from \"pg\";\nexport const pool = new Pool();\n",
    )
    .unwrap();
    let d = json(&dir, &Opts::default(), &(None, false)).unwrap();
    let n = d["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["id"] == "system:PostgreSQL")
        .unwrap();
    assert_eq!(
        (n["category"].clone(), n["packages"].clone()),
        (json!("database"), json!(["pg"]))
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn planned_nodes_and_plan_panel() {
    let dir = std::env::temp_dir().join(format!("ig-view-plan-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let src = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/monorepo");
    assert!(
        std::process::Command::new("cp")
            .arg("-R")
            .arg(&src)
            .arg(&dir)
            .status()
            .unwrap()
            .success()
    );
    let dir = dir.canonicalize().unwrap();
    let mut run = check_run(&dir, &Opts::default()).unwrap();
    let plan = ig_app::plan::parse("plans/export.md", PLAN).unwrap();
    ig_app::plan::overlay(&mut run, &plan);
    let d = view_data::data(&env(&dir), &run, &Opts::default());
    let n = d["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["id"] == "packages/core/src/export.ts#exportRows")
        .unwrap();
    assert_eq!(
        (n["planned"].clone(), n["status"].clone()),
        (json!(true), json!("added"))
    );
    assert_eq!(d["plan"]["title"], "Export");
    assert!(crate::render::render(&env(&dir), &run, &Opts::default()).contains("planned"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn view_reuses_saved_checker_findings() {
    let dir = std::env::temp_dir().join(format!("ig-view-checkers-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let src = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/monorepo");
    assert!(
        std::process::Command::new("cp")
            .arg("-R")
            .arg(&src)
            .arg(&dir)
            .status()
            .unwrap()
            .success()
    );
    let dir = dir.canonicalize().unwrap();
    let cfg = std::fs::read_to_string(dir.join("ig.yaml")).unwrap()
        + "checkers:\n  - name: t\n    format: lines\n    run: \"touch marker; echo 'packages/core/src/math.ts:1: boom'\"\n";
    std::fs::write(dir.join("ig.yaml"), cfg).unwrap();
    let view = Opts {
        skip_checkers: true,
        ..Default::default()
    };
    let boom = |v: &Value| {
        v["findings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|f| f["tool"] == "t" && f["message"] == "boom")
    };
    write(&dir, &view, &(None, false)).unwrap();
    assert!(!dir.join("marker").exists(), "view ran a checker");
    check_run(&dir, &Opts::default()).unwrap();
    assert!(dir.join("marker").exists(), "check did not run the checker");
    std::fs::remove_file(dir.join("marker")).unwrap();
    let d = json(&dir, &view, &(None, false)).unwrap();
    assert!(!dir.join("marker").exists(), "view ran a checker");
    assert!(boom(&d), "{}", d["findings"]);
    let saved: Value =
        serde_json::from_str(&std::fs::read_to_string(dir.join(".ig/findings.json")).unwrap())
            .unwrap();
    assert!(boom(&saved), "view build dropped the saved checker finding");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn checker_findings_on_changed_non_graph_files_survive_view_builds() {
    let dir = scratch("ckscope");
    let cfg = std::fs::read_to_string(dir.join("ig.yaml")).unwrap()
        + "checkers:\n  - name: t\n    format: lines\n    run: \"echo 'a.css:1: boom'\"\n";
    std::fs::write(dir.join("ig.yaml"), cfg).unwrap();
    std::fs::write(dir.join(".gitignore"), ".ig/\n").unwrap();
    let git = |a: &[&str]| {
        let mut c = vec!["-c", "user.name=t", "-c", "user.email=t@t"];
        c.extend(a);
        assert!(
            std::process::Command::new("git")
                .args(&c)
                .current_dir(&dir)
                .status()
                .unwrap()
                .success()
        )
    };
    git(&["init", "-q"]);
    git(&["add", "-A"]);
    git(&["commit", "-qm", "i"]);
    std::fs::write(dir.join("a.css"), "a { color: red }\n").unwrap();
    let boom = |v: &Value| {
        v["findings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|f| f["tool"] == "t" && f["file"] == "a.css")
    };
    let saved = || -> Value {
        serde_json::from_str(&std::fs::read_to_string(dir.join(".ig/findings.json")).unwrap())
            .unwrap()
    };
    // An untracked .css file is in the change: its checker finding counts.
    check_run(&dir, &Opts::default()).unwrap();
    assert!(
        boom(&saved()),
        "finding on a changed non-graph file dropped"
    );

    // Committed: out of scope now. A malformed saved entry must not hide the others.
    git(&["add", "a.css"]);
    git(&["commit", "-qm", "css"]);
    let mut v = saved();
    v["findings"]
        .as_array_mut()
        .unwrap()
        .insert(0, json!({"bogus": 1}));
    std::fs::write(dir.join(".ig/findings.json"), v.to_string()).unwrap();
    let view = Opts {
        skip_checkers: true,
        ..Default::default()
    };
    let d = json(&dir, &view, &(None, false)).unwrap();
    assert!(!boom(&d), "out-of-scope finding shown");
    assert!(boom(&saved()), "view build dropped a saved checker finding");
    let _ = std::fs::remove_dir_all(&dir);
}
