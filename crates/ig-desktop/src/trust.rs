//! The per-project trust gate: what a Claude Code session in a project would run (hooks, agent
//! instructions, MCP servers, the ig plugin), a hash of it, and the user's stored decision.

use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

const SETTINGS: [&str; 2] = [".claude/settings.json", ".claude/settings.local.json"];
const MCP: &str = ".mcp.json";
const INSTRUCTIONS: [&str; 4] = [
    "AGENTS.md",
    "CLAUDE.md",
    "CLAUDE.local.md",
    ".claude/CLAUDE.md",
];
const PLUGIN: &str = ".claude/skills/ig";
/// Everything under here is hashed: skills, commands, agents, output styles, ...
const CLAUDE_DIR: &str = ".claude";
/// A bigger file is not read: its path and size are hashed and it is listed as too large to inspect.
const MAX_FILE: u64 = 1 << 20;
/// The walk of `.claude/` stops here (ponytail: the rest goes unhashed; a note says so).
const MAX_FILES: usize = 5_000;

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
    /// AGENTS.md / CLAUDE.md files that exist, relative, then "skill: x" / "command: y" / "agent: z".
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
    /// A fixed top-level file that is a symlink to a regular file: its target path and the target's bytes.
    Followed(Vec<u8>, Vec<u8>),
    /// A regular file over `MAX_FILE`: its size only.
    TooBig(u64),
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
        Ok(m) if m.is_file() && m.len() > MAX_FILE => Content::TooBig(m.len()),
        Ok(m) if m.is_file() => std::fs::read(p).map_or(Content::Absent, Content::File),
        Ok(_) => Content::Other,
    }
}

/// A fixed top-level file: a symlink is followed for content; a dangling or non-regular target stays a `Link`.
fn read_fixed(p: &Path) -> Content {
    match (std::fs::symlink_metadata(p), std::fs::metadata(p)) {
        (Ok(l), Ok(m)) if l.is_symlink() && m.is_file() => {
            let target = std::fs::read_link(p).map(|t| raw(&t)).unwrap_or_default();
            std::fs::read(p).map_or(Content::Link(target.clone()), |b| {
                Content::Followed(target, b)
            })
        }
        _ => read_one(p),
    }
}

/// Every file under `dir`, sorted, at most `MAX_FILES`; symlinked directories are listed as links,
/// not entered. Returns false when it stopped at the cap.
fn files_under(dir: &Path, out: &mut Vec<PathBuf>) -> bool {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return true;
    };
    let mut kids: Vec<_> = rd.flatten().collect();
    kids.sort_by_key(|e| e.file_name());
    for e in kids {
        if out.len() >= MAX_FILES {
            return false;
        }
        let p = e.path();
        match e.file_type() {
            Ok(t) if t.is_dir() => {
                if !files_under(&p, out) {
                    return false;
                }
            }
            _ => out.push(p),
        }
    }
    true
}

/// "skill: x", "command: y", "agent: z" for a path relative to the root, if it is one.
fn extension_name(rel: &Path) -> Option<String> {
    let parts: Vec<&str> = rel.iter().filter_map(|c| c.to_str()).collect();
    let stem = |p: &[&str]| {
        let j = p.join("/");
        j.strip_suffix(".md").map(str::to_string)
    };
    match parts.as_slice() {
        [".claude", "skills", name, "SKILL.md"] if *name != "ig" => Some(format!("skill: {name}")),
        [".claude", "commands", rest @ ..] => stem(rest).map(|n| format!("command: {n}")),
        [".claude", "agents", rest @ ..] => stem(rest).map(|n| format!("agent: {n}")),
        _ => None,
    }
}

