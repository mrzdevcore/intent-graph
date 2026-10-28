//! The project the app shows, the `view_data` command, and the recent-projects list.

use crate::bounded::Gate;
use ig_app::map::view;
use ig_wire::Wired;
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// The open project: its root and the watcher that emits `rebuild` (dropped on a switch).
#[derive(Default)]
pub struct Project(pub Mutex<Option<(PathBuf, ig_wire::RecommendedWatcher)>>);

/// A project is a git repository or a directory with an `ig.yaml`.
pub fn is_project(root: &Path) -> bool {
    root.join("ig.yaml").is_file() || Wired::new(root).vcs().is_some()
}

/// The viewer's data for `root`, like `ig view --json`.
pub fn view_data_for(root: &Path) -> Result<Value, String> {
    let w = Wired::new(root);
    let v = view::run(&w.env()).map_err(|e| format!("{e:#}"))?;
    serde_json::to_value(v).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn view_data(
    project: tauri::State<'_, Project>,
    gate: tauri::State<'_, Gate>,
) -> Result<Value, String> {
    let root = project.0.lock().unwrap().as_ref().map(|(r, _)| r.clone());
    let Some(root) = root else {
        return Err("no project open: Project > Open… (⌘O)".into());
    };
    gate.run(move || view_data_for(&root)).await?
}

/// What a session in the open project would run, and whether the user already trusted exactly that.
#[derive(serde::Serialize)]
pub struct TrustState {
    #[serde(flatten)]
    inv: crate::trust::Inventory,
    trusted: bool,
}

pub(crate) fn open_root_and_data(
    project: &Project,
    app: &tauri::AppHandle,
) -> Result<(PathBuf, PathBuf), String> {
    use tauri::Manager;
    let root = project.0.lock().unwrap().as_ref().map(|(r, _)| r.clone());
    let Some(root) = root else {
        return Err("no project open: Project > Open… (⌘O)".into());
    };
    Ok((root, app.path().app_data_dir().map_err(|e| e.to_string())?))
}

#[tauri::command]
pub async fn trust_inventory(
    app: tauri::AppHandle,
    project: tauri::State<'_, Project>,
) -> Result<TrustState, String> {
    let (root, data) = open_root_and_data(&project, &app)?;
    tauri::async_runtime::spawn_blocking(move || {
        let inv = crate::trust::inventory(&root);
        TrustState {
            trusted: crate::trust::is_trusted(&data, &root, &inv),
            inv,
        }
    })
    .await
    .map_err(|e| e.to_string())
}

/// Trusts the project only if it still hashes to `hash` (the user trusts exactly what they saw)
/// and a native dialog, which webview script cannot click, is confirmed.
#[tauri::command]
pub async fn trust_project(
    app: tauri::AppHandle,
    project: tauri::State<'_, Project>,
    hash: String,
) -> Result<(), String> {
    use tauri_plugin_dialog::{DialogExt, MessageDialogButtons};
    let (root, data) = open_root_and_data(&project, &app)?;
    tauri::async_runtime::spawn_blocking(move || {
        crate::trust::trust_if(&data, &root, &hash, |_| {
            app.dialog()
                .message(format!(
                    "Trust {}?\n\nClaude Code will run its hooks, MCP servers and instructions as listed.",
                    root.display()
                ))
                .title("ig")
                .buttons(MessageDialogButtons::OkCancelCustom("Trust".into(), "Cancel".into()))
                .blocking_show()
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

const RECENT_MAX: usize = 10;

/// The recent projects, newest first, from `file` (a JSON list of paths); ones that are gone or
/// no longer a project are dropped.
pub fn recent(file: &Path) -> Vec<PathBuf> {
    let list: Vec<PathBuf> = std::fs::read(file)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default();
    list.into_iter().filter(|p| is_project(p)).collect()
}

/// Puts `root` first in the recent list in `file`.
pub fn remember(file: &Path, root: &Path) -> std::io::Result<Vec<PathBuf>> {
    let mut list = recent(file);
    list.retain(|p| p != root);
    list.insert(0, root.to_path_buf());
    list.truncate(RECENT_MAX);
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(file, serde_json::to_vec_pretty(&list)?)?;
    Ok(list)
}

/// An `http(s)` link only (the chat's links are agent text): never `file:`, `javascript:`, ...
pub fn web_url(raw: &str) -> Option<String> {
    let u = url::Url::parse(raw).ok()?;
    matches!(u.scheme(), "http" | "https").then(|| u.to_string())
}

/// Opens a web link in the system browser; the webview never navigates away.
#[tauri::command]
pub fn open_url(url: String) -> Result<(), String> {
    let u = web_url(&url).ok_or(format!("not a web link: {url}"))?;
    let (cmd, pre): (&str, &[&str]) = if cfg!(target_os = "macos") {
        ("open", &[])
    } else if cfg!(windows) {
        ("rundll32", &["url.dll,FileProtocolHandler"])
    } else {
        ("xdg-open", &[])
    };
    std::process::Command::new(cmd)
        .args(pre)
        .arg(u)
        .spawn()
        .map(drop)
        .map_err(|e| format!("{cmd}: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};

    /// A scratch copy of the monorepo fixture.
    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ig-desktop-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../ig/tests/fixtures/monorepo");
        let ok = std::process::Command::new("cp")
            .arg("-R")
            .arg(fixture)
            .arg(&dir)
            .status()
            .unwrap()
            .success();
        assert!(ok);
        dir.canonicalize().unwrap()
    }

    #[test]
    fn view_data_for_the_monorepo_has_boxes() {
        let dir = scratch("view");
        let v = view_data_for(&dir).unwrap();
        assert!(v["boxes"].as_array().is_some_and(|n| !n.is_empty()), "{v}");
    }

    #[test]
    fn only_web_links_open() {
        assert_eq!(
            web_url("https://x.org/a b").as_deref(),
            Some("https://x.org/a%20b")
        );
        for bad in [
            "javascript:alert(1)",
            "file:///etc/passwd",
            "x",
            "-a https://x.org",
        ] {
            assert_eq!(web_url(bad), None, "{bad}");
        }
    }

    #[test]
    fn a_project_has_an_ig_yaml_or_git() {
        let dir = scratch("project");
        assert!(is_project(&dir));
        std::fs::remove_file(dir.join("ig.yaml")).unwrap();
        assert!(!is_project(&dir));
    }

    #[test]
    fn remember_puts_the_project_first_once_and_recent_drops_stale_ones() {
        let (a, b) = (scratch("recent-a"), scratch("recent-b"));
        let file = a.join(".ig/data/recent.json");
        remember(&file, &a).unwrap();
        remember(&file, &b).unwrap();
        let list = remember(&file, &a).unwrap();
        assert_eq!(list, [a.clone(), b.clone()]);
        assert_eq!(recent(&file), list);
        std::fs::remove_dir_all(&b).unwrap();
        assert_eq!(recent(&file), [a]);
    }
}
