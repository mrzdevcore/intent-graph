//! The ig skills and how they are installed for each coding agent.
//! Claude gets a project plugin (`/ig:<name>`), Codex gets `ig-<name>` skills (`$ig-<name>`).

use anyhow::{Result, bail};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub struct Skill {
    pub name: &'static str,
    pub body: &'static str,
}

pub const SKILLS: &[Skill] = &[
    Skill {
        name: "explore",
        body: include_str!("../assets/skills/explore.md"),
    },
    Skill {
        name: "view",
        body: include_str!("../assets/skills/view.md"),
    },
];

const MCP_JSON: &str =
    "{\n  \"mcpServers\": {\n    \"ig\": { \"command\": \"ig\", \"args\": [\"mcp\"] }\n  }\n}\n";

pub struct Agent {
    pub name: &'static str,
}

pub const AGENTS: &[Agent] = &[Agent { name: "claude" }, Agent { name: "codex" }];

pub fn agent(name: &str) -> Result<&'static Agent> {
    match AGENTS.iter().find(|a| a.name == name) {
        Some(a) => Ok(a),
        None => bail!(
            "unknown agent {name:?}; expected one of: {}",
            AGENTS.iter().map(|a| a.name).collect::<Vec<_>>().join(", ")
        ),
    }
}

fn write_missing(path: PathBuf, content: &str, written: &mut Vec<PathBuf>) -> Result<()> {
    if path.exists() {
        return Ok(());
    }
    std::fs::create_dir_all(path.parent().unwrap())?;
    std::fs::write(&path, content)?;
    written.push(path);
    Ok(())
}

/// A skill as Codex sees it: named `ig-<name>`.
fn codex_body(s: &Skill) -> String {
    s.body.replacen(
        &format!("name: {}", s.name),
        &format!("name: ig-{}", s.name),
        1,
    )
}

/// The Claude plugin as (path under `.claude/skills/ig`, content): what `install` and `export` write.
fn claude_files() -> Result<Vec<(String, String)>> {
    let manifest = serde_json::json!({
        "name": "ig",
        "version": env!("CARGO_PKG_VERSION"),
        "description": "intent-graph: set up ig.yaml, explore and show the system",
        "skills": ["./skills"],
    });
    let mut v = vec![
        (
            ".claude-plugin/plugin.json".to_string(),
            serde_json::to_string_pretty(&manifest)? + "\n",
        ),
        // The ig MCP server (tools mcp__plugin_ig_ig__*).
        (".mcp.json".into(), MCP_JSON.into()),
    ];
    for s in SKILLS {
        v.push((format!("skills/{}/SKILL.md", s.name), s.body.into()));
    }
    Ok(v)
}

