//! Enforces the hexagonal layout: the dependency rule, no I/O in the pure crates, file sizes.

use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::Command;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn rs_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for e in std::fs::read_dir(dir).unwrap() {
        let p = e.unwrap().path();
        if p.is_dir() {
            rs_files(&p, out);
        } else if p.extension().is_some_and(|x| x == "rs") {
            out.push(p);
        }
    }
}

#[test]
fn dependency_rule() {
    let cargo = option_env!("CARGO").unwrap_or("cargo");
    let out = Command::new(cargo)
        .args(["metadata", "--format-version", "1", "--no-deps"])
        .current_dir(root())
        .output()
        .unwrap();
    assert!(out.status.success(), "cargo metadata failed");
    let meta: Value = serde_json::from_slice(&out.stdout).unwrap();
    let mut seen = 0;
    for pkg in meta["packages"].as_array().unwrap() {
        let name = pkg["name"].as_str().unwrap();
        // the driving adapters may depend on every crate: the `ig` binary (package
        // `intent-graph`), the desktop app, and the wiring they share
        if !name.starts_with("ig-") || ["ig-wire", "ig-desktop"].contains(&name) {
            continue;
        }
        seen += 1;
        let deps: Vec<&str> = pkg["dependencies"]
            .as_array()
            .unwrap()
            .iter()
            .map(|d| d["name"].as_str().unwrap())
            .filter(|d| d.starts_with("ig-"))
            .collect();
        let allowed: &[&str] = match name {
            "ig-core" => &[],
            _ => &["ig-core"],
        };
        for d in &deps {
            assert!(allowed.contains(d), "{name} must not depend on {d}");
        }
    }
    assert_eq!(
        seen, 8,
        "ig-core, ig-app, ig-lang-ts, ig-lang-py, ig-lang-rs, ig-fs, ig-git, ig-conn-incus"
    );
}

#[test]
fn no_io_in_core_or_app() {
    let banned = [
        "std::fs",
        "std::process",
        "std::net",
        "Command::new",
        "File::open",
        "println!",
        "eprintln!",
    ];
    let mut files = vec![];
    for c in ["ig-core", "ig-app"] {
        rs_files(&root().join("crates").join(c).join("src"), &mut files);
    }
    assert!(!files.is_empty());
    let mut bad = vec![];
    for f in files {
        let text = std::fs::read_to_string(&f).unwrap();
        for (i, line) in text.lines().enumerate() {
            for b in banned {
                if line.contains(b) {
                    bad.push(format!("{}:{}: {b}", f.display(), i + 1));
                }
            }
        }
    }
    assert!(
        bad.is_empty(),
        "I/O in the pure crates:\n{}",
        bad.join("\n")
    );
}

#[test]
fn files_stay_small() {
    let mut files = vec![];
    rs_files(&root().join("crates"), &mut files);
    let big: Vec<String> = files
        .iter()
        .filter(|f| !f.components().any(|c| c.as_os_str() == "target"))
        .filter_map(|f| {
            let n = std::fs::read_to_string(f).unwrap().lines().count();
            (n > 600).then(|| format!("{}: {n} lines", f.display()))
        })
        .collect();
    assert!(
        big.is_empty(),
        "split these files (max 600 lines):\n{}",
        big.join("\n")
    );
}
