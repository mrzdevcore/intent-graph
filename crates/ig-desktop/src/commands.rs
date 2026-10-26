//! The project the app shows, the `view_data` command, and the recent-projects list.

use crate::file_tree::{Entry, file_tree_for};
use crate::read_file::{FileText, read_file_for};
use ig_app::check::Opts;
use ig_app::view_data;
use ig_core::workspace;
use ig_wire::{Wired, shown_plan};
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

/// The viewer's data for `root`, like `ig view --json`: checkers skipped, the plan picked as
/// `ig view` does. `plan` is relative to the root and must stay inside it.
pub fn view_data_for(root: &Path, plan: Option<&str>, no_plan: bool) -> Result<Value, String> {
    let plan = match plan {
        Some(p) => {
            let abs = root
                .join(p)
                .canonicalize()
                .map_err(|e| format!("plan {p}: {e}"))?;
            if workspace::rel(root, &abs).is_none() {
                return Err(format!("plan {p}: outside the project"));
            }
            Some(abs)
        }
        None => None,
    };
    let w = Wired::new(root);
    let env = w.env();
    let picked = view_data::pick_plan(&env, plan.as_deref(), no_plan);
    let opts = Opts {
        skip_checkers: true,
        ..Opts::default()
    };
    view_data::json(&env, &opts, shown_plan(root, &picked)).map_err(|e| format!("{e:#}"))
}

#[tauri::command]
pub async fn view_data(
    project: tauri::State<'_, Project>,
    plan: Option<String>,
    no_plan: bool,
) -> Result<Value, String> {
    let root = project.0.lock().unwrap().as_ref().map(|(r, _)| r.clone());
    let Some(root) = root else {
        return Err("no project open: Project > Open… (⌘O)".into());
    };
    tauri::async_runtime::spawn_blocking(move || view_data_for(&root, plan.as_deref(), no_plan))
        .await
        .map_err(|e| e.to_string())?
}

/// The children of `dir` (None: the project root); see `file_tree::file_tree_for`.
#[tauri::command]
pub async fn file_tree(
    project: tauri::State<'_, Project>,
    dir: Option<String>,
) -> Result<Vec<Entry>, String> {
    let root = project.0.lock().unwrap().as_ref().map(|(r, _)| r.clone());
    let Some(root) = root else {
        return Err("no project open: Project > Open… (⌘O)".into());
    };
    tauri::async_runtime::spawn_blocking(move || file_tree_for(&root, dir.as_deref()))
        .await
        .map_err(|e| e.to_string())
}

/// One text file of the project; see `read_file::read_file_for`.
#[tauri::command]
pub async fn read_file(
    project: tauri::State<'_, Project>,
    path: String,
) -> Result<FileText, String> {
    let root = project.0.lock().unwrap().as_ref().map(|(r, _)| r.clone());
    let Some(root) = root else {
        return Err("no project open: Project > Open… (⌘O)".into());
    };
    tauri::async_runtime::spawn_blocking(move || read_file_for(&root, &path))
        .await
        .map_err(|e| e.to_string())?
}

/// What a session in the open project would run, and whether the user already trusted exactly that.
#[derive(serde::Serialize)]
pub struct TrustState {
    #[serde(flatten)]
    inv: crate::trust::Inventory,
    trusted: bool,
}

fn open_root_and_data(
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
    fn view_data_for_the_monorepo_has_nodes() {
        let dir = scratch("view");
        let v = view_data_for(&dir, None, false).unwrap();
        assert!(v["nodes"].as_array().is_some_and(|n| !n.is_empty()), "{v}");
        assert!(view_data_for(&dir, Some(".."), false).is_err());
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
