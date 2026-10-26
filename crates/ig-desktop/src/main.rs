//! The ig desktop app: the viewer in a window, for a project opened from the Project menu (a
//! native folder picker or a recent project), rebuilt live as its files change.

mod agent;
mod commands;
mod file_tree;
mod read_file;
mod trust;

use commands::Project;
use std::path::{Path, PathBuf};
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::{AppHandle, Emitter, Manager, Wry};
use tauri_plugin_dialog::{DialogExt, MessageDialogKind};

fn recent_file(app: &AppHandle) -> Option<PathBuf> {
    app.path()
        .app_data_dir()
        .ok()
        .map(|d| d.join("recent.json"))
}

/// The default menu plus Project: Open… and Open Recent.
fn menu(app: &AppHandle, recent: &[PathBuf]) -> tauri::Result<Menu<Wry>> {
    let m = Menu::default(app)?;
    let items = recent
        .iter()
        .map(|p| {
            let id = format!("recent:{}", p.display());
            MenuItem::with_id(app, id, p.display().to_string(), true, None::<&str>)
        })
        .collect::<tauri::Result<Vec<_>>>()?;
    let items: Vec<&dyn tauri::menu::IsMenuItem<Wry>> = items.iter().map(|i| i as _).collect();
    let open = MenuItem::with_id(app, "open", "Open…", true, Some("CmdOrCtrl+O"))?;
    let sep = PredefinedMenuItem::separator(app)?;
    let recent = Submenu::with_items(app, "Open Recent", !items.is_empty(), &items)?;
    let project = Submenu::with_items(app, "Project", true, &[&open, &sep, &recent])?;
    // After the app menu on macOS (its first submenu), first elsewhere.
    m.insert(&project, if cfg!(target_os = "macos") { 1 } else { 0 })?;
    Ok(m)
}

/// Opens `root`: refuses a non-project, else watches it, remembers it and shows it.
fn open(app: &AppHandle, root: &Path) {
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    if !commands::is_project(&root) {
        app.dialog()
            .message(format!(
                "{}\n\nnot an ig project: run `ig init`",
                root.display()
            ))
            .kind(MessageDialogKind::Error)
            .title("ig")
            .show(|_| {});
        return;
    }
    let emitter = app.clone();
    let watcher = ig_wire::watch(&root, move |full| {
        // Notes alone change nothing the app shows yet.
        if full {
            let _ = emitter.emit("rebuild", ());
        }
    });
    let watcher = match watcher {
        Ok(w) => w,
        Err(e) => {
            app.dialog()
                .message(format!("{}: cannot watch: {e}", root.display()))
                .kind(MessageDialogKind::Error)
                .show(|_| {});
            return;
        }
    };
    *app.state::<Project>().0.lock().unwrap() = Some((root.clone(), watcher));
    if let Some(file) = recent_file(app) {
        match commands::remember(&file, &root) {
            Ok(list) => {
                if let Ok(m) = menu(app, &list) {
                    let _ = app.set_menu(m);
                }
            }
            Err(e) => eprintln!("ig: {}: {e}", file.display()),
        }
    }
    if let Some(w) = app.get_webview_window("main") {
        let name = root
            .file_name()
            .unwrap_or(root.as_os_str())
            .to_string_lossy();
        let _ = w.set_title(&format!("ig — {name}"));
        // A fresh page for the new project (and one that failed for want of a project).
        let _ = w.reload();
    }
}

fn pick(app: &AppHandle) {
    let app = app.clone();
    app.dialog()
        .file()
        .set_title("Open an ig project")
        .pick_folder(move |dir| {
            if let Some(dir) = dir.and_then(|d| d.into_path().ok()) {
                open(&app, &dir);
            }
        });
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(Project::default())
        .invoke_handler(tauri::generate_handler![
            commands::view_data,
            commands::file_tree,
            commands::read_file,
            commands::trust_inventory,
            commands::trust_project
        ])
        .menu(|app| {
            let recent = recent_file(app).map(|f| commands::recent(&f));
            menu(app, &recent.unwrap_or_default())
        })
        .on_menu_event(|app, e| match e.id().as_ref() {
            "open" => pick(app),
            id => {
                if let Some(p) = id.strip_prefix("recent:") {
                    open(app, Path::new(p));
                }
            }
        })
        .setup(|app| {
            let app = app.handle();
            // `ig-desktop <dir>`, else the last project, else the picker.
            let last = recent_file(app).and_then(|f| commands::recent(&f).into_iter().next());
            match std::env::args_os().nth(1).map(PathBuf::from).or(last) {
                Some(dir) if dir.is_dir() => open(app, &dir),
                _ => pick(app),
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("ig desktop failed to start");
}
