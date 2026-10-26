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
    /// The event in plain words ("after each Claude turn").
    pub when: String,
    pub command: String,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct Inventory {
    pub hooks: Vec<HookSummary>,
    /// AGENTS.md / CLAUDE.md files that exist, relative.
    pub instructions: Vec<String>,
    /// MCP servers with their command and args, plus "unreadable <file>" for a file that does not parse.
    pub mcp: Vec<String>,
    /// Settings files that do not parse: listed, and still hashed.
    pub unreadable: Vec<String>,
    /// Plain-words effects of the settings files other than hooks (permissions, helpers, env, ...).
    pub notes: Vec<String>,
    /// Any hashed file exists: if not, nothing runs on its own.
    pub any_files: bool,
    pub ig_plugin: bool,
    pub hash: String,
}

/// One hashed path's content, read once: the displayed inventory and the hash both come from it.
enum Content {
    Absent,
    File(Vec<u8>),
    /// A symlink: its target path, never followed.
    Link(Vec<u8>),
    /// A directory entry that is neither (FIFO, socket, ...): not read.
    Other,
}

fn raw(p: &Path) -> Vec<u8> {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        p.as_os_str().as_bytes().to_vec()
    }
    #[cfg(not(unix))]
    {
        p.to_string_lossy().into_owned().into_bytes()
    }
}

fn read_one(p: &Path) -> Content {
    match std::fs::symlink_metadata(p) {
        Err(_) => Content::Absent,
        Ok(m) if m.is_symlink() => {
            Content::Link(std::fs::read_link(p).map(|t| raw(&t)).unwrap_or_default())
        }
        Ok(m) if m.is_file() => std::fs::read(p).map_or(Content::Absent, Content::File),
        Ok(_) => Content::Other,
    }
}

/// Every file under `dir`; symlinked directories are listed as links, not entered.
fn files_under(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for e in rd.flatten() {
        let p = e.path();
        match e.file_type() {
            Ok(t) if t.is_dir() => files_under(&p, out),
            _ => out.push(p),
        }
    }
}

fn hook_when(event: &str) -> String {
    match event {
        "Stop" => "after each Claude turn",
        "SubagentStop" => "after each sub-agent finishes",
        "PreToolUse" => "before each tool use",
        "PostToolUse" => "after each tool use",
        "UserPromptSubmit" => "when you send a prompt",
        "SessionStart" => "when a session starts",
        "SessionEnd" => "when a session ends",
        "Notification" => "on each notification",
        "PreCompact" => "before the conversation is compacted",
        other => other,
    }
    .to_string()
}

fn names(v: &Value) -> Vec<String> {
    match v {
        Value::Object(o) => o.keys().cloned().collect(),
        Value::Array(a) => a
            .iter()
            .map(|x| x.as_str().map_or(x.to_string(), str::to_string))
            .collect(),
        _ => vec![],
    }
}

/// Plain-words effects of one settings file other than its hooks.
fn settings_notes(rel: &str, v: &Value, hooks: &mut Vec<HookSummary>, notes: &mut Vec<String>) {
    let Some(o) = v.as_object() else {
        notes.push(format!("{rel}: not a JSON object"));
        return;
    };
    for (k, val) in o {
        let list = |x: &Value| names(x).join(", ");
        match k.as_str() {
            "hooks" => {
                for (event, groups) in val.as_object().into_iter().flatten() {
                    for h in groups
                        .as_array()
                        .into_iter()
                        .flatten()
                        .flat_map(|g| g["hooks"].as_array().into_iter().flatten())
                    {
                        hooks.push(HookSummary {
                            event: event.clone(),
                            when: hook_when(event),
                            command: h["command"]
                                .as_str()
                                .unwrap_or("(not a command)")
                                .to_string(),
                        });
                    }
                }
            }
            "permissions" => {
                for (pk, pv) in val.as_object().into_iter().flatten() {
                    match (pk.as_str(), pv.as_str()) {
                        ("allow", _) => {
                            notes.push(format!("{rel}: allows these without asking: {}", list(pv)))
                        }
                        ("defaultMode", Some(m @ ("bypassPermissions" | "acceptEdits"))) => notes
                            .push(format!(
                                "{rel}: DEFAULT MODE {m}: Claude acts without asking for permission"
                            )),
                        ("defaultMode", m) => notes.push(format!(
                            "{rel}: default permission mode {}",
                            m.unwrap_or("(not text)")
                        )),
                        _ => notes.push(format!("{rel}: sets permissions.{pk}")),
                    }
                }
            }
            "env" => notes.push(format!("{rel}: sets environment variables: {}", list(val))),
            "enabledPlugins" => notes.push(format!("{rel}: enables plugins: {}", list(val))),
            "extraKnownMarketplaces" => {
                notes.push(format!("{rel}: adds plugin marketplaces: {}", list(val)))
            }
            "enableAllProjectMcpServers" => {
                notes.push(format!("{rel}: enableAllProjectMcpServers = {val}"))
            }
            k if k == "apiKeyHelper" || k.ends_with("Helper") => notes.push(format!(
                "{rel}: runs: {}",
                val.as_str().unwrap_or("(not a command)")
            )),
            other => notes.push(format!("{rel}: also sets {other}")),
        }
    }
}

