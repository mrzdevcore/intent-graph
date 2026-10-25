//! `ig init`: scaffolding. Never overwrites anything.

use crate::check;
use crate::config::Config;
use crate::workspace::{self, Workspace};
use anyhow::Result;
use regex::Regex;
use std::collections::BTreeSet;
use std::path::Path;

const AGENTS_MARKER: &str = "<!-- ig:agents -->";

const AGENTS_SNIPPET: &str = "<!-- ig:agents -->
## ig (intent-graph)

- Run `ig check --agent` before finishing; fix every blocking finding.
- Tag code that implements an intent from `intents.md` with `// @intent ID-1`, and its tests too.
- Add new intents to `intents.md` as `- ID-1: one line`.
- Use /ig:setup ($ig-setup in Codex) to set up ig.yaml; /ig:plan and /ig:apply for features.
";

const HOOK: &str = r#"{
  "hooks": {
    "Stop": [
      { "hooks": [{ "type": "command", "command": "ig hook claude", "timeout": 300 }] }
    ]
  }
}
"#;

fn layer_name(s: &str) -> String {
    let base = s.rsplit('/').next().unwrap_or(s);
    let n: String = base
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect();
    if n.is_empty() { "root".into() } else { n }
}

fn yaml_str(s: &str) -> String {
    serde_json::to_string(s).unwrap()
}

/// Extensions that relative imports point at, are not JS/TS, and look like single-file components.
fn detect_embedded(root: &Path, ws: &Workspace) -> Vec<String> {
    let re = Regex::new(
        r#"(?:from\s+|import\s*\(\s*|import\s+)["'](\.{1,2}/[^"']+?(\.[A-Za-z0-9]+))["']"#,
    )
    .unwrap();
    let mut out = BTreeSet::new();
    for f in &ws.files {
        let Ok(src) = std::fs::read_to_string(root.join(&f.path)) else {
            continue;
        };
        let dir = f.path.rsplit_once('/').map(|(d, _)| d).unwrap_or(".");
        for c in re.captures_iter(&src) {
            let ext = c[2].to_ascii_lowercase();
            if workspace::is_source_ext(&format!("x{ext}"), &[]) || out.contains(&ext) {
                continue;
            }
            let target = root.join(workspace::normalize(&workspace::join(dir, &c[1])));
            if std::fs::read_to_string(&target).is_ok_and(|t| t.contains("<script")) {
                out.insert(ext);
            }
        }
    }
    out.into_iter().collect()
}

fn create(root: &Path, path: &Path, content: &str, created: &mut Vec<String>) -> Result<bool> {
    if path.exists() {
        return Ok(false);
    }
    if let Some(p) = path.parent() {
        std::fs::create_dir_all(p)?;
    }
    std::fs::write(path, content)?;
    created.push(
        path.strip_prefix(root)
            .unwrap_or(path)
            .to_string_lossy()
            .to_string(),
    );
    Ok(true)
}

