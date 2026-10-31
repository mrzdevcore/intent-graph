//! `ig init`: scaffolding. Never overwrites anything.

use anyhow::Result;
use ig_core::config::Config;
use ig_core::workspace::{self, WorkspaceInfo};
use regex::Regex;
use std::collections::BTreeSet;
use std::path::Path;

const AGENTS_MARKER: &str = "<!-- ig:agents -->";

const AGENTS_SNIPPET: &str = "<!-- ig:agents -->
## ig (intent-graph)

- Use /ig:explore ($ig-explore in Codex) to answer questions about the code with the ig graph.
";

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
fn detect_embedded(root: &Path, ws: &WorkspaceInfo) -> Vec<String> {
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
            if ig_lang_ts::is_source_ext(&format!("x{ext}"), &[]) || out.contains(&ext) {
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

/// `agents`: the coding agents to set up (the ig skills each).
pub fn run(root: &Path, agents: &[&crate::skills::Agent]) -> Result<()> {
    let mut created = vec![];

    // ig.yaml
    let cfg_path = root.join("ig.yaml");
    if !cfg_path.exists() {
        let files = ig_fs::FsFiles::new(root);
        let discover = |c: &Config| ig_app::build::discover(ig_wire::FRONTENDS, &files, c);
        let first = discover(&Config::default())?;
        let embedded = detect_embedded(&first.root, &first);
        let ws = if embedded.is_empty() {
            first
        } else {
            discover(&Config {
                embedded: embedded.clone(),
                ..Config::default()
            })?
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
             embedded: {embedded}\n\
             # ignore: [\"**/generated/**\"]\n\
             # patterns:\n\
             #   - kind: route\n\
             #     match: \"app.$METHOD($PATH, $$$)\"\n\
             #     where: {{ METHOD: [get, post, put, delete] }}\n\
             #     id: \"{{METHOD}} {{PATH}}\"\n"
        );
        Config::parse(&text)?;
        create(root, &cfg_path, &text, &mut created)?;
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
    if let Some(new) = renew_agents_block(&existing) {
        std::fs::write(&agents_md, new)?;
        created.push("AGENTS.md (snippet updated)".into());
    } else if !existing.contains(AGENTS_MARKER) {
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

    let settings = root.join(".claude/settings.json");
    if let Some(new) = std::fs::read_to_string(&settings)
        .ok()
        .and_then(|t| drop_stop_hook(&t))
    {
        std::fs::write(&settings, new)?;
        created.push(".claude/settings.json (removed ig's old Stop hook)".into());
    }

    let gi = root.join(".gitignore");
    let text = std::fs::read_to_string(&gi).unwrap_or_default();
    let new = gitignore(&text);
    if new != text {
        std::fs::write(&gi, new)?;
        created.push(".gitignore entries".into());
    }

    for c in &created {
        println!("created {c}");
    }
    if created.is_empty() {
        println!("ig init: nothing to do");
    }
    if agents.iter().any(|a| a.name == "claude") {
        println!(
            "Claude loads .claude/skills/ig only after you trust this folder; run /reload-plugins if /ig:explore is missing."
        );
    }
    Ok(())
}

/// `text` with an earlier version's `<!-- ig:agents -->` block (the one naming /ig:setup)
/// replaced by today's; None when there is none.
fn renew_agents_block(text: &str) -> Option<String> {
    let start = text.find(AGENTS_MARKER)?;
    let mut end = start;
    let mut bullets = false;
    for (i, line) in text[start..].split_inclusive('\n').enumerate() {
        let l = line.trim_end();
        let part = i == 0
            || (!bullets && (l.starts_with("## ig") || l.is_empty()))
            || l.starts_with("- ")
            || (bullets && (l.is_empty() || l.starts_with(' ')));
        if !part {
            break;
        }
        end += line.len();
        if bullets && l.is_empty() {
            break; // the blank line after the bullets is the block's
        }
        bullets |= l.starts_with("- ");
    }
    let (block, rest) = (&text[start..end], &text[end..]);
    block.contains("/ig:setup").then(|| {
        let sep = if rest.is_empty() { "" } else { "\n" };
        format!("{}{AGENTS_SNIPPET}{sep}{rest}", &text[..start])
    })
}

/// `.claude/settings.json` without the Stop hook earlier versions installed (`ig hook claude`);
/// None when it has none (or is not JSON).
fn drop_stop_hook(text: &str) -> Option<String> {
    let mut v: serde_json::Value = serde_json::from_str(text).ok()?;
    let ours = |h: &serde_json::Value| {
        h["command"]
            .as_str()
            .is_some_and(|c| c.trim().starts_with("ig hook"))
    };
    let stop = v.get_mut("hooks")?.get_mut("Stop")?.as_array_mut()?;
    let before = stop.clone();
    for e in stop.iter_mut() {
        if let Some(hs) = e.get_mut("hooks").and_then(|h| h.as_array_mut()) {
            hs.retain(|h| !ours(h));
        }
    }
    stop.retain(|e| e["hooks"].as_array().is_none_or(|h| !h.is_empty()));
    if *stop == before {
        return None;
    }
    let hooks = v["hooks"].as_object_mut()?;
    if hooks["Stop"].as_array().is_some_and(|s| s.is_empty()) {
        hooks.remove("Stop");
    }
    if hooks.is_empty() {
        v.as_object_mut()?.remove("hooks");
    }
    Some(serde_json::to_string_pretty(&v).ok()? + "\n")
}

fn gitignore(text: &str) -> String {
    // `ig/` sits outside `.ig/` and is never ignored
    const LINES: &str = ".ig/*";
    if text.lines().any(|x| x.trim() == LINES) {
        return text.to_string();
    }
    // a line ignoring all of `.ig`: replaced in place.
    let mut done = false;
    let mut out: Vec<&str> = vec![];
    for l in text.lines() {
        if !done && matches!(l.trim(), ".ig" | ".ig/" | "/.ig" | "/.ig/" | ".ig/*") {
            out.push(LINES);
            done = true;
        } else {
            out.push(l);
        }
    }
    if !done {
        out.push(LINES);
    }
    format!("{}\n", out.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gitignore_ignores_ig_dir() {
        assert_eq!(gitignore("node_modules"), "node_modules\n.ig/*\n");
        assert_eq!(gitignore(".ig/\n"), ".ig/*\n");
        assert_eq!(gitignore("a\n/.ig\nb\n"), "a\n.ig/*\nb\n");
        // an older repo's map.json line is left alone
        let old = ".ig/*\n!.ig/map.json\n";
        assert_eq!(gitignore(old), old);
        for t in ["node_modules", ".ig/\n", "target\n"] {
            let once = gitignore(t);
            assert_eq!(gitignore(&once), once);
        }
    }

    #[test]
    fn old_agents_block_is_renewed() {
        let old = "# Repo\n\n<!-- ig:agents -->\n## ig (intent-graph)\n\n\
                   - Run `ig check --agent` before finishing.\n\
                   - Use /ig:setup ($ig-setup in Codex) to set up ig.yaml.\n\n## Mine\n\nkeep\n";
        let new = renew_agents_block(old).unwrap();
        assert_eq!(
            new,
            format!("# Repo\n\n{AGENTS_SNIPPET}\n## Mine\n\nkeep\n")
        );
        assert_eq!(renew_agents_block(&new), None);
        let at_end = "<!-- ig:agents -->\n## ig (intent-graph)\n\n- Use /ig:setup.\n";
        assert_eq!(renew_agents_block(at_end).unwrap(), AGENTS_SNIPPET);
    }

    #[test]
    fn old_stop_hook_is_removed() {
        let old = r#"{
  "hooks": {
    "Stop": [
      { "hooks": [{ "type": "command", "command": "ig hook claude", "timeout": 300 }] }
    ]
  },
  "model": "x"
}"#;
        let v: serde_json::Value = serde_json::from_str(&drop_stop_hook(old).unwrap()).unwrap();
        assert_eq!(v, serde_json::json!({"model": "x"}));
        let mixed = r#"{"hooks": {"Stop": [{"hooks": [
            {"type": "command", "command": "ig hook claude"},
            {"type": "command", "command": "make lint"}]}]}}"#;
        let v: serde_json::Value = serde_json::from_str(&drop_stop_hook(mixed).unwrap()).unwrap();
        assert_eq!(v["hooks"]["Stop"][0]["hooks"][0]["command"], "make lint");
        assert_eq!(v["hooks"]["Stop"][0]["hooks"].as_array().unwrap().len(), 1);
        assert_eq!(drop_stop_hook(r#"{"hooks": {"Stop": []}}"#), None);
        assert_eq!(drop_stop_hook("not json"), None);
    }

    #[test]
    fn init_never_overwrites() {
        let dir = std::env::temp_dir().join(format!("ig-init-test-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("src/core")).unwrap();
        std::fs::write(dir.join("src/core/a.ts"), "export const a = () => 1;\n").unwrap();
        run(
            &dir,
            &[
                crate::skills::agent("claude").unwrap(),
                crate::skills::agent("codex").unwrap(),
            ],
        )
        .unwrap();
        assert!(
            dir.join(".claude/skills/ig/skills/explore/SKILL.md")
                .is_file()
        );
        assert!(dir.join(".agents/skills/ig-explore/SKILL.md").is_file());
        let cfg = std::fs::read_to_string(dir.join("ig.yaml")).unwrap();
        assert!(cfg.contains("core: [\"src/core/**\"]"));
        std::fs::write(dir.join("ig.yaml"), "rules: []\n").unwrap();
        std::fs::write(
            dir.join(".claude/skills/ig/skills/explore/SKILL.md"),
            "mine",
        )
        .unwrap();
        run(&dir, &[crate::skills::agent("claude").unwrap()]).unwrap();
        assert_eq!(
            std::fs::read_to_string(dir.join(".claude/skills/ig/skills/explore/SKILL.md")).unwrap(),
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
