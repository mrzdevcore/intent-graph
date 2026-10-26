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
        name: "setup",
        body: include_str!("../assets/skills/setup.md"),
    },
    Skill {
        name: "explore",
        body: include_str!("../assets/skills/explore.md"),
    },
    Skill {
        name: "plan",
        body: include_str!("../assets/skills/plan.md"),
    },
    Skill {
        name: "apply",
        body: include_str!("../assets/skills/apply.md"),
    },
    Skill {
        name: "view",
        body: include_str!("../assets/skills/view.md"),
    },
];

const MCP_JSON: &str =
    "{\n  \"mcpServers\": {\n    \"ig\": { \"command\": \"ig\", \"args\": [\"mcp\"] }\n  }\n}\n";

pub const REVIEWER: &str = include_str!("../assets/agents/reviewer.md");

const LEGACY_EXPLORE: &str = include_str!("../assets/legacy/ig-explore-v1.md");

pub struct Agent {
    pub name: &'static str,
    pub bin: &'static str,
}

pub const AGENTS: &[Agent] = &[
    Agent {
        name: "claude",
        bin: "claude",
    },
    Agent {
        name: "codex",
        bin: "codex",
    },
];

pub fn agent(name: &str) -> Result<&'static Agent> {
    match AGENTS.iter().find(|a| a.name == name) {
        Some(a) => Ok(a),
        None => bail!(
            "unknown agent {name:?}; expected one of: {}",
            AGENTS.iter().map(|a| a.name).collect::<Vec<_>>().join(", ")
        ),
    }
}

