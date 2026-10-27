//! Golden tests: each fixture is copied outside git, its graph is built and the arch rules run on
//! it, and the graph and findings are compared with `tests/golden/`. `UPDATE_GOLDEN=1` rewrites them.

use serde_json::Value;
use std::path::{Path, PathBuf};

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
    let w = ig_wire::Wired::new(&dir);
    let env = w.env();
    let config = ig_app::config::load(env.files).unwrap();
    let built = ig_app::graph(&env, env.files, &config, None).unwrap();
    let findings = ig_core::rules::arch(&built.graph, &config, &built.workspace.packages);
    let mut graph = serde_json::to_value(&built.graph).unwrap();
    graph["toolVersion"] = Value::Null;
    golden(&format!("{name}.graph.json"), &graph);
    golden(
        &format!("{name}.findings.json"),
        &serde_json::to_value(findings).unwrap(),
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
fn shop() {
    fixture("shop");
}
