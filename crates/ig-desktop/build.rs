//! Fills `dist/`, the app's frontend, from the viewer: `viewer/dist-app/` (`make viewer-app`:
//! multi-file, with the board engine's lazy chunk and .wasm), else `viewer/dist/index.html`
//! (`make viewer`, no boards), else a page that says so, so Rust-only work still builds. Then
//! Tauri's build, with `view_data`, `file_tree`, `read_file`, the trust commands, the design commands (with `plan_status`), the screen and asset commands, the agent commands, `open_url` and `exit_flushed` as the only commands the webview may call (granted in `capabilities/`).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

const PLACEHOLDER: &str = "<!doctype html><meta charset=\"utf-8\"><title>ig</title>\
<p>viewer not built: run <code>make viewer</code></p>";

/// Relative path -> bytes of every file under `dir`.
fn files(dir: &Path, rel: &Path, out: &mut BTreeMap<PathBuf, Vec<u8>>) {
    for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let (p, r) = (e.path(), rel.join(e.file_name()));
        if p.is_dir() {
            files(&p, &r, out);
        } else if let Ok(b) = std::fs::read(&p) {
            out.insert(r, b);
        }
    }
}

fn main() {
    let viewer = concat!(env!("CARGO_MANIFEST_DIR"), "/../../viewer");
    let (app, page) = (
        format!("{viewer}/dist-app"),
        format!("{viewer}/dist/index.html"),
    );
    println!("cargo:rerun-if-changed={app}");
    println!("cargo:rerun-if-changed={page}");
    let mut want = BTreeMap::new();
    files(Path::new(&app), Path::new(""), &mut want);
    if want.is_empty() {
        let html = std::fs::read(&page).unwrap_or_else(|_| {
            println!("cargo:warning=viewer/dist-app and viewer/dist/index.html missing; run make viewer-app");
            PLACEHOLDER.into()
        });
        want.insert("index.html".into(), html);
    }
    let out = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/dist"));
    let mut have = BTreeMap::new();
    files(out, Path::new(""), &mut have);
    // Only what changed: a rewrite would make tauri-build rerun this script for ever.
    for (rel, bytes) in &want {
        if have.get(rel) != Some(bytes) {
            let to = out.join(rel);
            std::fs::create_dir_all(to.parent().unwrap()).unwrap();
            std::fs::write(to, bytes).unwrap();
        }
    }
    for rel in have.keys().filter(|r| !want.contains_key(*r)) {
        std::fs::remove_file(out.join(rel)).unwrap();
    }
    let manifest = tauri_build::AppManifest::new().commands(&[
        "view_data",
        "trust_inventory",
        "trust_project",
        "agent_start",
        "agent_prompt",
        "agent_load",
        "agent_restart",
        "agent_cancel",
        "agent_answer",
        "agent_list",
        "agent_install",
        "agent_reply",
        "agent_set_config",
        "open_url",
        "exit_flushed",
    ]);
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(manifest)).unwrap();
}
