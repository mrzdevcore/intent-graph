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
        out.stderr.contains("no ig/ yet: run `ig analyse`"),
        "{}",
        out.stderr
    );
    assert!(!dir.join("ig").exists());
}

#[test]
fn check_passes_after_analyse_writes_ig_then_fails_on_drift() {
    let dir = repo("drift", &[("src/a.ts", "export function f() {}\n")]);
    assert_eq!(run_ig(&dir, &["analyse", "--no-agent"]).code, 0);
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
    let lines: Vec<&str> = out.stdout.lines().collect();
    assert!(
        lines.contains(&"  ? code under lib/** is in no box  lib")
            && lines.contains(&"Plan: 0 to build, 0 to move, 1 drift.")
            && lines.iter().any(|l| l.starts_with("  ig accept lib")),
        "{}",
        out.stdout
    );
    assert!(!out.stdout.contains('\x1b'), "{}", out.stdout);
}

#[test]
fn check_warns_without_failing() {
    let dir = repo("warn", &[("src/a.ts", "export function f() {}\n")]);
    assert_eq!(run_ig(&dir, &["analyse", "--no-agent"]).code, 0);
    let f = dir.join("ig/system.kdl");
    // a usecase straight in the system: reads, with a warning
    let t = std::fs::read_to_string(&f).unwrap();
    let t = t.replacen("{\n", "{\n  usecase \"lonely\" title=\"Lonely\"\n", 1);
    std::fs::write(&f, t).unwrap();
    let out = run_ig(&dir, &["check"]);
    assert_eq!(out.code, 0, "{}{}", out.stdout, out.stderr);
    assert!(
        out.stderr
            .contains("warning: `lonely`: a usecase sits inside a running block"),
        "{}",
        out.stderr
    );
    // every command words a warning the same
    for args in [&["check"][..], &["view", "--no-open", "--json"]] {
        let out = run_ig(&dir, args);
        assert!(
            (out.stderr.lines())
                .any(|l| l == "warning: `lonely`: a usecase sits inside a running block"),
            "{args:?}: {}",
            out.stderr
        );
    }
}

#[test]
fn check_and_plan_exit_2_on_a_broken_spec() {
    let dir = repo("broken", &[("src/a.ts", "export function f() {}\n")]);
    assert_eq!(run_ig(&dir, &["analyse", "--no-agent"]).code, 0);
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
    assert_eq!(run_ig(&dir, &["analyse", "--no-agent"]).code, 0);
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
        out.stdout.contains(&format!(
            "  ~ api/application-refund\n      {old} → {new}\n"
        )),
        "{}{}",
        out.stdout,
        out.stderr
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_move_is_named_after_a_staged_git_mv_of_its_spec_file() {
    let dir = committed_shop("staged");
    git(&dir, &["mv", "ig/api.kdl", "ig/backend.kdl"]);
    let (old, new) = (
        "apps/api/src/application/refund/**",
        "apps/api/src/billing/refund/**",
    );
    let p = dir.join("ig/backend.kdl");
    let text = std::fs::read_to_string(&p).unwrap();
    std::fs::write(&p, text.replace(old, new)).unwrap();
    git(&dir, &["add", "ig/backend.kdl"]);
    let out = run_ig(&dir, &["plan"]);
    assert!(
        out.stdout.contains(&format!(
            "  ~ api/application-refund\n      {old} → {new}\n"
        )),
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

#[test]
fn the_root_stays_read_beside_found_packages() {
    let sys = "system \"s\" title=\"S\" description=\"d\" {\n    at \"**\"\n    \
               service \"manage\" title=\"Manage\" description=\"d\" {\n        at \"manage.py\"\n        uses \"shop\"\n    }\n    \
               service \"shop\" title=\"Shop\" description=\"d\" {\n        at \"shop/**\"\n    }\n    \
               app \"web\" title=\"Web\" description=\"d\" {\n        at \"frontend/**\"\n    }\n}\n";
    let dir = repo(
        "django",
        &[
            ("ig/system.kdl", sys),
            ("requirements.txt", "django\n"),
            ("manage.py", "import shop.views\n"),
            ("shop/views.py", "def index():\n    return 1\n"),
            ("frontend/package.json", r#"{"name": "web"}"#),
            ("frontend/src/a.ts", "export const a = 1;\n"),
        ],
    );
    let out = run_ig(&dir, &["check"]);
    assert_eq!(out.code, 0, "{}{}", out.stdout, out.stderr);
    assert!(
        out.stdout.starts_with("ig check: 0 to build, 0 drift"),
        "{}",
        out.stdout
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_go_module_at_the_root_keeps_other_code_read() {
    let sys = "system \"s\" title=\"S\" description=\"d\" {\n    at \"**\"\n    \
               service \"api\" title=\"Api\" description=\"d\" {\n        at \"cmd/api/**\"\n    }\n    \
               box \"seed\" title=\"Seed\" description=\"d\" {\n        at \"scripts/**\"\n    }\n}\n";
    let dir = repo(
        "goloose",
        &[
            ("ig/system.kdl", sys),
            ("go.mod", "module example.com/loose\n"),
            ("cmd/api/main.go", "package main\n\nfunc main() {}\n"),
            ("scripts/seed.py", "def seed():\n    return 1\n"),
        ],
    );
    let out = run_ig(&dir, &["check"]);
    assert_eq!(out.code, 0, "{}{}", out.stdout, out.stderr);
    assert!(
        out.stdout.starts_with("ig check: 0 to build, 0 drift"),
        "{}",
        out.stdout
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_generated_go_file_still_names_its_database() {
    let sys = "system \"s\" title=\"S\" description=\"d\" {\n    at \"**\"\n    \
               service \"db\" title=\"Db\" description=\"d\" {\n        at \"internal/db/**\"\n    }\n}\n";
    let dir = repo(
        "sqlc",
        &[
            ("ig/system.kdl", sys),
            ("go.mod", "module example.com/s\n"),
            (
                "internal/db/db.go",
                "// Code generated by sqlc. DO NOT EDIT.\n\npackage db\n\nimport \"github.com/jackc/pgx/v5\"\n\nfunc Open() *pgx.Conn { return nil }\n",
            ),
        ],
    );
    let out = run_ig(&dir, &["check"]);
    let lines: Vec<&str> = out.stdout.lines().collect();
    assert!(
        lines.contains(&"drift: db reads postgres, not in ig/ (db->postgres)"),
        "{}{}",
        out.stdout,
        out.stderr
    );
    assert!(
        lines.contains(&"drift: store postgres is in the code, not in ig/"),
        "{}",
        out.stdout
    );
    let _ = std::fs::remove_dir_all(&dir);
}
