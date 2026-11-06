//! `ig diff` on temporary git repos.

use std::path::{Path, PathBuf};
use std::process::Command;

struct Out {
    code: i32,
    stdout: String,
    stderr: String,
}

fn git(dir: &Path, args: &[&str]) -> String {
    let o = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["-c", "user.name=t", "-c", "user.email=t@t"])
        .args(args)
        .output()
        .unwrap();
    assert!(
        o.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&o.stderr)
    );
    String::from_utf8_lossy(&o.stdout).into_owned()
}

fn write(dir: &Path, p: &str, s: &str) {
    let f = dir.join(p);
    std::fs::create_dir_all(f.parent().unwrap()).unwrap();
    std::fs::write(f, s).unwrap();
}

fn repo(tag: &str, files: &[(&str, &str)]) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("ig-diff-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    git(&dir, &["init", "-q"]);
    for (p, s) in files {
        write(&dir, p, s);
    }
    commit(&dir, "one");
    dir
}

fn commit(dir: &Path, msg: &str) {
    git(dir, &["add", "-A"]);
    git(dir, &["commit", "-qm", msg]);
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

fn json(dir: &Path, args: &[&str]) -> serde_json::Value {
    let mut a = vec!["diff"];
    a.extend(args);
    a.push("--json");
    let out = run_ig(dir, &a);
    assert_eq!(out.code, 0, "{}", out.stderr);
    serde_json::from_str(&out.stdout).unwrap()
}

const TWO: [(&str, &str); 6] = [
    ("package.json", "{\"name\":\"root\",\"private\":true}\n"),
    ("pnpm-workspace.yaml", "packages:\n  - \"packages/*\"\n"),
    (
        "packages/a/package.json",
        "{\"name\":\"@t/a\",\"dependencies\":{\"@t/b\":\"workspace:*\"}}\n",
    ),
    ("packages/a/src/a.ts", "export function f() {}\n"),
    (
        "packages/b/package.json",
        "{\"name\":\"@t/b\",\"main\":\"src/b.ts\"}\n",
    ),
    ("packages/b/src/b.ts", "export function g() {}\n"),
];

#[test]
fn no_change_on_the_same_commit() {
    let dir = repo("same", &TWO);
    let d = json(&dir, &["HEAD", "HEAD"]);
    assert_eq!(d["summary"], "no architecture change");
    assert_eq!(d["boxes"]["added"].as_array().unwrap().len(), 0);
    assert_eq!(d["arrows"]["added"].as_array().unwrap().len(), 0);
}

#[test]
fn a_new_file_in_a_box_is_a_changed_box() {
    let dir = repo("file", &TWO);
    write(&dir, "packages/a/src/more.ts", "export const x = 1;\n");
    let sha = git(&dir, &["rev-parse", "--short=7", "HEAD"]);
    let out = run_ig(&dir, &["diff", "HEAD"]);
    assert_eq!(out.code, 0, "{}", out.stderr);
    let want = format!("{}..working tree: 1 box changed", sha.trim());
    assert!(out.stdout.starts_with(&want), "{}", out.stdout);
}

#[test]
fn a_new_import_is_a_new_arrow() {
    let dir = repo("import", &TWO);
    write(
        &dir,
        "packages/a/src/a.ts",
        "import { g } from '@t/b';\nexport function f() { g(); }\n",
    );
    commit(&dir, "two");
    let d = json(&dir, &["HEAD~1", "HEAD"]);
    let added = d["arrows"]["added"].as_array().unwrap();
    assert!(!added.is_empty(), "{d}");
    let out = run_ig(&dir, &["diff", "HEAD~1", "HEAD"]);
    assert!(out.stdout.contains("  arrow +  "), "{}", out.stdout);
}

#[test]
fn ig_added_by_the_change() {
    let dir = repo("spec", &TWO);
    assert_eq!(run_ig(&dir, &["analyse", "--no-agent"]).code, 0);
    git(&dir, &["add", "-A"]);
    git(&dir, &["commit", "-qm", "map"]);
    let d = json(&dir, &["HEAD~1", "HEAD"]);
    assert_eq!(d["intent"]["status"], "added", "{d}");
    assert_eq!(d["boxes"]["added"].as_array().unwrap().len(), 0, "{d}");
    assert_eq!(d["boxes"]["removed"].as_array().unwrap().len(), 0, "{d}");
}

#[test]
fn no_arguments_compare_against_the_default_base() {
    let dir = repo("usage", &TWO);
    let out = run_ig(&dir, &["diff", "--json"]);
    assert_eq!(out.code, 0, "{}", out.stderr);
}

#[test]
fn changed_lines_match_the_count_and_skip_the_package() {
    let dir = repo(
        "direct",
        &[
            ("package.json", "{\"name\":\"root\",\"private\":true}\n"),
            ("pnpm-workspace.yaml", "packages:\n  - \"packages/*\"\n"),
            ("packages/a/package.json", "{\"name\":\"@t/a\"}\n"),
            ("packages/a/src/x/x.ts", "export const x = 1;\n"),
            ("packages/a/src/y/y.ts", "export const y = 1;\n"),
        ],
    );
    write(&dir, "packages/a/src/x/x.ts", "export const x = 2;\n");
    write(&dir, "packages/a/src/y/y.ts", "export const y = 2;\n");
    let out = run_ig(&dir, &["diff", "HEAD"]);
    assert_eq!(out.code, 0, "{}", out.stderr);
    let first = out.stdout.lines().next().unwrap();
    let n: usize = first
        .split(": ")
        .nth(1)
        .unwrap()
        .split(' ')
        .next()
        .unwrap()
        .parse()
        .unwrap();
    let lines = out
        .stdout
        .lines()
        .filter(|l| l.starts_with("  changed  "))
        .count();
    assert_eq!(lines, n, "{}", out.stdout);
    assert!(!out.stdout.contains("changed  @t/a"), "{}", out.stdout);
}

#[test]
fn not_a_commit_and_no_git() {
    let dir = repo("bad", &TWO);
    let out = run_ig(&dir, &["diff", "nope"]);
    assert_eq!(out.code, 2);
    assert_eq!(out.stderr.trim().lines().count(), 1, "{}", out.stderr);
    assert!(out.stderr.contains("not a commit"), "{}", out.stderr);
    assert!(out.stdout.is_empty());

    let plain = std::env::temp_dir().join(format!("ig-diff-plain-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&plain);
    write(&plain, "src/a.ts", "export function f() {}\n");
    let out = run_ig(&plain, &["diff", "HEAD"]);
    assert_eq!(out.code, 2);
    assert_eq!(out.stderr.trim(), "ig: ig diff needs a git repository");
}

#[test]
fn nothing_is_written() {
    let dir = repo("quiet", &TWO);
    write(&dir, "packages/a/src/more.ts", "export const x = 1;\n");
    let before = git(&dir, &["status", "--porcelain"]);
    let out = run_ig(&dir, &["diff", "HEAD~0"]);
    assert_eq!(out.code, 0, "{}", out.stderr);
    assert_eq!(git(&dir, &["status", "--porcelain"]), before);
    assert!(!dir.join("ig").exists());
    assert_eq!(git(&dir, &["worktree", "list"]).lines().count(), 1);
}

#[test]
fn a_moved_file_changes_both_boxes() {
    let mut files = TWO.to_vec();
    files.push(("packages/a/src/m.ts", "export const m = 1;\n"));
    let dir = repo("rename", &files);
    git(&dir, &["mv", "packages/a/src/m.ts", "packages/b/src/m.ts"]);
    commit(&dir, "move");
    let d = json(&dir, &["HEAD~1", "HEAD"]);
    let changed: Vec<&str> = (d["boxes"]["changed"].as_array().unwrap().iter())
        .map(|c| c["title"].as_str().unwrap())
        .collect();
    assert!(changed.contains(&"A") && changed.contains(&"B"), "{d}");
}

#[test]
fn json_revs_are_full_shas() {
    let dir = repo("shas", &TWO);
    write(&dir, "packages/a/src/more.ts", "export const x = 1;\n");
    commit(&dir, "two");
    let d = json(&dir, &["HEAD~1", "HEAD"]);
    let full = |v: &serde_json::Value| {
        let s = v.as_str().unwrap_or_default();
        s.len() == 40 && s.bytes().all(|b| b.is_ascii_hexdigit())
    };
    assert!(full(&d["base"]) && full(&d["head"]), "{d}");
    assert_eq!(
        d["head"].as_str().unwrap(),
        git(&dir, &["rev-parse", "HEAD"]).trim()
    );
}
