//! `ig init`: scaffolding. Never overwrites anything.

use crate::say;
use anyhow::Result;
use ig_core::config::Config;
use ig_core::workspace::{self, WorkspaceInfo};
use regex::Regex;
use std::collections::BTreeSet;
use std::path::Path;

const AGENTS_MARKER: &str = "<!-- ig:agents -->";

const APPLY_LINE: &str = "- Use /ig:apply ($ig-apply in Codex) to build what `ig plan` lists.\n";
const PLAN_LINE: &str = "- Use /ig:plan ($ig-plan in Codex) to sort the drift `ig plan` lists: take it into ig/ or fix the code.\n";
const LINES: [(&str, &str); 2] = [("/ig:apply", APPLY_LINE), ("/ig:plan", PLAN_LINE)];

const AGENTS_SNIPPET: &str = "<!-- ig:agents -->
## ig (intent-graph)

- Use /ig:explore ($ig-explore in Codex) to answer questions about the code with the ig graph.
- Use /ig:draw ($ig-draw in Codex) to change the intended architecture in ig/.
- Use /ig:plan ($ig-plan in Codex) to sort the drift `ig plan` lists: take it into ig/ or fix the code.
- Use /ig:apply ($ig-apply in Codex) to build what `ig plan` lists.
";

const BEFORE_PLAN: &str = "<!-- ig:agents -->
## ig (intent-graph)

- Use /ig:explore ($ig-explore in Codex) to answer questions about the code with the ig graph.
- Use /ig:draw ($ig-draw in Codex) to change the intended architecture in ig/.
- Use /ig:apply ($ig-apply in Codex) to build what `ig plan` lists.
";

const BEFORE_APPLY: &str = "<!-- ig:agents -->
## ig (intent-graph)

- Use /ig:explore ($ig-explore in Codex) to answer questions about the code with the ig graph.
- Use /ig:draw ($ig-draw in Codex) to change the intended architecture in ig/.
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
pub fn run(root: &Path, agents: &[&crate::skills::Agent], quiet: bool) -> Result<()> {
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
    if let Some((new, what)) = renew_agents_block(&existing) {
        std::fs::write(&agents_md, new)?;
        created.push(format!("AGENTS.md ({what})"));
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
        say(quiet, "ig init: nothing to do");
    }
    if agents.iter().any(|a| a.name == "claude") {
        say(
            quiet,
            "Claude loads .claude/skills/ig only after you trust this folder; run /reload-plugins if /ig:explore is missing.",
        );
    }
    Ok(())
}

/// `text` with an earlier version's `<!-- ig:agents -->` block (one naming /ig:setup, or a
/// snippet ig wrote before) replaced by today's; a block edited since only gets the /ig:apply and
/// /ig:plan lines it lacks after its last bullet. None when there is nothing to do.
fn renew_agents_block(text: &str) -> Option<(String, &'static str)> {
    let start = text.find(AGENTS_MARKER)?;
    let (mut end, mut last) = (start, None);
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
        if l.starts_with("- ") || (bullets && l.starts_with(' ')) {
            last = Some(end);
        }
        bullets |= l.starts_with("- ");
    }
    let (block, rest) = (&text[start..end], &text[end..]);
    let ours = [BEFORE_APPLY, BEFORE_PLAN].iter().any(|o| block.trim_end() == o.trim_end());
    if block.contains("/ig:setup") || ours {
        let sep = if rest.is_empty() { "" } else { "\n" };
        let new = format!("{}{AGENTS_SNIPPET}{sep}{rest}", &text[..start]);
        return Some((new, "snippet updated"));
    }
    let missing: Vec<&str> = (LINES.iter())
        .filter(|(skill, _)| !block.contains(skill))
        .map(|(_, line)| *line)
        .collect();
    if missing.is_empty() {
        return None;
    }
    let at = last?;
    let nl = if text[..at].ends_with('\n') { "" } else { "\n" };
    let new = format!("{}{nl}{}{}", &text[..at], missing.concat(), &text[at..]);
    Some((new, "added the missing ig skill lines to your ig block"))
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
        let renew = |t: &str| renew_agents_block(t).map(|(n, _)| n);
        let old = "# Repo\n\n<!-- ig:agents -->\n## ig (intent-graph)\n\n\
                   - Run `ig check --agent` before finishing.\n\
                   - Use /ig:setup ($ig-setup in Codex) to set up ig.yaml.\n\n## Mine\n\nkeep\n";
        let new = renew(old).unwrap();
        assert_eq!(
            new,
            format!("# Repo\n\n{AGENTS_SNIPPET}\n## Mine\n\nkeep\n")
        );
        assert_eq!(renew(&new), None);
        let at_end = "<!-- ig:agents -->\n## ig (intent-graph)\n\n- Use /ig:setup.\n";
        assert_eq!(renew(at_end).unwrap(), AGENTS_SNIPPET);
        for old in [BEFORE_APPLY, BEFORE_PLAN] {
            assert_eq!(
                renew(&format!("{old}\n# Mine\n")).unwrap(),
                format!("{AGENTS_SNIPPET}\n# Mine\n")
            );
        }
    }

    #[test]
    fn an_edited_block_keeps_its_bullets_and_gets_the_apply_line() {
        let mine = BEFORE_APPLY.replace("ig/.\n", "ig/.\n- Ask Ana before editing ig/.\n");
        let (new, what) = renew_agents_block(&format!("{mine}\n# Mine\n")).unwrap();
        assert_eq!(new, format!("{mine}{APPLY_LINE}{PLAN_LINE}\n# Mine\n"), "{new}");
        assert!(what.contains("added"), "{what}");
        assert_eq!(renew_agents_block(&new), None);
        let no_newline = mine.trim_end();
        assert_eq!(
            renew_agents_block(no_newline).unwrap().0,
            format!("{no_newline}\n{APPLY_LINE}{PLAN_LINE}")
        );
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
            true,
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
        run(&dir, &[crate::skills::agent("claude").unwrap()], true).unwrap();
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
