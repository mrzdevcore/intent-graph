//! The ig desktop app: the viewer in a window, for a project opened from the Project menu (a
//! native folder picker or a recent project), rebuilt live as its files change.

mod agent;
mod commands;
mod design;
mod events;
mod file_tree;
mod permissions;
mod plugin;
mod read_file;
mod session;
mod shell_path;
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
    let show = root.join(ig_app::show::SHOW);
    let mut bad: Option<String> = None;
    let watched = root.clone();
    let watcher = ig_wire::watch(&root, move |c| {
        // A design file changed on disk, unless the app wrote exactly that itself.
        if !c.designs.is_empty() {
            design::emit_changed(&emitter, &watched, &c.designs);
        }
        // Notes alone change nothing the app shows yet.
        if c.full {
            let _ = emitter.emit("rebuild", ());
        }
        // The agent's `ig_show`, already checked against the graph; a removed file shows nothing,
        // an invalid one is logged (once per content) and never reaches the webview.
        if !c.show {
            return;
        }
        let Ok(text) = std::fs::read_to_string(&show) else {
            return;
        };
        match serde_json::from_str::<ig_app::show::Show>(&text) {
            Ok(s) => {
                let _ = emitter.emit("show", s);
            }
            Err(e) if bad.as_deref() != Some(&text) => {
                eprintln!("ig: {}: not a show, ignored: {e}", show.display());
                bad = Some(text);
            }
            Err(_) => {}
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
    app.state::<design::Known>().0.lock().unwrap().clear();
    // Another project: its agent, if any, stops (open permission requests are rejected).
    let _ = agent::shutdown(app);
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

/// Stops the agent and waits (at most 1 s) until the rejects are out and the adapter is gone.
fn stop_agent(app: &AppHandle) {
    if let Some(task) = agent::shutdown(app) {
        tauri::async_runtime::block_on(session::wait(task, 1000));
    }
}

fn main() {
    shell_path::login_path();
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(Project::default())
        .manage(agent::Agent::default())
        .manage(design::Known::default())
        .invoke_handler(tauri::generate_handler![
            commands::view_data,
            commands::file_tree,
            commands::read_file,
            commands::trust_inventory,
            commands::trust_project,
            design::features,
            design::create_feature,
            design::read_design,
            design::write_design,
            agent::agent_start,
            agent::agent_prompt,
            agent::agent_load,
            agent::agent_restart,
            agent::agent_cancel,
            agent::agent_answer,
            agent::agent_list,
            agent::agent_install,
            agent::agent_reply,
            agent::agent_set_config,
            commands::open_url
        ])
        .on_window_event(|w, e| {
            // The window is gone: its agent stops, rejecting what still waits for an answer.
            if matches!(e, tauri::WindowEvent::Destroyed) {
                stop_agent(w.app_handle());
            }
        })
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
        .build(tauri::generate_context!())
        .expect("ig desktop failed to start")
        .run(|app, e| {
            if let tauri::RunEvent::Exit = e {
                stop_agent(app);
            }
        });
}