pub fn inventory(root: &Path) -> Inventory {
    let (mut hooks, mut mcp, mut notes, mut unreadable) = (vec![], vec![], vec![], vec![]);
    let mut skill = vec![];
    files_under(&root.join(PLUGIN), &mut skill);
    // Each file is read once, path-sorted; an absent file hashes as absent, so adding one changes the hash.
    let mut files: BTreeMap<PathBuf, Content> = SETTINGS
        .iter()
        .chain(&[MCP])
        .chain(&INSTRUCTIONS)
        .map(|r| (PathBuf::from(r), read_one(&root.join(r))))
        .collect();
    for p in skill {
        if let Ok(rel) = p.strip_prefix(root) {
            files.insert(rel.to_path_buf(), read_one(&p));
        }
    }
    let parsed = |rel: &str| match &files[Path::new(rel)] {
        Content::File(b) => Some(serde_json::from_slice::<Value>(b).map_err(|_| ())),
        _ => None,
    };
    for rel in SETTINGS {
        match parsed(rel) {
            Some(Ok(v)) => settings_notes(rel, &v, &mut hooks, &mut notes),
            Some(Err(())) => unreadable.push(rel.to_string()),
            None => {}
        }
    }
    match parsed(MCP) {
        Some(Ok(v)) => {
            for (name, s) in v["mcpServers"].as_object().into_iter().flatten() {
                let args: Vec<&str> = s["args"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .collect();
                let cmd = s["command"]
                    .as_str()
                    .or(s["url"].as_str())
                    .unwrap_or("(unknown)");
                mcp.push(
                    format!("{name}: {cmd} {}", args.join(" "))
                        .trim_end()
                        .to_string(),
                );
            }
        }
        Some(Err(())) => {
            unreadable.push(MCP.to_string());
            mcp.push(format!("unreadable {MCP}"));
        }
        None => {}
    }
    let instructions = INSTRUCTIONS
        .iter()
        .filter(|r| matches!(files[Path::new(r)], Content::File(_)))
        .map(|r| r.to_string())
        .collect();
    let any_files = files.values().any(|c| !matches!(c, Content::Absent));
    let mut h = Sha256::new();
    for (rel, c) in &files {
        let p = raw(rel);
        h.update((p.len() as u64).to_le_bytes());
        h.update(&p);
        match c {
            Content::Absent => h.update([0]),
            Content::Other => h.update([3]),
            Content::File(b) | Content::Link(b) => {
                h.update([if matches!(c, Content::File(_)) { 1 } else { 2 }]);
                h.update((b.len() as u64).to_le_bytes());
                h.update(b);
            }
        }
    }
    Inventory {
        hooks,
        instructions,
        mcp,
        notes,
        unreadable,
        ig_plugin: root.join(PLUGIN).is_dir(),
        any_files,
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
    std::fs::create_dir_all(data_dir)?;
    let file = store(data_dir);
    // A trust.json that does not parse is kept aside, not silently erased.
    if let Ok(b) = std::fs::read(&file)
        && serde_json::from_slice::<BTreeMap<String, String>>(&b).is_err()
    {
        let bad = file.with_extension("json.bad");
        eprintln!(
            "ig: {} is unreadable; moved to {}",
            file.display(),
            bad.display()
        );
        std::fs::rename(&file, &bad)?;
    }
    let mut d = decisions(data_dir);
    d.insert(key(root), inv.hash.clone());
    let tmp = file.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(&d)?)?;
    std::fs::rename(tmp, file)
}

/// Trusts `root` if it still hashes to `hash` and `confirm` (the native dialog) agrees.
pub fn trust_if(
    data_dir: &Path,
    root: &Path,
    hash: &str,
    confirm: impl FnOnce(&Inventory) -> bool,
) -> Result<(), String> {
    let inv = inventory(root);
    if inv.hash != hash {
        return Err("the project's agent files changed: review them again".into());
    }
    if !confirm(&inv) {
        return Err("not trusted".into());
    }
    trust(data_dir, root, &inv).map_err(|e| e.to_string())
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

    fn done(root: &Path) {
        let _ = std::fs::remove_dir_all(root.parent().unwrap());
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
        assert_eq!(inv.hooks[0].command, "ig hook claude");
        assert_eq!(inv.hooks[0].when, "after each Claude turn");
        assert!(!is_trusted(&data, &root, &inv));
        trust(&data, &root, &inv).unwrap();
        assert!(is_trusted(&data, &root, &inventory(&root)));
        std::fs::write(
            &s,
            r#"{"hooks":{"Stop":[{"hooks":[{"type":"command","command":"evil"}]}]}}"#,
        )
        .unwrap();
        assert!(!is_trusted(&data, &root, &inventory(&root)));
        done(&root);
    }

    #[test]
    fn adding_a_file_or_breaking_json_changes_the_hash_without_crashing() {
        let (root, _) = scratch("add");
        let h0 = inventory(&root);
        assert!(!h0.any_files);
        std::fs::write(root.join("AGENTS.md"), "x").unwrap();
        let h1 = inventory(&root);
        assert_ne!(h0.hash, h1.hash);
        assert_eq!(h1.instructions, ["AGENTS.md"]);
        std::fs::write(root.join(".mcp.json"), "{oops").unwrap();
        let h2 = inventory(&root);
        assert_ne!(h1.hash, h2.hash);
        assert_eq!(h2.mcp, ["unreadable .mcp.json"]);
        done(&root);
    }

    #[test]
    fn settings_without_hooks_are_still_described() {
        let (root, _) = scratch("notes");
        std::fs::write(root.join(".claude/settings.json"), r#"{"permissions":{"defaultMode":"bypassPermissions","allow":["Bash(rm:*)"]},"apiKeyHelper":"./k.sh","env":{"A":"1"}}"#).unwrap();
        std::fs::write(
            root.join(".mcp.json"),
            r#"{"mcpServers":{"s":{"command":"npx","args":["-y","x"]}}}"#,
        )
        .unwrap();
        let inv = inventory(&root);
        let all = inv.notes.join("\n");
        for want in [
            "bypassPermissions",
            "Bash(rm:*)",
            "runs: ./k.sh",
            "environment variables: A",
        ] {
            assert!(all.contains(want), "{want} in {all}");
        }
        assert_eq!(inv.mcp, ["s: npx -y x"]);
        assert!(inv.any_files);
        done(&root);
    }

    #[cfg(unix)]
    #[test]
    fn a_symlink_loop_in_the_plugin_dir_terminates() {
        let (root, _) = scratch("loop");
        let ig = root.join(".claude/skills/ig");
        std::fs::create_dir_all(&ig).unwrap();
        std::os::unix::fs::symlink(&ig, ig.join("self")).unwrap();
        std::fs::write(ig.join("SKILL.md"), "x").unwrap();
        let a = inventory(&root);
        assert!(a.ig_plugin);
        std::fs::write(ig.join("SKILL.md"), "y").unwrap();
        assert_ne!(a.hash, inventory(&root).hash);
        done(&root);
    }

    #[test]
    fn trust_needs_the_hash_and_the_confirm_and_keeps_a_bad_store() {
        let (root, data) = scratch("if");
        let h = inventory(&root).hash;
        assert!(trust_if(&data, &root, "stale", |_| true).is_err());
        assert!(trust_if(&data, &root, &h, |_| false).is_err());
        std::fs::create_dir_all(&data).unwrap();
        std::fs::write(data.join("trust.json"), "{garbage").unwrap();
        trust_if(&data, &root, &h, |_| true).unwrap();
        assert!(is_trusted(&data, &root, &inventory(&root)));
        assert_eq!(
            std::fs::read(data.join("trust.json.bad")).unwrap(),
            b"{garbage"
        );
        done(&root);
    }
}
