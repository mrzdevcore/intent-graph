//! The self-contained HTML viewer page built from `ig_app::map::View`, and opening it.

use anyhow::Result;
use ig_app::map::View;
use ig_wire::Wired;
use serde_json::Value;
use std::path::{Path, PathBuf};

// The viewer page (viewer/dist/index.html, or a placeholder when it is not built), from build.rs.
const TEMPLATE: &str = include_str!(concat!(env!("OUT_DIR"), "/view.html"));

/// The page for `data`. The viewer's `live` marker stays `false`: nothing serves it any more.
pub fn page(data: &Value) -> String {
    fill(TEMPLATE, &data.to_string().replace('<', "\\u003c"), "false")
}

fn fill(template: &str, data_json: &str, live: &str) -> String {
    template
        .replace("/*IG_LIVE*/", live)
        .replace("/*IG_DATA*/", data_json)
}

/// `ig view`: writes `.ig/view.html` showing `v`.
pub fn write(w: &Wired, v: &View) -> Result<PathBuf> {
    let html = page(&serde_json::to_value(v)?);
    let path = w.root().join(".ig/view.html");
    std::fs::create_dir_all(w.root().join(".ig"))?;
    ig_fs::write_atomic(&path, html.as_bytes())?;
    Ok(path)
}

/// Writes `.ig/.gitignore` (all but the map) when `.ig` exists and neither it nor the repo's
/// `.gitignore` ignores `.ig/*` already, so the page and ig's other files are never committed by
/// accident.
pub fn ignore_ig_dir(root: &Path) -> Result<()> {
    let dir = root.join(".ig");
    let own = dir.join(".gitignore");
    if !dir.is_dir() || own.exists() {
        return Ok(());
    }
    let repo = std::fs::read_to_string(root.join(".gitignore")).unwrap_or_default();
    let ignores = |l: &str| {
        matches!(
            l.trim(),
            ".ig" | ".ig/" | "/.ig" | "/.ig/" | ".ig/*" | "/.ig/*"
        )
    };
    if !repo.lines().any(ignores) {
        std::fs::write(own, "*\n!map.json\n!.gitignore\n")?;
    }
    Ok(())
}

pub fn open(target: &Path) {
    let target = target.to_string_lossy();
    let target = target.as_ref();
    let cmd = if cfg!(target_os = "macos") {
        vec!["open", target]
    } else if cfg!(windows) {
        vec!["cmd", "/C", "start", "", target]
    } else {
        vec!["xdg-open", target]
    };
    let _ = std::process::Command::new(cmd[0])
        .args(&cmd[1..])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn page_fills_the_markers_in_the_viewer_and_the_placeholder() {
        let data = r#"{"nodes":[],"x":"\u003c/script>"}"#;
        let placeholder = include_str!(concat!(env!("OUT_DIR"), "/placeholder.html"));
        for (name, html) in [
            ("view", page(&json!({"nodes": [], "x": "</script>"}))),
            ("placeholder", fill(placeholder, data, "false")),
        ] {
            assert!(html.contains(data), "{name}: data missing");
            assert!(
                html.contains("<title>ig — system map</title>"),
                "{name}: title"
            );
            assert!(!html.contains("/*IG_DATA*/"), "{name}: data marker left");
            assert!(!html.contains("/*IG_LIVE*/"), "{name}: live marker left");
        }
    }
}
