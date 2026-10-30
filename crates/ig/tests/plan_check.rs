//! `ig plan` and `ig check` on temporary copies of a tiny repo.

use std::path::{Path, PathBuf};
use std::process::Command;

struct Out {
    code: i32,
    stdout: String,
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
    }
}

#[test]
fn check_passes_on_a_fresh_repo_then_fails_on_drift() {
    let dir = repo("drift", &[("src/a.ts", "export function f() {}\n")]);
    assert_eq!(run_ig(&dir, &["check"]).code, 0);
    // the generated library claims `**`; narrow it so that new code outside src/ is drift
    let lib = std::fs::read_dir(dir.join("ig"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .find(|p| {
            std::fs::read_to_string(p).unwrap().starts_with(
                "// ig's intended architecture: edit freely; ig never rewrites this file.\nlibrary",
            )
        })
        .unwrap();
    let text = std::fs::read_to_string(&lib).unwrap();
    std::fs::write(&lib, text.replacen("at \"**\"", "at \"src/**\"", 1)).unwrap();
    assert_eq!(run_ig(&dir, &["check"]).code, 0);
    std::fs::create_dir_all(dir.join("lib")).unwrap();
    std::fs::write(dir.join("lib/c.ts"), "export const y = 2;\n").unwrap();
    let out = run_ig(&dir, &["check"]);
    assert_eq!(out.code, 1);
    assert!(
        out.stdout.contains("drift: code under lib/"),
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
fn check_exits_2_on_a_broken_spec() {
    let dir = repo("broken", &[("src/a.ts", "export function f() {}\n")]);
    assert_eq!(run_ig(&dir, &["check"]).code, 0);
    let f = std::fs::read_dir(dir.join("ig"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    std::fs::write(f, "{{ not kdl").unwrap();
    assert_eq!(run_ig(&dir, &["check"]).code, 2);
}
