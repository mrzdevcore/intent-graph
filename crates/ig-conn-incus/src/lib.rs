//! Reads a running Incus server through the `incus` client, read-only. Only the instance's name,
//! type, status and global addresses are ever deserialized: `config`, `devices` and the rest of
//! the output are never looked at.

use ig_core::ports::{
    Connector, Files, PortError, Resource, ResourceId, ResourceKind, Settings, Snapshot, State,
};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::io::Read;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

// A real Incus server was seen taking 10s to list, so 5s could never read it.
const DEADLINE: Duration = Duration::from_secs(30);
/// Interfaces that hold a container runtime's own addresses, the same on every instance.
const SKIPPED: [&str; 5] = ["docker", "br-", "cni", "flannel", "veth"];

pub struct IncusConnector;

#[derive(Deserialize)]
struct Instance {
    name: String,
    status: Option<String>,
    state: Option<InstState>,
}

#[derive(Deserialize)]
struct InstState {
    network: Option<BTreeMap<String, Iface>>,
}

#[derive(Deserialize)]
struct Iface {
    addresses: Option<Vec<Addr>>,
}

#[derive(Deserialize)]
struct Addr {
    address: String,
    scope: String,
}

/// The instances of `json` (the output of `incus list --format json`) on `remote`.
pub fn parse(remote: &str, json: &str) -> Result<Snapshot, PortError> {
    let list: Vec<Instance> = serde_json::from_str(json)
        .map_err(|_| PortError::Io(format!("incus {remote}: could not read the instance list")))?;
    let resources = list
        .into_iter()
        .map(|i| {
            let addresses = i
                .state
                .and_then(|s| s.network)
                .into_iter()
                .flatten()
                .filter(|(n, _)| n != "lo" && !SKIPPED.iter().any(|p| n.starts_with(p)))
                .flat_map(|(_, f)| f.addresses.into_iter().flatten())
                .filter(|a| a.scope == "global")
                .map(|a| a.address)
                .collect();
            Resource {
                id: ResourceId {
                    connector: "incus".into(),
                    remote: remote.into(),
                    name: i.name.clone(),
                },
                title: i.name,
                kind: ResourceKind::Service,
                parent: None,
                state: match i.status.as_deref() {
                    Some("Running") => State::Running,
                    Some("Stopped") => State::Stopped,
                    _ => State::Unknown,
                },
                addresses,
            }
        })
        .collect();
    Ok(Snapshot {
        resources,
        links: vec![],
    })
}

/// `<program> list <remote>: --format json`, stopped after `deadline`. Output never reaches an
/// error, except the first line of stderr.
fn run_with(program: &str, remote: &str, deadline: Duration) -> Result<String, PortError> {
    let err = |m: String| PortError::Io(format!("incus {remote}: {m}"));
    let mut child = Command::new(program)
        .args(["list", &format!("{remote}:"), "--format", "json"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| err(format!("could not start {program}: {e}")))?;
    let (out, errs) = (child.stdout.take().unwrap(), child.stderr.take().unwrap());
    let (tx, rx) = std::sync::mpsc::channel();
    let tx2 = tx.clone();
    std::thread::spawn(move || tx.send((true, slurp(out))));
    std::thread::spawn(move || tx2.send((false, slurp(errs))));
    let end = Instant::now() + deadline;
    let status = loop {
        match child.try_wait() {
            Ok(Some(s)) => break s,
            Ok(None) if Instant::now() > end => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(err(format!("took over {}s; stopped", deadline.as_secs())));
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(20)),
            Err(e) => return Err(err(e.to_string())),
        }
    };
    let (mut stdout, mut stderr) = (String::new(), String::new());
    for _ in 0..2 {
        match rx.recv_timeout(end.saturating_duration_since(Instant::now())) {
            Ok((true, s)) => stdout = s,
            Ok((false, s)) => stderr = s,
            Err(_) => return Err(err("took too long; stopped".into())),
        }
    }
    if !status.success() {
        let line = stderr.lines().next().unwrap_or("").trim();
        return Err(err(format!("failed ({status}): {line}")));
    }
    Ok(stdout)
}

fn slurp(mut r: impl Read) -> String {
    let mut b = Vec::new();
    let _ = r.read_to_end(&mut b);
    String::from_utf8_lossy(&b).into_owned()
}

/// `[A-Za-z0-9][A-Za-z0-9_-]*`
fn valid_remote(r: &str) -> bool {
    r.chars().next().is_some_and(|c| c.is_ascii_alphanumeric())
        && r.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

impl Connector for IncusConnector {
    fn kind(&self) -> &'static str {
        "incus"
    }
    fn read(&self, settings: &Settings, _files: &dyn Files) -> Result<Snapshot, PortError> {
        let remote = settings.get("remote").map(String::as_str).unwrap_or("");
        if !valid_remote(remote) {
            return Err(PortError::Io(format!(
                "incus: remote {remote:?} must be letters, digits, _ or -"
            )));
        }
        parse(remote, &run_with("incus", remote, DEADLINE)?)
    }
}

#[cfg(test)]
mod tests;
