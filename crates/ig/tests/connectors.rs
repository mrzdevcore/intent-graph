//! `ig view` reading connectors, with a fake `incus` on a temp PATH; nothing else calls one.
#![cfg(unix)]

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

const INSTANCES: &str = r#"[{"name":"gitlab","type":"container","status":"Running",
 "expanded_config":{"k":"SECRET_CANARY"},
 "state":{"network":{"eth0":{"addresses":[{"address":"10.0.0.2","scope":"global"}]}}}}]"#;

/// A copy of the shop fixture with `yaml` appended to its `ig.yaml`, and an empty `bin`.
fn shop(tag: &str, yaml: &str) -> (PathBuf, PathBuf) {
    let base = std::env::temp_dir().join(format!("ig-conn-{tag}-{}", std::process::id()));
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
    let root = base.join("shop");
    let mut cfg = std::fs::read_to_string(root.join("ig.yaml")).unwrap();
    cfg.push_str(yaml);
    std::fs::write(root.join("ig.yaml"), cfg).unwrap();
    (root, base.join("bin"))
}

/// A fake `incus` in `bin` that touches `root/called`, then runs `body`.
fn fake_incus(root: &Path, bin: &Path, body: &str) {
    let script = format!(
        "#!/bin/sh\n/usr/bin/touch {}/called\n{body}\n",
        root.display()
    );
    let p = bin.join("incus");
    std::fs::write(&p, script).unwrap();
    std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
}

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

const VIEW: &[&str] = &["view", "--no-read", "--no-open", "--json"];
const INCUS: &str = "connectors:\n  - { kind: incus, remote: homelab }\n";

#[test]
fn view_draws_the_server_and_no_secret_reaches_the_page() {
    let (root, bin) = shop("draws", INCUS);
    fake_incus(&root, &bin, &format!("/bin/cat <<'EOF'\n{INSTANCES}\nEOF"));
    let (code, out) = ig(&root, &bin, VIEW);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("\"state\"") && out.contains("gitlab"), "{out}");
    assert!(!out.contains("SECRET_CANARY") && !out.contains("10.0.0.2"));
    // `--json` never writes the page: run again without it and read the real file.
    let (code, out) = ig(&root, &bin, &["view", "--no-read", "--no-open"]);
    assert_eq!(code, 0, "{out}");
    let page = std::fs::read_to_string(root.join(".ig/view.html")).expect("view.html is written");
    assert!(page.contains("gitlab"), "the page draws the instance");
    assert!(!page.contains("SECRET_CANARY") && !page.contains("10.0.0.2"));
    let spec: String = std::fs::read_dir(root.join("ig"))
        .unwrap()
        .map(|e| std::fs::read_to_string(e.unwrap().path()).unwrap())
        .collect();
    assert!(!spec.contains("gitlab"), "a connector never writes ig/");
}

#[test]
fn a_remote_of_many_instances_is_no_crowding_to_fix() {
    let (root, bin) = shop("crowded", INCUS);
    let list: Vec<String> = (0..13)
        .map(|i| format!(r#"{{"name":"worker{i}","type":"container","status":"Running"}}"#))
        .collect();
    fake_incus(
        &root,
        &bin,
        &format!("/bin/cat <<'EOF'\n[{}]\nEOF", list.join(",")),
    );
    let (code, out) = ig(&root, &bin, VIEW);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("worker12"), "{out}");
    assert!(!out.contains("holds 13 boxes"), "{out}");
}

#[test]
fn plan_and_check_never_call_a_connector() {
    let (root, bin) = shop("plan", INCUS);
    fake_incus(&root, &bin, &format!("/bin/cat <<'EOF'\n{INSTANCES}\nEOF"));
    ig(&root, &bin, &["plan"]);
    ig(&root, &bin, &["check"]);
    assert!(!root.join("called").exists());
}

#[test]
fn an_unreachable_server_is_a_notice_and_the_map_draws() {
    let (root, bin) = shop("down", INCUS);
    fake_incus(&root, &bin, "echo 'server is down' >&2\nexit 1");
    let (code, out) = ig(&root, &bin, VIEW);
    assert_eq!(code, 0, "{out}");
    assert!(
        out.contains("server is down") && out.contains("\"boxes\""),
        "{out}"
    );
}

#[test]
fn a_missing_client_is_a_notice_and_the_map_draws() {
    let (root, bin) = shop("missing", INCUS);
    let (code, out) = ig(&root, &bin, VIEW);
    assert_eq!(code, 0, "{out}");
    assert!(
        out.contains("could not start") && out.contains("\"boxes\""),
        "{out}"
    );
}

#[test]
fn an_unknown_connector_kind_is_a_notice() {
    let (root, bin) = shop("unknown", "connectors:\n  - { kind: nope, remote: x }\n");
    let (code, out) = ig(&root, &bin, VIEW);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("nope x: no connector of that kind"), "{out}");
}

#[test]
fn no_connectors_changes_nothing() {
    let (root, bin) = shop("none", "");
    fake_incus(&root, &bin, &format!("/bin/cat <<'EOF'\n{INSTANCES}\nEOF"));
    let (code, out) = ig(&root, &bin, VIEW);
    assert_eq!(code, 0, "{out}");
    assert!(!out.contains("gitlab") && !root.join("called").exists());
}