/// Plain-words effects named in a markdown file's frontmatter: pre-approved tools, hooks.
fn frontmatter_notes(label: &str, text: &str) -> Vec<String> {
    let mut lines = text.lines();
    if lines.next().map(str::trim) != Some("---") {
        return vec![];
    }
    let (mut out, mut tools, mut in_tools) = (vec![], vec![], false);
    for l in lines.by_ref().take_while(|l| l.trim() != "---") {
        let item = l.trim_start().strip_prefix("- ");
        if in_tools && let Some(i) = item.filter(|_| l.starts_with([' ', '-'])) {
            tools.push(i.trim().to_string());
            continue;
        }
        in_tools = false;
        if let Some(v) = l.strip_prefix("allowed-tools:") {
            in_tools = v.trim().is_empty();
            if !in_tools {
                tools.push(v.trim().trim_matches(['[', ']']).to_string());
            }
        } else if l.starts_with("hooks:") {
            out.push(format!("{label} runs hooks"));
        }
    }
    if !tools.is_empty() {
        out.insert(0, format!("{label} pre-approves: {}", tools.join(", ")));
    }
    out
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
    let mut walked = vec![];
    let complete = files_under(&root.join(CLAUDE_DIR), &mut walked);
    if !complete {
        notes.push(format!(
            "more than {MAX_FILES} files under {CLAUDE_DIR}: the rest are not inspected"
        ));
    }
    // Each file is read once, path-sorted; an absent file hashes as absent, so adding one changes the hash.
    let mut files: BTreeMap<PathBuf, Content> = SETTINGS
        .iter()
        .chain(&[MCP])
        .chain(&INSTRUCTIONS)
        .map(|r| (PathBuf::from(r), read_fixed(&root.join(r))))
        .collect();
    for p in walked {
        if let Ok(rel) = p.strip_prefix(root) {
            files
                .entry(rel.to_path_buf())
                .or_insert_with(|| read_one(&p));
        }
    }
    let mut named = vec![];
    for (rel, c) in &files {
        let Some(name) = extension_name(rel) else {
            continue;
        };
        match c {
            Content::File(b) => notes.extend(frontmatter_notes(&name, &String::from_utf8_lossy(b))),
            Content::TooBig(_) => notes.push(format!("{name} is too large to inspect")),
            _ => {}
        }
        named.push(name);
    }
    for (rel, c) in &files {
        if let Content::TooBig(_) = c
            && extension_name(rel).is_none()
        {
            notes.push(format!("{} is too large to inspect", rel.display()));
        }
    }
    let parsed = |rel: &str| match &files[Path::new(rel)] {
        Content::File(b) | Content::Followed(_, b) => {
            Some(serde_json::from_slice::<Value>(b).map_err(|_| ()))
        }
        Content::Link(_) | Content::Other | Content::TooBig(_) => Some(Err(())),
        Content::Absent => None,
    };
    // Fixed files that are symlinks: say where they point, or that they lead nowhere.
    for rel in SETTINGS.iter().chain(&[MCP]).chain(&INSTRUCTIONS) {
        match &files[Path::new(rel)] {
            Content::Followed(t, _) => {
                notes.push(format!("{rel} (symlink to {})", String::from_utf8_lossy(t)))
            }
            Content::Link(t) => {
                notes.push(format!(
                    "{rel} (symlink to {}) cannot be read",
                    String::from_utf8_lossy(t)
                ));
                if INSTRUCTIONS.contains(rel) {
                    unreadable.push(rel.to_string());
                }
            }
            _ => {}
        }
    }
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
    let mut instructions: Vec<String> = INSTRUCTIONS
        .iter()
        .filter(|r| {
            matches!(
                files[Path::new(r)],
                Content::File(_) | Content::Followed(..)
            )
        })
        .map(|r| r.to_string())
        .collect();
    instructions.extend(named);
    let any_files = files.values().any(|c| !matches!(c, Content::Absent));
    let mut h = Sha256::new();
    for (rel, c) in &files {
        let p = raw(rel);
        h.update((p.len() as u64).to_le_bytes());
        h.update(&p);
        match c {
            Content::Absent => h.update([0]),
            Content::Other => h.update([3]),
            Content::TooBig(n) => {
                h.update([5]);
                h.update(n.to_le_bytes());
            }
            Content::Followed(t, b) => {
                h.update([4]);
                h.update((t.len() as u64).to_le_bytes());
                h.update(t);
                h.update((b.len() as u64).to_le_bytes());
                h.update(b);
            }
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
#[path = "trust_tests.rs"]
mod tests;
