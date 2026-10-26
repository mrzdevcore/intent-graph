//! The self-contained HTML viewer page built from `ig_app::view_data`, and opening it.

use anyhow::Result;
use ig_app::Env;
use ig_app::check::{Opts, Run};
use ig_app::view_data::{self, FOCUS, Picked};
use ig_wire::{Wired, shown_plan};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

// The viewer page (viewer/dist/index.html, or a placeholder when it is not built), from build.rs.
const TEMPLATE: &str = include_str!(concat!(env!("OUT_DIR"), "/view.html"));

pub fn render(env: &Env, run: &Run, opts: &Opts) -> String {
    page(&view_data::data(env, run, opts), None)
}

/// The page for `data`. Served by `ig serve`, it gets the API token (live); a file gets none.
pub fn page(data: &Value, token: Option<&str>) -> String {
    let live = token.map_or("false".into(), |t| json!({ "token": t }).to_string());
    fill(TEMPLATE, &data.to_string().replace('<', "\\u003c"), &live)
}

fn fill(template: &str, data_json: &str, live: &str) -> String {
    template
        .replace("/*IG_LIVE*/", live)
        .replace("/*IG_DATA*/", data_json)
}

/// `ig view`: writes `.ig/view.html` for the check with `plan` overlaid.
pub fn write(w: &Wired, opts: &Opts, plan: &Picked) -> Result<PathBuf> {
    let env = w.env();
    let run = view_data::run_with_plan(&env, opts, shown_plan(w.root(), plan))?;
    let html = render(&env, &run, opts);
    let path = w.root().join(".ig/view.html");
    ig_fs::write_atomic(&path, html.as_bytes())?;
    let _ = std::fs::remove_file(w.root().join(FOCUS)); // a focus is shown once
    Ok(path)
}

pub fn write_run(env: &Env, run: &Run, opts: &Opts) -> Result<PathBuf> {
    let path = env.root.join(".ig/view.html");
    ig_fs::write_atomic(&path, render(env, run, opts).as_bytes())?;
    Ok(path)
}

pub fn open(target: &Path) {
    open_str(&target.to_string_lossy());
}

pub fn open_str(target: &str) {
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

    #[test]
    fn page_fills_the_markers_in_the_viewer_and_the_placeholder() {
        let data = r#"{"nodes":[],"x":"\u003c/script>"}"#;
        let placeholder = include_str!(concat!(env!("OUT_DIR"), "/placeholder.html"));
        let live = page(&json!({"nodes": [], "x": "</script>"}), Some("tok"));
        assert!(live.contains(r#"{"token":"tok"}"#));
        for (name, html) in [
            ("view", live),
            ("placeholder", fill(placeholder, data, "false")),
        ] {
            assert!(html.contains(data), "{name}: data missing");
            assert!(!html.contains("/*IG_DATA*/"), "{name}: data marker left");
            assert!(!html.contains("/*IG_LIVE*/"), "{name}: live marker left");
        }
    }
}
