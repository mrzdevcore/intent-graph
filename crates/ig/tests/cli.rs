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
        assert!(out.stdout.contains("--no-open"), "{args:?}: {}", out.stdout);
        assert!(!out.stdout.contains("--read"), "{args:?}: {}", out.stdout);
        assert!(!out.stdout.contains("accept"), "{args:?}: {}", out.stdout);
    }
}

#[test]
fn top_help_lists_commands_and_hides_nothing_needed() {
    let out = ig(&["--help"]);
    assert_eq!(out.code, 0);
    for cmd in [
        "init", "analyse", "view", "plan", "check", "accept", "fmt", "app", "mcp",
    ] {
        assert!(out.stdout.contains(cmd), "{cmd}: {}", out.stdout);
    }
    let at = |s: &str| out.stdout.find(&format!("\n  {s} ")).unwrap();
    assert!(at("init") < at("analyse") && at("analyse") < at("view"));
    assert!(
        !out.stdout.contains("ig/ and the agent skills"),
        "{}",
        out.stdout
    );
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
    let out = ig(&["analyse", "--agent", "gpt"]);
    assert_eq!(out.code, 2);
    assert!(out.stderr.contains("claude"), "{}", out.stderr);
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
    assert_eq!(ig(&["-C", d, "analyse", "--no-agent"]).code, 0);
    assert!(dir.join("ig").is_dir());
    let out = ig(&["check", "--root", d]);
    assert_eq!(out.code, 0, "{}{}", out.stdout, out.stderr);
    let spaced = dir.join("dir with space");
    std::fs::create_dir_all(spaced.join("src")).unwrap();
    std::fs::write(spaced.join("src/a.ts"), "export function f() {}\n").unwrap();
    let out = ig(&["-C", spaced.to_str().unwrap(), "analyse", "--no-agent"]);
    assert_eq!(out.code, 0, "{}{}", out.stdout, out.stderr);
}