/// Writes `content` to `path` through a temp file, so a reader never sees half of it.
fn write_atomic(path: &Path, content: &str) -> Result<()> {
    std::fs::create_dir_all(path.parent().unwrap())?;
    let tmp = path.with_extension("ig-tmp");
    std::fs::write(&tmp, content)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

/// `ig plugin export <dir>`: the current Claude plugin in `dir`, overwriting.
pub fn export(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut out = vec![];
    for (rel, content) in claude_files()? {
        let p = dir.join(rel);
        write_atomic(&p, &content)?;
        out.push(p);
    }
    Ok(out)
}

const GENERATED: &str = ".ig-generated.json";

/// FNV-1a 64: a stable, non-cryptographic hash (the ig crate has no sha2). It only tells
/// "untouched since ig wrote it" from "edited", so collisions do not matter.
fn fingerprint(text: &str) -> String {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in text.bytes() {
        h = (h ^ u64::from(b)).wrapping_mul(0x100000001b3);
    }
    format!("{h:016x}")
}

/// An old install without a manifest: does the file still look like one ig wrote? Markdown must
/// open with that file's own frontmatter name; JSON must have a shipped shape.
fn looks_generated(rel: &str, text: &str) -> bool {
    let json = || serde_json::from_str::<serde_json::Value>(text).ok();
    match rel {
        ".mcp.json" => json() == serde_json::from_str(MCP_JSON).ok(),
        ".claude-plugin/plugin.json" => json()
            .is_some_and(|v| v["name"] == "ig" && v["skills"] == serde_json::json!(["./skills"])),
        _ => rel
            .split('/')
            .nth(1)
            .is_some_and(|n| text.starts_with(&format!("---\nname: {n}\n"))),
    }
}

/// `X.bak`, or the next free `X.bak.N`: an earlier backup is never overwritten.
fn free_bak(p: &Path) -> PathBuf {
    let first = PathBuf::from(format!("{}.bak", p.display()));
    if !first.exists() {
        return first;
    }
    (1..)
        .map(|n| PathBuf::from(format!("{}.bak.{n}", p.display())))
        .find(|c| !c.exists())
        .unwrap()
}

/// Brings the Claude plugin in `base` up to date: missing files are written; a file untouched
/// since ig wrote it (its hash is in `.ig-generated.json`, or, with no manifest at all, it still
/// looks generated and is backed up to `.bak`) is overwritten; an edited one is kept and the new
/// version goes next to it as `.new`. Returns the files written.
fn refresh_claude(base: &Path) -> Result<Vec<PathBuf>> {
    let mp = base.join(GENERATED);
    let old: Option<BTreeMap<String, String>> = std::fs::read(&mp)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok());
    let mut rec = old.clone().unwrap_or_default();
    let mut written = vec![];
    for (rel, content) in claude_files()? {
        let p = base.join(&rel);
        let want = fingerprint(&content);
        match std::fs::read_to_string(&p) {
            Ok(cur) if cur == content => {}
            Ok(cur) => {
                let untouched = rec.get(&rel).is_some_and(|h| *h == fingerprint(&cur));
                if untouched {
                    write_atomic(&p, &content)?;
                } else if old.is_none() && looks_generated(&rel, &cur) {
                    let bak = free_bak(&p);
                    std::fs::copy(&p, &bak)?;
                    write_atomic(&p, &content)?;
                    eprintln!(
                        "ig: replaced {}; your copy is {}",
                        p.display(),
                        bak.display()
                    );
                } else {
                    let new = PathBuf::from(format!("{}.new", p.display()));
                    if std::fs::read_to_string(&new).ok().as_deref() != Some(&content) {
                        write_atomic(&new, &content)?;
                        eprintln!(
                            "ig: kept your edited {}; the new version is at {}",
                            p.display(),
                            new.display()
                        );
                    }
                    continue;
                }
                written.push(p);
            }
            Err(_) => {
                write_atomic(&p, &content)?;
                written.push(p);
            }
        }
        rec.insert(rel, want);
    }
    if old.as_ref() != Some(&rec) {
        write_atomic(&mp, &(serde_json::to_string_pretty(&rec)? + "\n"))?;
    }
    Ok(written)
}

