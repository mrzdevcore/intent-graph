#![cfg(unix)]

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

fn shop(tag: &str) -> (PathBuf, PathBuf) {
    let base = std::env::temp_dir().join(format!("ig-explore-{tag}-{}", std::process::id()));
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

fn fake_claude(bin: &Path) {
    let d = bin.display();
    let script = format!(
        "#!/bin/sh\nfor a in \"$@\"; do printf '%s\\n' \"$a\"; done > {d}/argv\n\
         /bin/cat > {d}/prompt\necho 'The answer.'\n"
    );
    let p = bin.join("claude");
    std::fs::write(&p, script).unwrap();
    std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
}

fn ig(root: &Path, path: &Path, args: &[&str]) -> (i32, String, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_ig"))
        .args(args)
        .arg("-C")
        .arg(root)
        .env("PATH", path)
        .output()
        .unwrap();
    (
        out.status.code().unwrap(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

#[test]
fn explore_answers_and_draws() {
    let (root, bin) = shop("answers");
    fake_claude(&bin);
    let (code, out, err) = ig(&root, &bin, &["explore", "how", "--no-open"]);
    assert_eq!(code, 0, "{out}{err}");
    assert!(out.starts_with("The answer."), "{out}");
    assert_eq!(out.trim_end(), "The answer.");
    let argv = std::fs::read_to_string(bin.join("argv")).unwrap();
    assert!(argv.contains("--strict-mcp-config"), "{argv}");
    assert!(argv.contains("mcp__ig__ig_show"), "{argv}");
    assert!(argv.contains("Read(**/.env*)"), "{argv}");
    // with -p nobody answers a permission prompt: an MCP tool must be allowed up front
    assert!(
        argv.contains("--allowed-tools\nmcp__ig__ig_state,mcp__ig__ig_show\n"),
        "{argv}"
    );
    for banned in ["ig_edit", "ig_plan", "Bash", "--safe-mode"] {
        assert!(!argv.contains(banned), "{banned} in {argv}");
    }
    let prompt = std::fs::read_to_string(bin.join("prompt")).unwrap();
    assert!(
        prompt.contains("how") && !prompt.contains("{{question}}"),
        "{prompt}"
    );
    let view: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(root.join(".ig/view.json")).unwrap())
            .unwrap();
    assert!(view["boxes"].as_array().is_some_and(|b| !b.is_empty()));
    assert!(root.join(".ig/view.html").exists());
    assert!(!root.join("ig").exists());
    let _ = std::fs::remove_dir_all(root.parent().unwrap());
}

#[test]
fn explore_needs_a_question() {
    let (root, bin) = shop("question");
    fake_claude(&bin);
    let (code, _, err) = ig(&root, &bin, &["explore"]);
    assert_eq!(code, 2, "{err}");
    assert!(!bin.join("argv").exists());
    let _ = std::fs::remove_dir_all(root.parent().unwrap());
}

#[test]
fn explore_refuses_other_agents() {
    let (root, bin) = shop("codex");
    fake_claude(&bin);
    let (code, out, err) = ig(&root, &bin, &["explore", "how", "--agent", "codex"]);
    assert_eq!(code, 2, "{err}");
    assert!(err.contains("explore needs claude for now"), "{err}");
    assert!(out.is_empty(), "{out}");
    let _ = std::fs::remove_dir_all(root.parent().unwrap());
}

#[test]
fn explore_without_claude_on_path() {
    let (root, bin) = shop("nopath");
    let (code, out, err) = ig(&root, &bin, &["explore", "how", "--no-open"]);
    assert_eq!(code, 2, "{err}");
    assert!(err.contains("claude"), "{err}");
    assert!(out.is_empty(), "{out}");
    let _ = std::fs::remove_dir_all(root.parent().unwrap());
}
