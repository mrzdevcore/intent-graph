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

/// Writes the files that are missing and returns them. Existing files are kept; an untouched
/// ig-explore skill from an earlier version is removed, an edited one is left with a notice.
pub fn install(root: &Path, agent: &Agent) -> Result<Vec<PathBuf>> {
    let mut written = vec![];
    let (base, legacy) = if agent.name == "claude" {
        (
            root.join(".claude/skills/ig"),
            ".claude/skills/ig-explore/SKILL.md",
        )
    } else {
        (
            root.join(".agents/skills"),
            ".agents/skills/ig-explore/SKILL.md",
        )
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
            let body = s.body.replacen(
                &format!("name: {}", s.name),
                &format!("name: ig-{}", s.name),
                1,
            );
            write_missing(
                base.join(format!("ig-{}", s.name)).join("SKILL.md"),
                &body,
                &mut written,
            )?;
        }
    }
    let old = root.join(legacy);
    if let Ok(text) = std::fs::read_to_string(&old) {
        if text == LEGACY_EXPLORE {
            std::fs::remove_file(&old)?;
            let _ = std::fs::remove_dir(old.parent().unwrap());
        } else {
            eprintln!(
                "ig: kept {} (edited); the setup skill is now ig-setup",
                old.display()
            );
        }
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
        assert!(
            dir.join(".claude/skills/ig/skills/setup/SKILL.md")
                .is_file()
        );
        assert!(dir.join(".claude/skills/ig/agents/reviewer.md").is_file());
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
    fn replaces_only_untouched_legacy_skill() {
        let dir = temp_dir("ig-legacy");
        let old = dir.join(".agents/skills/ig-explore/SKILL.md");
        std::fs::create_dir_all(old.parent().unwrap()).unwrap();
        std::fs::write(&old, include_str!("../assets/legacy/ig-explore-v1.md")).unwrap();
        install(&dir, agent("codex").unwrap()).unwrap();
        assert!(!old.exists());
        std::fs::create_dir_all(old.parent().unwrap()).unwrap();
        std::fs::write(&old, "edited").unwrap();
        install(&dir, agent("codex").unwrap()).unwrap();
        assert_eq!(std::fs::read_to_string(&old).unwrap(), "edited");
    }
}