/// Writes the files that are missing and returns them. Claude's plugin is also refreshed (see
/// `refresh_claude`); Codex's skills are only written when missing.
pub fn install(root: &Path, agent: &Agent) -> Result<Vec<PathBuf>> {
    if agent.name == "claude" {
        return refresh_claude(&root.join(".claude/skills/ig"));
    }
    let mut written = vec![];
    let base = root.join(".agents/skills");
    for s in SKILLS {
        write_missing(
            base.join(format!("ig-{}", s.name)).join("SKILL.md"),
            &codex_body(s),
            &mut written,
        )?;
    }
    Ok(written)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn installs_claude_plugin_and_codex_skills() {
        let dir = temp_dir("ig-skills");
        install(&dir, agent("claude").unwrap()).unwrap();
        install(&dir, agent("codex").unwrap()).unwrap();
        let manifest: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(dir.join(".claude/skills/ig/.claude-plugin/plugin.json"))
                .unwrap(),
        )
        .unwrap();
        assert_eq!(manifest["name"], "ig");
        assert_eq!(manifest["skills"], serde_json::json!(["./skills"]));
        assert!(dir.join(".claude/skills/ig/skills/view/SKILL.md").is_file());
        let mcp: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(dir.join(".claude/skills/ig/.mcp.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(
            mcp,
            serde_json::json!({"mcpServers": {"ig": {"command": "ig", "args": ["mcp"]}}})
        );
        assert!(
            std::fs::read_to_string(dir.join(".agents/skills/ig-view/SKILL.md"))
                .unwrap()
                .contains("name: ig-view")
        );
        let explore =
            std::fs::read_to_string(dir.join(".claude/skills/ig/skills/explore/SKILL.md")).unwrap();
        assert!(explore.contains("`ig_show`") && explore.contains("`.ig/show.json`"));
    }

    fn files_under(d: &Path) -> Vec<String> {
        let mut v = vec![];
        let mut stack = vec![d.to_path_buf()];
        while let Some(x) = stack.pop() {
            for e in std::fs::read_dir(&x).unwrap() {
                let p = e.unwrap().path();
                if p.is_dir() {
                    stack.push(p);
                } else {
                    v.push(p.strip_prefix(d).unwrap().to_string_lossy().into_owned());
                }
            }
        }
        v.sort();
        v
    }

    #[test]
    fn export_writes_what_install_writes() {
        let dir = temp_dir("ig-export");
        install(&dir, agent("claude").unwrap()).unwrap();
        let out = dir.join("out");
        export(&out).unwrap();
        export(&out).unwrap(); // overwriting is fine
        let mut installed = files_under(&dir.join(".claude/skills/ig"));
        installed.retain(|f| f != GENERATED);
        assert_eq!(files_under(&out), installed);
    }

    #[test]
    fn refresh_untouched_edited_and_legacy() {
        let dir = temp_dir("ig-refresh");
        let claude = agent("claude").unwrap();
        let base = dir.join(".claude/skills/ig");
        let shown = base.join("skills/view/SKILL.md");
        let explore = base.join("skills/explore/SKILL.md");
        install(&dir, claude).unwrap();
        assert!(base.join(GENERATED).is_file());
        // Untouched since ig wrote it (an older version): updated.
        let old = "---\nname: view\n---\nolder body\n";
        std::fs::write(&shown, old).unwrap();
        let mut rec: BTreeMap<String, String> =
            serde_json::from_slice(&std::fs::read(base.join(GENERATED)).unwrap()).unwrap();
        rec.insert("skills/view/SKILL.md".into(), fingerprint(old));
        std::fs::write(base.join(GENERATED), serde_json::to_vec(&rec).unwrap()).unwrap();
        // Edited: kept, new version beside it.
        std::fs::write(&explore, "my edit").unwrap();
        install(&dir, claude).unwrap();
        assert_eq!(
            std::fs::read_to_string(&shown).unwrap(),
            SKILLS.iter().find(|s| s.name == "view").unwrap().body
        );
        assert_eq!(std::fs::read_to_string(&explore).unwrap(), "my edit");
        assert!(explore.with_file_name("SKILL.md.new").is_file());
        // Legacy: no manifest, still generated: updated with a .bak; a non-generated one is kept.
        std::fs::remove_file(base.join(GENERATED)).unwrap();
        std::fs::write(&shown, old).unwrap();
        install(&dir, claude).unwrap();
        assert_eq!(
            std::fs::read_to_string(&shown).unwrap(),
            SKILLS.iter().find(|s| s.name == "view").unwrap().body
        );
        assert_eq!(
            std::fs::read_to_string(shown.with_file_name("SKILL.md.bak")).unwrap(),
            old
        );
        assert_eq!(std::fs::read_to_string(&explore).unwrap(), "my edit");
        assert!(explore.with_file_name("SKILL.md.new").is_file());
        // A second legacy refresh keeps the first backup and takes the next free name.
        std::fs::remove_file(base.join(GENERATED)).unwrap();
        std::fs::write(&shown, old).unwrap();
        install(&dir, claude).unwrap();
        assert!(shown.with_file_name("SKILL.md.bak.1").is_file());
        assert!(!looks_generated(
            "skills/view/SKILL.md",
            "---\nname: other\n"
        ));
        assert!(!looks_generated(".mcp.json", "{\"x\": \"ig\"}"));
    }
}
