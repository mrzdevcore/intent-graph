//! `ig accept` and `ig fmt` on temporary copies of a tiny repo.

use std::path::{Path, PathBuf};
use std::process::Command;

struct Out {
    code: i32,
    stdout: String,
    stderr: String,
}

fn repo(tag: &str, files: &[(&str, &str)]) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("ig-edit-{tag}-{}", std::process::id()));
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

fn planned(tag: &str) -> PathBuf {
    let dir = repo(tag, &[("src/a.ts", "export function f() {}\n")]);
    assert_eq!(run_ig(&dir, &["plan"]).code, 0);
    dir
}

fn add_lib(dir: &Path) {
    std::fs::create_dir_all(dir.join("lib")).unwrap();
    std::fs::write(dir.join("lib/c.ts"), "export const y = 2;\n").unwrap();
}

#[test]
fn accept_adds_the_drift_box_and_check_passes() {
    let dir = planned("accept");
    add_lib(&dir);
    let out = run_ig(&dir, &["check"]);
    assert_eq!(out.code, 1);
    // the drift box takes the repo's slug, printed at the end of its line
    let id = out.stdout.lines().find(|l| l.contains("no box (")).unwrap();
    let id = id.rsplit('(').next().unwrap().trim_end_matches(')');
    let out = run_ig(&dir, &["accept", id]);
    assert_eq!(out.code, 0, "{}{}", out.stdout, out.stderr);
    assert!(out.stdout.contains(id), "{}", out.stdout);
    let out = run_ig(&dir, &["check"]);
    assert_eq!(out.code, 0, "{}{}", out.stdout, out.stderr);
}

#[test]
fn accept_all_and_unknown_id() {
    let dir = planned("all");
    add_lib(&dir);
    let out = run_ig(&dir, &["accept", "nope"]);
    assert_eq!(out.code, 2);
    assert!(out.stderr.contains("nope"), "{}", out.stderr);
    assert_eq!(run_ig(&dir, &["check"]).code, 1, "nothing was added");
    assert_eq!(run_ig(&dir, &["accept", "--all"]).code, 0);
    assert_eq!(run_ig(&dir, &["check"]).code, 0);
}

#[test]
fn fmt_fixes_indentation_and_keeps_comments() {
    let dir = planned("fmt");
    assert_eq!(run_ig(&dir, &["fmt", "--check"]).code, 0);
    let p = dir.join("ig/system.kdl");
    let text = std::fs::read_to_string(&p).unwrap();
    let mangled = text
        .replacen('\n', "\n// keep me\n", 1)
        .replace("\n  ", "\n          ");
    let mangled = format!("{mangled}// tail comment\n");
    std::fs::write(&p, &mangled).unwrap();
    let out = run_ig(&dir, &["fmt", "--check"]);
    assert_eq!(out.code, 1, "{}{}", out.stdout, out.stderr);
    assert_eq!(
        std::fs::read_to_string(&p).unwrap(),
        mangled,
        "check writes nothing"
    );
    assert_eq!(run_ig(&dir, &["fmt"]).code, 0);
    let fixed = std::fs::read_to_string(&p).unwrap();
    assert!(
        fixed.contains("// keep me") && fixed.contains("// tail comment"),
        "{fixed}"
    );
    assert!(!fixed.contains("          "), "{fixed}");
    assert_eq!(run_ig(&dir, &["fmt", "--check"]).code, 0);
    assert_eq!(run_ig(&dir, &["check"]).code, 0);
}

#[test]
fn fmt_without_ig_exits_2() {
    let dir = repo("nofmt", &[("src/a.ts", "export function f() {}\n")]);
    let out = run_ig(&dir, &["fmt"]);
    assert_eq!(out.code, 2);
    assert!(out.stderr.contains("no ig/ yet"), "{}", out.stderr);
}
