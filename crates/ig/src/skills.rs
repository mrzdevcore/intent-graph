//! The ig skills and how they are installed for each coding agent.
//! Claude gets a project plugin (`/ig:<name>`), Codex gets `ig-<name>` skills (`$ig-<name>`).

use anyhow::{Result, bail};
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

/// Writes the files that are missing and returns them. Existing files are kept; an untouched
/// ig-explore skill from an earlier version is removed, an edited one is left with a notice.
pub fn install(root: &Path, agent: &Agent) -> Result<Vec<PathBuf>> {
    let mut written = vec![];
    if let Some(notice) = migrate_legacy(root, agent)? {
        eprintln!("{notice}");
    }
    let base = if agent.name == "claude" {
        root.join(".claude/skills/ig")
    } else {
        root.join(".agents/skills")
    };
    if agent.name == "claude" {
        let manifest = serde_json::json!({
            "name": "ig",
            "version": env!("CARGO_PKG_VERSION"),
            "description": "intent-graph: set up, plan and apply changes against ig.yaml",
            "skills": ["./skills"],
        });
        write_missing(
            base.join(".claude-plugin/plugin.json"),
            &(serde_json::to_string_pretty(&manifest)? + "\n"),
            &mut written,
        )?;
    }
    if agent.name == "claude" {
        write_missing(base.join("agents/reviewer.md"), REVIEWER, &mut written)?;
        // The ig MCP server (tools mcp__plugin_ig_ig__*, and the viewer's notes channel).
        write_missing(base.join(".mcp.json"), MCP_JSON, &mut written)?;
    }
    if agent.name != "claude" {
        write_missing(base.join("ig-apply/reviewer.md"), REVIEWER, &mut written)?;
    }
    for s in SKILLS {
        if agent.name == "claude" {
            write_missing(
                base.join("skills").join(s.name).join("SKILL.md"),
                s.body,
                &mut written,
            )?;
        } else {
            write_missing(
                base.join(format!("ig-{}", s.name)).join("SKILL.md"),
                &codex_body(s),
                &mut written,
            )?;
        }
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
        assert_eq!(invoke(agent("codex").unwrap(), "plan"), "$ig-plan");
        assert_eq!(invoke(agent("claude").unwrap(), "plan"), "/ig:plan");
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