#[test]
fn plan_and_check_print_json() {
    let dir = repo("json");
    assert_eq!(ig_in(&dir, &["analyse", "--no-agent"]).code, 0);
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
    assert_eq!(ig_in(&dir, &["analyse", "--no-agent"]).code, 0);
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
    assert_eq!(ig_in(&dir, &["analyse", "--no-agent"]).code, 0);
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

fn files(dir: &Path) -> Vec<(PathBuf, String)> {
    let mut out: Vec<(PathBuf, String)> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .map(|p| {
            let t = std::fs::read_to_string(&p).unwrap();
            (p, t)
        })
        .collect();
    out.sort();
    out
}

#[test]
fn plan_without_ig_writes_nothing_and_points_to_analyse() {
    let dir = repo("plan-no-ig");
    let out = ig_in(&dir, &["plan"]);
    assert_eq!(out.code, 2);
    assert!(out.stderr.contains("ig analyse"), "{}", out.stderr);
    assert!(out.stdout.is_empty(), "{}", out.stdout);
    assert!(!dir.join("ig").exists());
}

#[test]
fn view_never_writes_ig() {
    let dir = std::env::temp_dir().join(format!("ig-cli-empty-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let out = ig_in(&dir, &["view", "--no-open"]);
    assert_eq!(out.code, 0, "{}", out.stderr);
    assert!(!dir.join("ig").exists());
    assert!(out.stderr.contains("run `ig analyse`"), "{}", out.stderr);
}

#[test]
fn analyse_twice_without_agent_changes_nothing() {
    let dir = repo("twice");
    let out = ig_in(&dir, &["analyse", "--no-agent"]);
    assert_eq!(out.code, 0, "{}", out.stderr);
    assert!(out.stdout.is_empty(), "{}", out.stdout);
    assert_eq!(
        out.stderr.lines().last(),
        Some("next: ig view, then commit ig/"),
        "{}",
        out.stderr
    );
    let before = files(&dir.join("ig"));
    let out = ig_in(&dir, &["analyse", "--no-agent"]);
    assert_eq!(out.code, 0, "{}", out.stderr);
    assert!(out.stderr.contains("nothing to propose"), "{}", out.stderr);
    assert!(!out.stderr.contains("next:"), "{}", out.stderr);
    assert_eq!(files(&dir.join("ig")), before);
}

#[test]
fn analyse_on_broken_ig_writes_nothing() {
    let dir = repo("broken-ig");
    std::fs::create_dir_all(dir.join("ig")).unwrap();
    std::fs::write(dir.join("ig/system.kdl"), "system \"x\" {").unwrap();
    let out = ig_in(&dir, &["analyse", "--no-agent"]);
    assert_eq!(out.code, 2);
    assert!(out.stderr.contains("system.kdl"), "{}", out.stderr);
    assert_eq!(
        files(&dir.join("ig")),
        [(dir.join("ig/system.kdl"), "system \"x\" {".to_string())]
    );
}

#[test]
fn view_read_points_to_analyse() {
    let dir = repo("view-read");
    for args in [
        &["view", "--read"][..],
        &["view", "--agent", "claude"],
        &["view", "--read", "--no-read"],
    ] {
        let out = ig_in(&dir, args);
        assert_eq!(out.code, 2, "{args:?}");
        assert!(
            out.stderr.contains("the reading moved to `ig analyse`"),
            "{args:?}: {}",
            out.stderr
        );
    }
    assert!(!dir.join("ig").exists());
}

#[test]
fn view_no_read_still_views_with_a_notice() {
    let dir = repo("no-read");
    let out = ig_in(&dir, &["view", "--no-read", "--no-open"]);
    assert_eq!(out.code, 0, "{}", out.stderr);
    assert!(
        out.stderr
            .contains("--no-read is the default now; run `ig init` to update the skills"),
        "{}",
        out.stderr
    );
    assert!(
        out.stdout.trim_end().ends_with("view.html"),
        "{}",
        out.stdout
    );
    assert!(!dir.join("ig").exists());
}

#[test]
fn init_does_not_write_ig() {
    let dir = repo("init");
    let out = ig_in(&dir, &["init"]);
    assert_eq!(out.code, 0, "{}", out.stderr);
    assert!(!dir.join("ig").exists());
    assert_eq!(
        out.stderr.lines().last(),
        Some("next: ig analyse"),
        "{}",
        out.stderr
    );
}

#[test]
fn init_notices_go_to_stderr_and_quiet_drops_them() {
    let dir = repo("init-quiet");
    let out = ig_in(&dir, &["init", "claude"]);
    assert_eq!(out.code, 0, "{}", out.stderr);
    assert!(out.stdout.contains("created "), "{}", out.stdout);
    assert!(!out.stdout.contains("trust"), "{}", out.stdout);
    assert!(out.stderr.contains("trust this folder"), "{}", out.stderr);
    let out = ig_in(&dir, &["init", "claude"]);
    assert_eq!(out.code, 0, "{}", out.stderr);
    assert!(out.stdout.is_empty(), "{}", out.stdout);
    assert!(
        out.stderr.contains("ig init: nothing to do"),
        "{}",
        out.stderr
    );
    let out = ig_in(&dir, &["-q", "init", "claude"]);
    assert_eq!(out.code, 0, "{}", out.stderr);
    assert!(out.stdout.is_empty(), "{}", out.stdout);
    assert!(out.stderr.is_empty(), "{}", out.stderr);
}

#[test]
fn quiet_still_says_why_ig_does_not_read() {
    let dir = repo("quiet-broken");
    std::fs::create_dir_all(dir.join("ig")).unwrap();
    std::fs::write(dir.join("ig/system.kdl"), "system \"x\" {").unwrap();
    for args in [
        &["-q", "check"][..],
        &["-q", "plan"],
        &["-q", "analyse", "--no-agent"],
    ] {
        let out = ig_in(&dir, args);
        assert_eq!(out.code, 2, "{args:?}");
        assert!(
            out.stderr.contains("system.kdl"),
            "{args:?}: {}",
            out.stderr
        );
    }
}

#[test]
fn analyze_is_analyse() {
    let dir = repo("analyze");
    let out = ig_in(&dir, &["analyze", "--no-agent"]);
    assert_eq!(out.code, 0, "{}", out.stderr);
    assert!(dir.join("ig/system.kdl").is_file());
}
