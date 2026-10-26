//! The per-project trust gate: what a Claude Code session in a project would run (hooks, agent
//! instructions, MCP servers, the ig plugin), a hash of it, and the user's stored decision.

use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

const SETTINGS: [&str; 2] = [".claude/settings.json", ".claude/settings.local.json"];
const MCP: &str = ".mcp.json";
const INSTRUCTIONS: [&str; 3] = ["AGENTS.md", "CLAUDE.md", ".claude/CLAUDE.md"];
const PLUGIN: &str = ".claude/skills/ig";

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct HookSummary {
    pub event: String,
    pub command: String,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct Inventory {
    pub hooks: Vec<HookSummary>,
    /// AGENTS.md / CLAUDE.md files that exist, relative.
    pub instructions: Vec<String>,
    /// MCP server names, plus "unreadable <file>" for a file that does not parse.
    pub mcp: Vec<String>,
    /// Settings files that do not parse: listed, and still hashed.
    pub unreadable: Vec<String>,
    pub ig_plugin: bool,
    pub hash: String,
}

fn files_under(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            files_under(&p, out);
        } else {
            out.push(p);
        }
    }
}

fn json(root: &Path, rel: &str) -> Option<Result<Value, ()>> {
    let bytes = std::fs::read(root.join(rel)).ok()?;
    Some(serde_json::from_slice(&bytes).map_err(|_| ()))
}

pub fn inventory(root: &Path) -> Inventory {
    let (mut hooks, mut mcp, mut unreadable) = (vec![], vec![], vec![]);
    for rel in SETTINGS {
        match json(root, rel) {
            Some(Ok(v)) => {
                for (event, groups) in v["hooks"].as_object().into_iter().flatten() {
                    for h in groups
                        .as_array()
                        .into_iter()
                        .flatten()
                        .flat_map(|g| g["hooks"].as_array().into_iter().flatten())
                    {
                        let command = h["command"].as_str().unwrap_or("(not a command)");
                        hooks.push(HookSummary {
                            event: event.clone(),
                            command: command.to_string(),
                        });
                    }
                }
            }
            Some(Err(())) => unreadable.push(rel.to_string()),
            None => {}
        }
    }
    match json(root, MCP) {
        Some(Ok(v)) => mcp.extend(
            v["mcpServers"]
                .as_object()
                .into_iter()
                .flatten()
                .map(|(k, _)| k.clone()),
        ),
        Some(Err(())) => {
            unreadable.push(MCP.to_string());
            mcp.push(format!("unreadable {MCP}"));
        }
        None => {}
    }
    let instructions = INSTRUCTIONS
        .iter()
        .filter(|r| root.join(r).is_file())
        .map(|r| r.to_string())
        .collect();
    let mut skill = vec![];
    files_under(&root.join(PLUGIN), &mut skill);
    // Path-sorted; an absent file hashes as absent, so adding one changes the hash.
    let mut rels: BTreeMap<String, Option<Vec<u8>>> = SETTINGS
        .iter()
        .chain(&[MCP])
        .chain(&INSTRUCTIONS)
        .map(|r| (r.to_string(), std::fs::read(root.join(r)).ok()))
        .collect();
    for p in skill {
        if let Ok(rel) = p.strip_prefix(root) {
            rels.insert(rel.to_string_lossy().into_owned(), std::fs::read(&p).ok());
        }
    }
    let mut h = Sha256::new();
    for (rel, bytes) in &rels {
        h.update((rel.len() as u64).to_le_bytes());
        h.update(rel.as_bytes());
        match bytes {
            Some(b) => {
                h.update([1]);
                h.update((b.len() as u64).to_le_bytes());
                h.update(b);
            }
            None => h.update([0]),
        }
    }
    Inventory {
        hooks,
        instructions,
        mcp,
        unreadable,
        ig_plugin: root.join(PLUGIN).is_dir(),
        hash: h.finalize().iter().map(|b| format!("{b:02x}")).collect(),
    }
}

fn store(data_dir: &Path) -> PathBuf {
    data_dir.join("trust.json")
}

fn decisions(data_dir: &Path) -> BTreeMap<String, String> {
    std::fs::read(store(data_dir))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

fn key(root: &Path) -> String {
    root.canonicalize()
        .unwrap_or_else(|_| root.to_path_buf())
        .to_string_lossy()
        .into_owned()
}

pub fn is_trusted(data_dir: &Path, root: &Path, inv: &Inventory) -> bool {
    decisions(data_dir).get(&key(root)) == Some(&inv.hash)
}

pub fn trust(data_dir: &Path, root: &Path, inv: &Inventory) -> std::io::Result<()> {
    let mut d = decisions(data_dir);
    d.insert(key(root), inv.hash.clone());
    std::fs::create_dir_all(data_dir)?;
    std::fs::write(store(data_dir), serde_json::to_vec_pretty(&d)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> (PathBuf, PathBuf) {
        let base = std::env::temp_dir().join(format!("ig-trust-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let (root, data) = (base.join("p"), base.join("data"));
        std::fs::create_dir_all(root.join(".claude")).unwrap();
        (root, data)
    }

    #[test]
    fn a_change_to_settings_revokes_trust() {
        let (root, data) = scratch("settings");
        let s = root.join(".claude/settings.json");
        std::fs::write(
            &s,
            r#"{"hooks":{"Stop":[{"hooks":[{"type":"command","command":"ig hook claude"}]}]}}"#,
        )
        .unwrap();
        let inv = inventory(&root);
        assert_eq!(
            inv.hooks,
            [HookSummary {
                event: "Stop".into(),
                command: "ig hook claude".into()
            }]
        );
        assert!(!is_trusted(&data, &root, &inv));
        trust(&data, &root, &inv).unwrap();
        assert!(is_trusted(&data, &root, &inventory(&root)));
        std::fs::write(
            &s,
            r#"{"hooks":{"Stop":[{"hooks":[{"type":"command","command":"evil"}]}]}}"#,
        )
        .unwrap();
        assert!(!is_trusted(&data, &root, &inventory(&root)));
    }

    #[test]
    fn adding_a_file_or_breaking_json_changes_the_hash_without_crashing() {
        let (root, _) = scratch("add");
        let h0 = inventory(&root).hash;
        std::fs::write(root.join("AGENTS.md"), "x").unwrap();
        let h1 = inventory(&root);
        assert_ne!(h0, h1.hash);
        assert_eq!(h1.instructions, ["AGENTS.md"]);
        std::fs::write(root.join(".mcp.json"), "{oops").unwrap();
        let h2 = inventory(&root);
        assert_ne!(h1.hash, h2.hash);
        assert_eq!(h2.mcp, ["unreadable .mcp.json"]);
    }
}
