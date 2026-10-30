//! `ig view` asking an AI agent to read the architecture, with a fake agent on a temp PATH.
#![cfg(unix)]

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

/// A copy of the shop fixture (outside git) and an empty `bin` for fake agents.
fn shop(tag: &str) -> (PathBuf, PathBuf) {
    let base = std::env::temp_dir().join(format!("ig-agent-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(base.join("bin")).unwrap();
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/shop");
    assert!(
        Command::new("cp")
            .arg("-R")
            .arg(fixture)
            .arg(base.join("shop"))
            .status()
            .unwrap()
            .success()
    );
    (base.join("shop"), base.join("bin"))
}

/// A fake agent `name` in `bin`: it saves each prompt (stdin) as `prompt.N` and prints
/// `replies[N-1]` (the last one once they run out).
fn fake(bin: &Path, name: &str, replies: &[&str]) {
    for (i, r) in replies.iter().enumerate() {
        std::fs::write(bin.join(format!("reply.{}", i + 1)), r).unwrap();
    }
    let d = bin.display();
    let script = format!(
        "#!/bin/sh\nn=1\nwhile [ -e {d}/prompt.$n ]; do n=$((n+1)); done\n\
         /bin/cat > {d}/prompt.$n\n\
         if [ -e {d}/reply.$n ]; then /bin/cat {d}/reply.$n; else /bin/cat {d}/reply.{}; fi\n",
        replies.len()
    );
    let p = bin.join(name);
    std::fs::write(&p, script).unwrap();
    std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
}

/// `ig <args> --root root` with PATH = `path`: (exit code, stdout + stderr).
fn ig(root: &Path, path: &Path, args: &[&str]) -> (i32, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_ig"))
        .args(args)
        .arg("--root")
        .arg(root)
        .env("PATH", path)
        .output()
        .unwrap();
    let text =
        String::from_utf8_lossy(&out.stdout).into_owned() + &String::from_utf8_lossy(&out.stderr);
    (out.status.code().unwrap(), text)
}

/// Every file of `root/ig/`, one after the other, by name.
fn spec(root: &Path) -> String {
    let mut paths: Vec<PathBuf> = std::fs::read_dir(root.join("ig"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    paths.sort();
    paths
        .iter()
        .map(|p| std::fs::read_to_string(p).unwrap())
        .collect()
}

const CHECKOUT: &str = "<ig-annotations>[{\"id\": \"api/application-checkout\", \
    \"description\": \"Takes an order and charges it.\"}]</ig-annotations>";

#[test]
fn agent_annotates() {
    let (root, bin) = shop("annotates");
    fake(&bin, "claude", &[&format!("Here it is.\n{CHECKOUT}\n")]);
    let (code, out) = ig(&root, &bin, &["view", "--read", "--no-open"]);
    assert_eq!(code, 0, "{out}");
    assert!(
        spec(&root).contains("description=\"Takes an order and charges it.\""),
        "{out}"
    );
    let prompt = std::fs::read_to_string(bin.join("prompt.1")).unwrap();
    assert!(prompt.contains("<ig-annotations>"), "{prompt}");
    assert!(prompt.contains("api/application-checkout"), "{prompt}");
    let _ = std::fs::remove_dir_all(root.parent().unwrap());
}

#[test]
fn agent_garbage_still_writes_ig_and_ig_is_not_read_again() {
    let (root, bin) = shop("garbage");
    fake(&bin, "claude", &["I looked at it and it is a nice shop."]);
    let (code, out) = ig(&root, &bin, &["view", "--read", "--no-open"]);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("<ig-annotations>"), "the reason: {out}");
    assert!(bin.join("prompt.2").exists(), "asked twice");
    assert!(
        out.contains("wrote ig/: the intended architecture, from the code"),
        "{out}"
    );
    let before = spec(&root);
    let (code, out) = ig(&root, &bin, &["view", "--read", "--no-open"]);
    assert_eq!(code, 0, "{out}");
    assert!(!bin.join("prompt.3").exists(), "read again: {out}");
    assert!(
        out.contains(
            "the AI reading runs when ig/ is written; it will propose edits in a later version"
        ),
        "{out}"
    );
    assert_eq!(spec(&root), before);
    assert!(!root.join(".ig/map.json").exists());
    let _ = std::fs::remove_dir_all(root.parent().unwrap());
}

#[test]
fn agent_retry_on_errors() {
    let (root, bin) = shop("retry");
    let bad = "<ig-annotations>[{\"id\": \"api/application-checkout\", \
               \"madeOf\": [\"../elsewhere/*\"]}]</ig-annotations>";
    fake(&bin, "claude", &[bad, CHECKOUT]);
    let (code, out) = ig(&root, &bin, &["view", "--read", "--no-open"]);
    assert_eq!(code, 0, "{out}");
    let second = std::fs::read_to_string(bin.join("prompt.2")).unwrap();
    assert!(
        second.contains("must be relative, without `..`"),
        "{second}"
    );
    assert!(
        spec(&root).contains("description=\"Takes an order and charges it.\""),
        "{out}"
    );
    let _ = std::fs::remove_dir_all(root.parent().unwrap());
}

#[test]
fn no_agent_still_views() {
    let (root, _bin) = shop("none");
    let (code, out) = ig(&root, Path::new(""), &["view", "--read", "--no-open"]);
    assert_eq!(code, 0, "{out}");
    assert!(
        out.contains("No AI agent found (claude, gemini or codex): showing ig's own reading."),
        "{out}"
    );
    assert!(root.join(".ig/view.html").exists());
    let _ = std::fs::remove_dir_all(root.parent().unwrap());
}

#[test]
fn no_tty_never_reads() {
    let (root, bin) = shop("notty");
    fake(&bin, "claude", &[CHECKOUT]);
    let (code, out) = ig(&root, &bin, &["view", "--no-open"]);
    assert_eq!(code, 0, "{out}");
    assert!(!bin.join("prompt.1").exists(), "the agent ran: {out}");
    assert!(
        out.contains("boxes are ig's guess; run `ig view --read` to let an AI agent read them."),
        "{out}"
    );
    assert!(!out.contains("No AI agent found"), "{out}");
    let _ = std::fs::remove_dir_all(root.parent().unwrap());
}

#[test]
fn a_named_agent_not_on_path_is_no_agent() {
    let (root, bin) = shop("named");
    fake(&bin, "claude", &[CHECKOUT]);
    let (code, out) = ig(
        &root,
        &bin,
        &["view", "--read", "--agent", "gemini", "--no-open"],
    );
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("No AI agent found"), "{out}");
    assert!(!out.contains("could not start"), "{out}");
    assert!(!bin.join("prompt.1").exists(), "claude ran instead: {out}");
    let _ = std::fs::remove_dir_all(root.parent().unwrap());
}

#[test]
fn read_and_no_read_together_is_a_usage_error() {
    let (root, bin) = shop("both");
    let (code, out) = ig(&root, &bin, &["view", "--read", "--no-read", "--no-open"]);
    assert_eq!(code, 2, "{out}");
    assert!(out.contains("--read and --no-read"), "{out}");
    assert!(!root.join("ig").exists());
    let _ = std::fs::remove_dir_all(root.parent().unwrap());
}

#[test]
fn removed_commands_say_so() {
    let (root, bin) = shop("removed");
    let (code, out) = ig(&root, &bin, &["hook", "claude"]);
    assert_eq!(code, 0, "{out}");
    assert_eq!(
        out,
        "ig hook was removed; delete ig's Stop hook from .claude/settings.json\n"
    );
    let _ = std::fs::remove_dir_all(root.parent().unwrap());
}

/// Outside a git repo, with git on PATH: nothing from git on stderr, and `.ig/view.html` cannot
/// be committed by accident.
#[test]
fn no_git_noise_and_the_page_is_ignored() {
    let (root, bin) = shop("nogit");
    let path = format!("{}:/usr/bin:/bin", bin.display());
    let (code, out) = ig(&root, Path::new(&path), &["view", "--no-read", "--no-open"]);
    assert_eq!(code, 0, "{out}");
    assert!(!out.contains("fatal"), "{out}");
    assert_eq!(
        std::fs::read_to_string(root.join(".ig/.gitignore")).unwrap(),
        "*\n!.gitignore\n"
    );
    // a repo that ignores `.ig/*` itself gets no `.ig/.gitignore`
    std::fs::remove_dir_all(root.join(".ig")).unwrap();
    std::fs::write(root.join(".gitignore"), ".ig/*\n").unwrap();
    let (code, out) = ig(&root, Path::new(&path), &["view", "--no-read", "--no-open"]);
    assert_eq!(code, 0, "{out}");
    assert!(!root.join(".ig/.gitignore").exists());
    let _ = std::fs::remove_dir_all(root.parent().unwrap());
}
