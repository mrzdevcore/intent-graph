//! `ig plan` and `ig check` on temporary copies of a tiny repo.

use std::path::{Path, PathBuf};
use std::process::Command;

struct Out {
    code: i32,
    stdout: String,
    stderr: String,
}

fn repo(tag: &str, files: &[(&str, &str)]) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("ig-plan-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    for (p, s) in files {
        let f = dir.join(p);
        std::fs::create_dir_all(f.parent().unwrap()).unwrap();
        std::fs::write(f, s).unwrap();
    }
    dir
}

fn run_ig(dir: &Path, args: &[&str]) -> Out {
    let o = Command::new(env!("CARGO_BIN_EXE_ig"))
        .arg("--root")
        .arg(dir)
        .args(args)
        .output()
        .unwrap();
    Out {
        code: o.status.code().unwrap(),
        stdout: String::from_utf8_lossy(&o.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&o.stderr).into_owned(),
    }
}

#[test]
fn check_never_writes_ig() {
    let dir = repo("first", &[("src/a.ts", "export function f() {}\n")]);
    let out = run_ig(&dir, &["check"]);
    assert_eq!(out.code, 2);
    assert!(
        out.stderr
            .contains("no ig/ yet: run `ig plan` or `ig view` to write it"),
        "{}",
        out.stderr
    );
    assert!(!dir.join("ig").exists());
}

#[test]
fn check_passes_after_plan_writes_ig_then_fails_on_drift() {
    let dir = repo("drift", &[("src/a.ts", "export function f() {}\n")]);
    assert_eq!(run_ig(&dir, &["plan"]).code, 0);
    let out = run_ig(&dir, &["check"]);
    assert_eq!(out.code, 0, "{}{}", out.stdout, out.stderr);
    assert!(
        out.stdout.starts_with("ig check: 0 to build, 0 drift"),
        "{}",
        out.stdout
    );
    // the root package claims its own folders, not `**`: new code elsewhere is drift
    std::fs::create_dir_all(dir.join("lib")).unwrap();
    std::fs::write(dir.join("lib/c.ts"), "export const y = 2;\n").unwrap();
    let out = run_ig(&dir, &["check"]);
    assert_eq!(out.code, 1);
    assert!(
        out.stdout.contains("drift: code under lib/** is in no box"),
        "{}",
        out.stdout
    );
    let out = run_ig(&dir, &["plan"]);
    assert_eq!(out.code, 0);
    assert!(
        out.stdout.starts_with("ig plan: 0 to build, 1 drift"),
        "{}",
        out.stdout
    );
}

#[test]
fn check_and_plan_exit_2_on_a_broken_spec() {
    let dir = repo("broken", &[("src/a.ts", "export function f() {}\n")]);
    assert_eq!(run_ig(&dir, &["plan"]).code, 0);
    let f = std::fs::read_dir(dir.join("ig"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    std::fs::write(f, "{{ not kdl").unwrap();
    assert_eq!(run_ig(&dir, &["check"]).code, 2);
    let out = run_ig(&dir, &["plan"]);
    assert_eq!(out.code, 2);
    assert_eq!(out.stdout, "");
}
