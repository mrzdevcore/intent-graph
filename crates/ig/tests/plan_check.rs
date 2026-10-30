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

fn git(dir: &Path, args: &[&str]) {
    let ok = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args([
            "-c",
            "commit.gpgsign=false",
            "-c",
            "user.email=t@t",
            "-c",
            "user.name=t",
        ])
        .args(args)
        .status()
        .unwrap()
        .success();
    assert!(ok, "git {args:?}");
}

/// The shop fixture in a fresh git repo, its `ig/` written and committed.
fn committed_shop(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("ig-plan-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/shop");
    assert!(
        Command::new("cp")
            .arg("-R")
            .arg(fixture)
            .arg(&dir)
            .status()
            .unwrap()
            .success()
    );
    git(&dir, &["init", "-q"]);
    assert_eq!(run_ig(&dir, &["plan"]).code, 0);
    git(&dir, &["add", "-A"]);
    git(&dir, &["commit", "-qm", "base"]);
    dir
}

#[test]
fn check_names_a_move_since_the_last_commit_and_code_ig_does_not_read() {
    let dir = committed_shop("move");
    let api = dir.join("ig/api.kdl");
    let text = std::fs::read_to_string(&api).unwrap();
    let (old, new) = (
        "apps/api/src/application/refund/**",
        "apps/api/src/billing/refund/**",
    );
    assert!(text.contains(old), "{text}");
    std::fs::write(&api, text.replace(old, new)).unwrap();
    std::fs::create_dir_all(dir.join("scripts")).unwrap();
    std::fs::write(dir.join("scripts/seed.ts"), "export const x = 1;\n").unwrap();
    let out = run_ig(&dir, &["check"]);
    assert_eq!(out.code, 1, "{}{}", out.stdout, out.stderr);
    let lines: Vec<&str> = out.stdout.lines().collect();
    assert_eq!(
        lines,
        [
            "ig check: 0 to build, 0 drift, 1 to move, 1 not read",
            &format!("move: {old} → {new} (api/application-refund)"),
            "not read: scripts/** (outside every package ig reads; add it to a workspace or to `ignore` in ig.yaml)",
        ],
        "{}",
        out.stderr
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_move_is_named_after_its_spec_file_is_renamed() {
    let dir = committed_shop("rename");
    let text = std::fs::read_to_string(dir.join("ig/api.kdl")).unwrap();
    let (old, new) = (
        "apps/api/src/application/refund/**",
        "apps/api/src/billing/refund/**",
    );
    std::fs::write(dir.join("ig/backend.kdl"), text.replace(old, new)).unwrap();
    std::fs::remove_file(dir.join("ig/api.kdl")).unwrap();
    let out = run_ig(&dir, &["plan"]);
    assert!(
        out.stdout
            .contains(&format!("move: {old} → {new} (api/application-refund)")),
        "{}{}",
        out.stdout,
        out.stderr
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn root_files_dot_folders_and_build_output_are_not_listed_as_not_read() {
    let dir = committed_shop("unread");
    for (p, s) in [
        ("eslint.config.mjs", "export default [];\n"),
        (".github/scripts/rel.js", "export const r = 1;\n"),
        ("vendor/x.js", "export const v = 1;\n"),
        ("scripts/seed.ts", "export const x = 1;\n"),
    ] {
        let f = dir.join(p);
        std::fs::create_dir_all(f.parent().unwrap()).unwrap();
        std::fs::write(f, s).unwrap();
    }
    let out = run_ig(&dir, &["check"]);
    assert_eq!(out.code, 1, "{}{}", out.stdout, out.stderr);
    let unread: Vec<&str> = (out.stdout.lines())
        .filter(|l| l.starts_with("not read:"))
        .collect();
    assert_eq!(
        unread,
        [
            "not read: scripts/** (outside every package ig reads; add it to a workspace or to `ignore` in ig.yaml)"
        ],
        "{}",
        out.stdout
    );
    let _ = std::fs::remove_dir_all(&dir);
}
