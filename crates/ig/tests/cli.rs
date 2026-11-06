use std::path::{Path, PathBuf};
use std::process::Command;

struct Out {
    code: i32,
    stdout: String,
    stderr: String,
}

fn repo(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("ig-cli-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(dir.join("src/a.ts"), "export function f() {}\n").unwrap();
    dir
}

fn ig_in(dir: &Path, args: &[&str]) -> Out {
    let o = Command::new(env!("CARGO_BIN_EXE_ig"))
        .current_dir(dir)
        .env("NO_COLOR", "1")
        .args(args)
        .output()
        .unwrap();
    Out {
        code: o.status.code().unwrap(),
        stdout: String::from_utf8_lossy(&o.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&o.stderr).into_owned(),
    }
}

fn ig(args: &[&str]) -> Out {
    ig_in(&std::env::temp_dir(), args)
}

#[test]
fn each_command_has_its_own_help() {
    for args in [
        vec!["view", "--help"],
        vec!["help", "view"],
        vec!["view", "-h"],
    ] {
        let out = ig(&args);
        assert_eq!(out.code, 0, "{args:?}: {}", out.stderr);
        assert!(out.stdout.contains("--no-read"), "{args:?}: {}", out.stdout);
        assert!(!out.stdout.contains("accept"), "{args:?}: {}", out.stdout);
    }
}

#[test]
fn top_help_lists_commands_and_hides_nothing_needed() {
    let out = ig(&["--help"]);
    assert_eq!(out.code, 0);
    for cmd in [
        "init", "view", "plan", "check", "accept", "fmt", "app", "mcp",
    ] {
        assert!(out.stdout.contains(cmd), "{cmd}: {}", out.stdout);
    }
    assert!(out.stdout.contains("-C"), "{}", out.stdout);
    assert!(out.stdout.contains("Plumbing"), "{}", out.stdout);
    assert!(!out.stdout.contains("hook"), "{}", out.stdout);
}

#[test]
fn a_flag_of_another_command_is_an_error() {
    let out = ig(&["plan", "--all"]);
    assert_eq!(out.code, 2);
    assert!(out.stderr.contains("--all"), "{}", out.stderr);
    assert!(out.stdout.is_empty());
}

#[test]
fn a_mistyped_command_gets_a_suggestion() {
    let out = ig(&["vew"]);
    assert_eq!(out.code, 2);
    assert!(out.stderr.contains("view"), "{}", out.stderr);
}

#[test]
fn an_unknown_agent_is_refused_before_anything_runs() {
    let out = ig(&["view", "--agent", "gpt"]);
    assert_eq!(out.code, 2);
    assert!(out.stderr.contains("claude"), "{}", out.stderr);
}

#[test]
fn read_and_no_read_conflict() {
    let out = ig(&["view", "--read", "--no-read"]);
    assert_eq!(out.code, 2);
}

#[test]
fn no_command_prints_usage_and_fails() {
    let out = ig(&[]);
    assert_eq!(out.code, 2);
    assert!(
        out.stderr.to_lowercase().contains("usage"),
        "{}",
        out.stderr
    );
}

#[test]
fn version_short_and_long() {
    for f in ["-V", "--version"] {
        let out = ig(&[f]);
        assert_eq!(out.code, 0);
        assert!(out.stdout.starts_with("ig "), "{}", out.stdout);
    }
}

#[test]
fn dash_c_and_root_both_pick_the_folder() {
    let dir = repo("dash-c");
    let d = dir.to_str().unwrap();
    assert_eq!(ig(&["-C", d, "plan"]).code, 0);
    assert!(dir.join("ig").is_dir());
    let out = ig(&["check", "--root", d]);
    assert_eq!(out.code, 0, "{}{}", out.stdout, out.stderr);
    let spaced = dir.join("dir with space");
    std::fs::create_dir_all(spaced.join("src")).unwrap();
    std::fs::write(spaced.join("src/a.ts"), "export function f() {}\n").unwrap();
    let out = ig(&["-C", spaced.to_str().unwrap(), "plan"]);
    assert_eq!(out.code, 0, "{}{}", out.stdout, out.stderr);
}

#[test]
fn plan_and_check_print_json() {
    let dir = repo("json");
    assert_eq!(ig_in(&dir, &["plan"]).code, 0);
    for cmd in ["plan", "check"] {
        let out = ig_in(&dir, &[cmd, "--json"]);
        assert_eq!(out.code, 0, "{}", out.stderr);
        let v: serde_json::Value = serde_json::from_str(&out.stdout).unwrap();
        for key in ["build", "move", "drift", "notRead"] {
            assert!(v[key].is_array(), "{key}: {}", out.stdout);
        }
    }
}

#[test]
fn quiet_silences_notices_not_results() {
    let dir = repo("quiet");
    assert_eq!(ig_in(&dir, &["plan"]).code, 0);
    std::fs::create_dir_all(dir.join("lib")).unwrap();
    std::fs::write(dir.join("lib/c.ts"), "export const y = 2;\n").unwrap();
    let out = ig_in(&dir, &["-q", "check"]);
    assert_eq!(out.code, 1);
    assert!(out.stderr.is_empty(), "{}", out.stderr);
    assert!(out.stdout.contains("drift"), "{}", out.stdout);
}

#[test]
fn double_dash_ends_options() {
    let dir = repo("dashdash");
    assert_eq!(ig_in(&dir, &["plan"]).code, 0);
    let out = ig_in(&dir, &["accept", "--", "web->api"]);
    assert_ne!(out.code, 0);
    assert!(out.stderr.contains("web->api"), "{}", out.stderr);
    assert!(
        !out.stderr.contains("unexpected argument"),
        "{}",
        out.stderr
    );
}

#[test]
fn a_bare_directory_points_to_ig_app() {
    let dir = repo("bare-dir");
    let out = ig_in(&dir, &["."]);
    assert_eq!(out.code, 2);
    assert!(out.stderr.contains("use `ig app .`"), "{}", out.stderr);
}

#[test]
fn completions_for_each_shell() {
    for sh in ["bash", "zsh", "fish"] {
        let out = ig(&["completions", sh]);
        assert_eq!(out.code, 0, "{sh}: {}", out.stderr);
        assert!(out.stdout.contains("view"), "{sh}");
    }
}

#[test]
fn removed_hook_still_answers() {
    let out = ig(&["hook"]);
    assert_eq!(out.code, 0);
    assert!(out.stderr.contains("ig hook was removed"), "{}", out.stderr);
}