pub fn invoke(agent: &Agent, skill: &str) -> String {
    if agent.name == "claude" {
        format!("/ig:{skill}")
    } else {
        format!("$ig-{skill}")
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

/// The v1 `ig-explore` skill (the setup skill under its old name): removed when untouched, kept
/// with a notice when edited. For Codex its path is now the new `$ig-explore`, which counts as
/// current. Runs before the skills are written.
fn migrate_legacy(root: &Path, agent: &Agent) -> Result<Option<String>> {
    let base = if agent.name == "claude" {
        ".claude"
    } else {
        ".agents"
    };
    let old = root.join(base).join("skills/ig-explore/SKILL.md");
    let Ok(text) = std::fs::read_to_string(&old) else {
        return Ok(None);
    };
    let current = SKILLS
        .iter()
        .find(|s| s.name == "explore")
        .is_some_and(|s| agent.name != "claude" && text == codex_body(s));
    if text == LEGACY_EXPLORE {
        std::fs::remove_file(&old)?;
        let _ = std::fs::remove_dir(old.parent().unwrap());
        Ok(None)
    } else if current {
        Ok(None)
    } else {
        Ok(Some(format!(
            "ig: kept {} (edited); the setup skill is now ig-setup",
            old.display()
        )))
    }
}

/// The Claude plugin as (path under `.claude/skills/ig`, content): what `install` and `export` write.
fn claude_files() -> Result<Vec<(String, String)>> {
    let manifest = serde_json::json!({
        "name": "ig",
        "version": env!("CARGO_PKG_VERSION"),
        "description": "intent-graph: set up, plan and apply changes against ig.yaml",
        "skills": ["./skills"],
    });
    let mut v = vec![
        (
            ".claude-plugin/plugin.json".to_string(),
            serde_json::to_string_pretty(&manifest)? + "\n",
        ),
        ("agents/reviewer.md".into(), REVIEWER.into()),
        // The ig MCP server (tools mcp__plugin_ig_ig__*, and the viewer's notes channel).
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

/// An old install without a manifest: does the file still look like one ig wrote?
fn looks_generated(rel: &str, text: &str) -> bool {
    if rel.ends_with(".md") {
        text.starts_with("---\nname:")
    } else {
        text.contains("\"ig\"")
    }
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
                    let bak = PathBuf::from(format!("{}.bak", p.display()));
                    std::fs::copy(&p, &bak)?;
                    write_atomic(&p, &content)?;
                } else {
                    let new = PathBuf::from(format!("{}.new", p.display()));
                    write_atomic(&new, &content)?;
                    eprintln!(
                        "ig: kept your edited {}; the new version is at {}",
                        p.display(),
                        new.display()
                    );
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
/// `refresh_claude`); Codex's skills are only written when missing. An untouched ig-explore
/// skill from an earlier version is removed, an edited one is left with a notice.
pub fn install(root: &Path, agent: &Agent) -> Result<Vec<PathBuf>> {
    if let Some(notice) = migrate_legacy(root, agent)? {
        eprintln!("{notice}");
    }
    if agent.name == "claude" {
        return refresh_claude(&root.join(".claude/skills/ig"));
    }
    let mut written = vec![];
    let base = root.join(".agents/skills");
    write_missing(base.join("ig-apply/reviewer.md"), REVIEWER, &mut written)?;
    for s in SKILLS {
        write_missing(
            base.join(format!("ig-{}", s.name)).join("SKILL.md"),
            &codex_body(s),
            &mut written,
        )?;
    }
    Ok(written)
}

/// `ig setup <agent>`: installs the skill if needed, then hands the terminal to the agent with
/// the skill invoked.
pub fn run_setup(root: &Path, name: &str) -> Result<u8> {
    let a = agent(name)?;
    for p in install(root, a)? {
        eprintln!("ig: installed {}", p.display());
    }
    let invoke = invoke(a, "setup");
    let status = std::process::Command::new(a.bin)
        .arg(&invoke)
        .current_dir(root)
        .status();
    match status {
        Ok(s) => Ok(s.code().unwrap_or(1).clamp(0, 255) as u8),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            bail!(
                "{} is not on PATH; install it, or open your agent in {} and run {}",
                a.bin,
                root.display(),
                invoke
            )
        }
        Err(e) => Err(e.into()),
    }
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
        assert!(
            dir.join(".claude/skills/ig/skills/setup/SKILL.md")
                .is_file()
        );
        assert!(dir.join(".claude/skills/ig/agents/reviewer.md").is_file());
        let mcp: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(dir.join(".claude/skills/ig/.mcp.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(
            mcp,
            serde_json::json!({"mcpServers": {"ig": {"command": "ig", "args": ["mcp"]}}})
        );
        assert!(dir.join(".agents/skills/ig-apply/SKILL.md").is_file());
        assert!(dir.join(".agents/skills/ig-apply/reviewer.md").is_file());
        assert!(dir.join(".agents/skills/ig-setup/SKILL.md").is_file());
        assert!(
            std::fs::read_to_string(dir.join(".agents/skills/ig-setup/SKILL.md"))
                .unwrap()
                .contains("name: ig-setup")
        );
        let explore =
            std::fs::read_to_string(dir.join(".claude/skills/ig/skills/explore/SKILL.md")).unwrap();
        assert!(explore.contains("`ig_show`") && explore.contains("`.ig/show.json`"));
        assert_eq!(invoke(agent("codex").unwrap(), "plan"), "$ig-plan");
        assert_eq!(invoke(agent("claude").unwrap(), "plan"), "/ig:plan");
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
        let plan = base.join("skills/plan/SKILL.md");
        let view = base.join("skills/view/SKILL.md");
        let apply = base.join("skills/apply/SKILL.md");
        install(&dir, claude).unwrap();
        assert!(base.join(GENERATED).is_file());
        // Untouched since ig wrote it (an older version): updated.
        let old = "---\nname: plan\n---\nolder body\n";
        std::fs::write(&plan, old).unwrap();
        let mut rec: BTreeMap<String, String> =
            serde_json::from_slice(&std::fs::read(base.join(GENERATED)).unwrap()).unwrap();
        rec.insert("skills/plan/SKILL.md".into(), fingerprint(old));
        std::fs::write(base.join(GENERATED), serde_json::to_vec(&rec).unwrap()).unwrap();
        // Edited: kept, new version beside it.
        std::fs::write(&view, "my edit").unwrap();
        install(&dir, claude).unwrap();
        assert_eq!(
            std::fs::read_to_string(&plan).unwrap(),
            SKILLS.iter().find(|s| s.name == "plan").unwrap().body
        );
        assert_eq!(std::fs::read_to_string(&view).unwrap(), "my edit");
        assert!(view.with_file_name("SKILL.md.new").is_file());
        // Legacy: no manifest, still generated: updated with a .bak; a non-generated one is kept.
        std::fs::remove_file(base.join(GENERATED)).unwrap();
        std::fs::write(&plan, old).unwrap();
        std::fs::write(&apply, "my own apply").unwrap();
        install(&dir, claude).unwrap();
        assert_eq!(
            std::fs::read_to_string(&plan).unwrap(),
            SKILLS.iter().find(|s| s.name == "plan").unwrap().body
        );
        assert_eq!(
            std::fs::read_to_string(plan.with_file_name("SKILL.md.bak")).unwrap(),
            old
        );
        assert_eq!(std::fs::read_to_string(&apply).unwrap(), "my own apply");
        assert!(apply.with_file_name("SKILL.md.new").is_file());
    }

    #[test]
    fn explore_migration() {
        let codex = agent("codex").unwrap();
        let new = codex_body(SKILLS.iter().find(|s| s.name == "explore").unwrap());
        // Fresh install: the new $ig-explore, and no notice on the next run.
        let dir = temp_dir("ig-legacy-fresh");
        let path = dir.join(".agents/skills/ig-explore/SKILL.md");
        install(&dir, codex).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), new);
        assert!(migrate_legacy(&dir, codex).unwrap().is_none());
        // v1 upgrade: the untouched v1 file is replaced by the new body.
        let dir = temp_dir("ig-legacy-v1");
        let path = dir.join(".agents/skills/ig-explore/SKILL.md");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, LEGACY_EXPLORE).unwrap();
        install(&dir, codex).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), new);
        // Edited: kept, with a notice.
        std::fs::write(&path, "edited").unwrap();
        assert!(migrate_legacy(&dir, codex).unwrap().is_some());
        install(&dir, codex).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "edited");
        // Claude: the v1 file is removed, an edited one kept with a notice.
        let claude = agent("claude").unwrap();
        let old = dir.join(".claude/skills/ig-explore/SKILL.md");
        std::fs::create_dir_all(old.parent().unwrap()).unwrap();
        std::fs::write(&old, LEGACY_EXPLORE).unwrap();
        install(&dir, claude).unwrap();
        assert!(!old.exists() && !old.parent().unwrap().exists());
        assert!(
            dir.join(".claude/skills/ig/skills/explore/SKILL.md")
                .is_file()
        );
        std::fs::create_dir_all(old.parent().unwrap()).unwrap();
        std::fs::write(&old, "edited").unwrap();
        assert!(migrate_legacy(&dir, claude).unwrap().is_some());
        install(&dir, claude).unwrap();
        assert_eq!(std::fs::read_to_string(&old).unwrap(), "edited");
    }
}
