//! Copies the viewer (`viewer/dist/index.html`, built by `make viewer`) into `dist/`, the app's
//! frontend; without it, a page that says so, so Rust-only work still builds. Then Tauri's build,
//! with `view_data`, `file_tree`, `read_file`, the trust commands, the agent commands and `open_url` as the only commands the webview may call (granted in `capabilities/`).

const PLACEHOLDER: &str = "<!doctype html><meta charset=\"utf-8\"><title>ig</title>\
<p>viewer not built: run <code>make viewer</code></p>";

fn main() {
    let dist = concat!(env!("CARGO_MANIFEST_DIR"), "/../../viewer/dist/index.html");
    println!("cargo:rerun-if-changed={dist}");
    let page = std::fs::read_to_string(dist).unwrap_or_else(|_| {
        println!("cargo:warning=viewer/dist/index.html missing; run make viewer");
        PLACEHOLDER.to_string()
    });
    let out = concat!(env!("CARGO_MANIFEST_DIR"), "/dist");
    std::fs::create_dir_all(out).unwrap();
    // Only when changed: a rewrite would make tauri-build rerun this script for ever.
    let index = format!("{out}/index.html");
    if std::fs::read_to_string(&index).ok().as_deref() != Some(&page) {
        std::fs::write(&index, page).unwrap();
    }
    let manifest = tauri_build::AppManifest::new().commands(&[
        "view_data",
        "file_tree",
        "read_file",
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
    ]);
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(manifest)).unwrap();
}
