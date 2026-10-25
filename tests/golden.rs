//! Golden tests: each fixture is copied outside git, `ig check --all --json` and `ig review --json` run on it,
//! and the graph, findings and review are compared with `tests/golden/`. `UPDATE_GOLDEN=1` rewrites them.

use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::Command;

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for e in std::fs::read_dir(from).unwrap().flatten() {
        let p = e.path();
        if p.is_dir() {
            copy_dir(&p, &to.join(e.file_name()));
        } else {
            std::fs::copy(&p, to.join(e.file_name())).unwrap();
        }
    }
}

fn run(root: &Path, args: &[&str]) -> Value {
    let out = Command::new(env!("CARGO_BIN_EXE_ig"))
        .args(args)
        .arg("--root")
        .arg(root)
        .output()
        .unwrap();
    assert!(
        out.status.code().unwrap() < 2,
        "ig {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).unwrap()
}

fn golden(name: &str, actual: &Value) {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden")
        .join(name);
    let text = serde_json::to_string_pretty(actual).unwrap() + "\n";
    if std::env::var("UPDATE_GOLDEN").is_ok() {
        std::fs::write(&path, &text).unwrap();
        return;
    }
    let expected = std::fs::read_to_string(&path)
        .unwrap_or_else(|_| panic!("missing {name}; run with UPDATE_GOLDEN=1"));
    assert!(
        expected == text,
        "{name} differs from golden; run with UPDATE_GOLDEN=1 and review the diff"
    );
}

fn fixture(name: &str) {
    let src = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name);
    let dir = std::env::temp_dir().join(format!("ig-golden-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    copy_dir(&src, &dir);
    let mut check = run(&dir, &["check", "--all", "--json"]);
    let mut graph = check["graph"].take();
    graph["toolVersion"] = Value::Null;
    golden(&format!("{name}.graph.json"), &graph);
    golden(&format!("{name}.findings.json"), &check["findings"]);
    golden(
        &format!("{name}.review.json"),
        &run(&dir, &["review", "--json"]),
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn monorepo() {
    fixture("monorepo");
}

#[test]
fn cycle() {
    fixture("cycle");
}

#[test]
fn query_cli() {
    let dir = std::env::temp_dir().join(format!("ig-query-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    copy_dir(
        &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/monorepo"),
        &dir,
    );
    let v = run(
        &dir,
        &[
            "query",
            "packages/core/src/repo.ts#Repo",
            "--in",
            "--depth",
            "1",
            "--json",
        ],
    );
    assert!(
        v["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .any(|n| n["id"] == "apps/web/src/main.tsx")
    );
    let v = run(&dir, &["query", "Rep", "--json"]);
    assert!(!v["suggestions"].as_array().unwrap().is_empty());
    let st = Command::new(env!("CARGO_BIN_EXE_ig"))
        .args(["focus", "a", "b", "--root"])
        .arg(&dir)
        .status()
        .unwrap();
    assert!(st.success());
    assert_eq!(
        std::fs::read_to_string(dir.join(".ig/focus.json"))
            .unwrap()
            .matches('"')
            .count(),
        4
    );
}
