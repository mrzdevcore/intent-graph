use super::*;
use ig_core::testing::MemFiles;
use std::os::unix::fs::PermissionsExt;

const FIXTURE: &str = r#"[
 {"name":"alpha","type":"container","status":"Running","config":{"k":"SECRET_CANARY"},
  "expanded_config":{"k":"SECRET_CANARY"},"devices":{"d":"SECRET_CANARY"},
  "state":{"network":{
   "lo":{"addresses":[{"address":"127.0.0.1","scope":"local"}]},
   "eth0":{"addresses":[{"address":"10.0.0.2","scope":"global"},{"address":"fe80::1","scope":"link"}]},
   "docker0":{"addresses":[{"address":"172.17.0.1","scope":"global"}]},
   "br-abc":{"addresses":[{"address":"172.18.0.1","scope":"global"}]}}}},
 {"name":"beta","type":"container","status":"Running","config":{"k":"SECRET_CANARY"},
  "expanded_config":{"k":"SECRET_CANARY"},"devices":{"d":"SECRET_CANARY"},
  "state":{"network":{
   "eth0":{"addresses":[{"address":"10.0.0.3","scope":"global"}]},
   "docker0":{"addresses":[{"address":"172.17.0.1","scope":"global"}]},
   "br-abc":{"addresses":[{"address":"172.18.0.1","scope":"global"}]}}}},
 {"name":"gamma","type":"virtual-machine","status":"Stopped","config":{"k":"SECRET_CANARY"},
  "expanded_config":{"k":"SECRET_CANARY"},"devices":{"d":"SECRET_CANARY"},
  "state":{"network":null}}
]"#;

fn settings(remote: &str) -> Settings {
    Settings::from([("remote".to_string(), remote.to_string())])
}

fn script(name: &str, body: &str) -> String {
    let dir = std::env::temp_dir().join(format!("ig-incus-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let p = dir.join("incus");
    std::fs::write(&p, format!("#!/bin/sh\n{body}\n")).unwrap();
    std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
    p.to_string_lossy().into_owned()
}

#[test]
fn only_the_allowlisted_fields_are_kept() {
    let s = parse("homelab", FIXTURE).unwrap();
    assert!(!format!("{s:?}").contains("SECRET_CANARY"));
    assert_eq!(s.resources[0].id.name, "alpha");
    assert_eq!(s.resources[0].id.remote, "homelab");
    assert_eq!(s.resources[0].title, "alpha");
    assert_eq!(s.resources[0].state, State::Running);
}

#[test]
fn a_stopped_instance_has_no_addresses() {
    let s = parse("homelab", FIXTURE).unwrap();
    assert_eq!(s.resources[2].state, State::Stopped);
    assert!(s.resources[2].addresses.is_empty());
}

#[test]
fn an_unknown_status_is_unknown() {
    let s = parse(
        "h",
        r#"[{"name":"a","type":"container","status":"Frozen"}]"#,
    )
    .unwrap();
    assert_eq!(s.resources[0].state, State::Unknown);
}

#[test]
fn docker_bridge_addresses_are_left_out() {
    let s = parse("homelab", FIXTURE).unwrap();
    assert_eq!(s.resources[0].addresses, ["10.0.0.2"]);
    assert_eq!(s.resources[1].addresses, ["10.0.0.3"]);
}

#[test]
fn a_parse_failure_never_echoes_the_output() {
    let e = parse("homelab", "SECRET_CANARY not json")
        .unwrap_err()
        .to_string();
    assert_eq!(e, "incus homelab: could not read the instance list");
}

#[test]
fn a_bad_remote_is_refused_before_anything_runs() {
    for bad in ["-x", "a b", "a:b", "", "../x"] {
        let e = IncusConnector
            .read(&settings(bad), &MemFiles::default())
            .unwrap_err()
            .to_string();
        assert!(e.contains("remote"), "{bad}: {e}");
    }
}

#[test]
fn a_missing_client_is_a_clear_error() {
    let e = run_with("/no/such/incus", "homelab", DEADLINE)
        .unwrap_err()
        .to_string();
    assert!(e.contains("incus") && e.contains("could not start"), "{e}");
}

#[test]
fn the_client_is_asked_for_the_list_of_the_remote() {
    let p = script("args", r#"echo "$@""#);
    assert_eq!(
        run_with(&p, "homelab", DEADLINE).unwrap().trim(),
        "list homelab: --format json"
    );
}

#[test]
fn a_hanging_client_is_stopped_at_the_deadline() {
    let p = script("hang", "exec sleep 30");
    let t = Instant::now();
    assert!(run_with(&p, "homelab", Duration::from_secs(1)).is_err());
    assert!(t.elapsed() < Duration::from_secs(5));
}

#[test]
fn stderr_shows_its_first_line_only() {
    let p = script(
        "fail",
        "echo SECRET_CANARY; echo first >&2; echo second >&2; exit 1",
    );
    let e = run_with(&p, "homelab", DEADLINE).unwrap_err().to_string();
    assert!(
        e.contains("first") && !e.contains("second") && !e.contains("SECRET_CANARY"),
        "{e}"
    );
}
