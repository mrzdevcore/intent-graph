//! Unreadable `ig.yaml` and `.ig/baseline.json` are errors, never read as missing (which would
//! drop the rules, or let `ig except` overwrite the baseline).
#![cfg(unix)]

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("ig-unreadable-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join(".ig")).unwrap();
    std::fs::write(dir.join("a.ts"), "export const a = 1;\n").unwrap();
    dir
}

/// Makes `path` unreadable; false when that does not stop reads (running as root).
fn lock(path: &Path) -> bool {
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o000)).unwrap();
    std::fs::read(path).is_err()
}

fn ig(root: &Path, args: &[&str]) -> (i32, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_ig"))
        .args(args)
        .arg("--root")
        .arg(root)
        .output()
        .unwrap();
    (
        out.status.code().unwrap(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

#[test]
fn unreadable_config_fails_the_check() {
    let dir = scratch("config");
    let cfg = dir.join("ig.yaml");
    std::fs::write(&cfg, "rules: []\n").unwrap();
    if lock(&cfg) {
        let (code, err) = ig(&dir, &["check"]);
        assert_eq!(code, 2, "{err}");
        assert!(err.contains("ig: ig.yaml: cannot be read"), "{err}");
    }
    std::fs::set_permissions(&cfg, std::fs::Permissions::from_mode(0o644)).unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn except_keeps_an_unreadable_baseline() {
    let dir = scratch("baseline");
    let b = dir.join(".ig/baseline.json");
    let before = b"[\"keep-me\"]\n";
    std::fs::write(&b, before).unwrap();
    if lock(&b) {
        let (code, err) = ig(&dir, &["except", "fp", "--reason", "x"]);
        assert_eq!(code, 2, "{err}");
        assert!(err.contains("baseline.json: cannot be read"), "{err}");
    }
    std::fs::set_permissions(&b, std::fs::Permissions::from_mode(0o644)).unwrap();
    assert_eq!(std::fs::read(&b).unwrap(), before);
    let _ = std::fs::remove_dir_all(&dir);
}