/// `agents`: the coding agents to set up (the ig skills each, and the Stop hook for Claude).
pub fn run(root: &Path, agents: &[&crate::skills::Agent]) -> Result<()> {
    let mut created = vec![];

    // ig.yaml
    let cfg_path = root.join("ig.yaml");
    if !cfg_path.exists() {
        let first = Workspace::discover(root, &Config::default())?;
        let embedded = detect_embedded(&first.root, &first);
        let ws = if embedded.is_empty() {
            first
        } else {
            Workspace::discover(
                root,
                &Config {
                    embedded: embedded.clone(),
                    ..Config::default()
                },
            )?
        };
        let mut layers = String::new();
        if ws.packages.len() > 1 || ws.packages[0].dir != "." {
            for p in &ws.packages {
                layers.push_str(&format!(
                    "  {}: [{}]\n",
                    layer_name(&p.name),
                    yaml_str(&p.name)
                ));
            }
        } else {
            let mut dirs = BTreeSet::new();
            for f in &ws.files {
                if let Some(rest) = f.path.strip_prefix("src/")
                    && let Some((d, _)) = rest.split_once('/')
                {
                    dirs.insert(d.to_string());
                }
            }
            for d in dirs {
                layers.push_str(&format!(
                    "  {}: [{}]\n",
                    layer_name(&d),
                    yaml_str(&format!("src/{d}/**"))
                ));
            }
        }
        let layers = if layers.is_empty() {
            "layers: {}\n".to_string()
        } else {
            format!("layers:\n{layers}")
        };
        let embedded = format!(
            "[{}]",
            embedded
                .iter()
                .map(|e| yaml_str(e))
                .collect::<Vec<_>>()
                .join(", ")
        );
        let text = format!(
            "# ig (intent-graph) config. See docs for every key.\n\
             {layers}\
             # Forbidden layer dependencies, e.g.:\n\
             #   - forbid: \"core -> ui\"\n\
             rules: []\n\
             complexity:\n  max: 15\n  maxIncrease: 5\n\
             embedded: {embedded}\n\
             # ignore: [\"**/generated/**\"]\n\
             # patterns:\n\
             #   - kind: route\n\
             #     match: \"app.$METHOD($PATH, $$$)\"\n\
             #     where: {{ METHOD: [get, post, put, delete] }}\n\
             #     id: \"{{METHOD}} {{PATH}}\"\n\
             # checkers:\n\
             #   - name: eslint\n\
             #     run: \"npx eslint -f @microsoft/eslint-formatter-sarif .\"\n\
             # blockOn: error\n"
        );
        Config::parse(&text)?;
        create(root, &cfg_path, &text, &mut created)?;
    }

    create(
        root,
        &root.join("intents.md"),
        "# Intents\n\nOne intent per line, referenced from code with `// @intent ID-1`.\n\n<!-- - APP-1: Users can sign in with email -->\n",
        &mut created,
    )?;

    if agents.iter().any(|a| a.name == "claude") {
        let settings = root.join(".claude/settings.json");
        if !create(root, &settings, HOOK, &mut created)? {
            println!("ig: .claude/settings.json exists; add this Stop hook to it:\n{HOOK}");
        }
    }
    for a in agents {
        for p in crate::skills::install(root, a)? {
            created.push(
                p.strip_prefix(root)
                    .unwrap_or(&p)
                    .to_string_lossy()
                    .to_string(),
            );
        }
    }

    let agents_md = root.join("AGENTS.md");
    let existing = std::fs::read_to_string(&agents_md).unwrap_or_default();
    if !existing.contains(AGENTS_MARKER) {
        let sep = if existing.is_empty() || existing.ends_with("\n\n") {
            ""
        } else if existing.ends_with('\n') {
            "\n"
        } else {
            "\n\n"
        };
        std::fs::write(&agents_md, format!("{existing}{sep}{AGENTS_SNIPPET}"))?;
        created.push("AGENTS.md (snippet)".into());
    }

    create(
        root,
        &root.join("rules.md"),
        "# Rules\n\nRules no tool can check. One per heading, with a stable id.\n\n<!-- ## R-EXAMPLE: Components stay presentational; logic lives in composables -->\n",
        &mut created,
    )?;

    let gi = root.join(".gitignore");
    let text = std::fs::read_to_string(&gi).unwrap_or_default();
    let new = gitignore(&text);
    if new != text {
        std::fs::write(&gi, new)?;
        created.push(".gitignore entries".into());
    }

    if !root.join(".ig/baseline.json").exists() {
        let run = check::run(
            root,
            &check::Opts {
                all: true,
                ..Default::default()
            },
        )?;
        let fps = run
            .result
            .findings
            .iter()
            .filter(|f| f.rule_id != "ig/checker-error" && f.rule_id != "ig/parse-error")
            .map(|f| f.fingerprint.clone());
        crate::checks::write_baseline(root, fps)?;
        created.push(format!(
            ".ig/baseline.json ({} findings)",
            run.result.findings.len()
        ));
    }
    for c in &created {
        println!("created {c}");
    }
    if created.is_empty() {
        println!("ig init: nothing to do");
    }
    let next: Vec<String> = agents
        .iter()
        .map(|a| {
            format!(
                "`ig setup {}` (or {} inside {})",
                a.name,
                crate::skills::invoke(a, "setup"),
                a.name
            )
        })
        .collect();
    println!(
        "\nNext: let your coding agent populate ig.yaml: {}.",
        next.join(", ")
    );
    if agents.iter().any(|a| a.name == "claude") {
        println!(
            "Claude loads .claude/skills/ig only after you trust this folder; run /reload-plugins if /ig:setup is missing."
        );
    }
    Ok(())
}

fn gitignore(text: &str) -> String {
    let mut lines: Vec<String> = vec![];
    let mut done = false;
    for l in text.lines() {
        let t = l.trim();
        if t == ".ig" || t == ".ig/" || t == "/.ig" || t == "/.ig/" {
            if !done {
                lines.push(".ig/*".into());
                lines.push("!.ig/baseline.json".into());
                done = true;
            }
            continue;
        }
        lines.push(l.to_string());
    }
    for entry in [".ig/*", "!.ig/baseline.json"] {
        if !lines.iter().any(|l| l.trim() == entry) {
            lines.push(entry.into());
        }
    }
    lines.join("\n") + "\n"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gitignore_rewrites_and_is_idempotent() {
        assert_eq!(
            gitignore("node_modules\n.ig/\n"),
            "node_modules\n.ig/*\n!.ig/baseline.json\n"
        );
        let once = gitignore("target\n");
        assert_eq!(gitignore(&once), once);
    }

    #[test]
    fn init_never_overwrites() {
        let dir = std::env::temp_dir().join(format!("ig-init-test-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("src/core")).unwrap();
        std::fs::write(dir.join("src/core/a.ts"), "export const a = () => 1;\n").unwrap();
        std::fs::write(dir.join("intents.md"), "mine\n").unwrap();
        run(
            &dir,
            &[
                crate::skills::agent("claude").unwrap(),
                crate::skills::agent("codex").unwrap(),
            ],
        )
        .unwrap();
        assert!(
            dir.join(".claude/skills/ig/skills/setup/SKILL.md")
                .is_file()
        );
        assert!(dir.join(".agents/skills/ig-setup/SKILL.md").is_file());
        assert_eq!(
            std::fs::read_to_string(dir.join("intents.md")).unwrap(),
            "mine\n"
        );
        let cfg = std::fs::read_to_string(dir.join("ig.yaml")).unwrap();
        assert!(cfg.contains("core: [\"src/core/**\"]"));
        std::fs::write(dir.join("ig.yaml"), "rules: []\n").unwrap();
        std::fs::write(dir.join(".claude/skills/ig/skills/setup/SKILL.md"), "mine").unwrap();
        run(&dir, &[crate::skills::agent("claude").unwrap()]).unwrap();
        assert_eq!(
            std::fs::read_to_string(dir.join(".claude/skills/ig/skills/setup/SKILL.md")).unwrap(),
            "mine"
        );
        assert_eq!(
            std::fs::read_to_string(dir.join("ig.yaml")).unwrap(),
            "rules: []\n"
        );
        let agents = std::fs::read_to_string(dir.join("AGENTS.md")).unwrap();
        assert_eq!(agents.matches(AGENTS_MARKER).count(), 1);
        let _ = std::fs::remove_dir_all(dir);
    }
}
