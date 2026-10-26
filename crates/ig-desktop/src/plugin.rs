//! The ig plugin a session loads. The app brings its own (exported by the bundled `ig`, once per
//! version, into `<app data>/plugin/<version>/`), so an upgrade never leaves a project on stale
//! skills. If that fails, the project's own copy is used when it has one.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Where the app's plugin for this version lives.
pub fn app_dir(data: &Path) -> PathBuf {
    data.join("plugin").join(env!("CARGO_PKG_VERSION"))
}

/// Concurrent session starts export once, not twice.
static EXPORTING: Mutex<()> = Mutex::new(());

/// Exports into a sibling directory and renames it into place, so `dir` exists only when whole.
fn ensure(dir: &Path, export: impl FnOnce(&Path) -> Result<(), String>) -> Result<(), String> {
    let _g = EXPORTING.lock().unwrap_or_else(|e| e.into_inner());
    if dir.is_dir() {
        return Ok(());
    }
    let tmp = dir.with_extension(format!("tmp{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    let done = export(&tmp).and_then(|()| std::fs::rename(&tmp, dir).map_err(|e| e.to_string()));
    let _ = std::fs::remove_dir_all(&tmp);
    done.or_else(|e| if dir.is_dir() { Ok(()) } else { Err(e) })
}

/// The plugin path for a session in `root`, and a notice when the app's own could not be made.
pub fn choose(
    root: &Path,
    data: &Path,
    export: impl FnOnce(&Path) -> Result<(), String>,
) -> (Option<PathBuf>, Option<String>) {
    let dir = app_dir(data);
    match ensure(&dir, export) {
        Ok(()) => (Some(dir), None),
        Err(e) => {
            let mine = root.join(".claude/skills/ig");
            let note = format!("Could not set up the app's ig plugin ({e}); using the project's.");
            (mine.is_dir().then_some(mine), Some(note))
        }
    }
}

/// Exports with the bundled `ig` (the directory the app puts first on the agent's PATH).
pub fn export_with(ig_dir: Option<&Path>, to: &Path) -> Result<(), String> {
    let ig = ig_dir.ok_or("no bundled ig")?.join("ig");
    let out = std::process::Command::new(&ig)
        .args(["plugin", "export"])
        .arg(to)
        .output()
        .map_err(|e| format!("{}: {e}", ig.display()))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&out.stderr)
            .trim()
            .chars()
            .take(200)
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dirs(tag: &str) -> (PathBuf, PathBuf) {
        let b = std::env::temp_dir().join(format!("ig-plugin-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&b);
        std::fs::create_dir_all(b.join("root/.claude/skills/ig")).unwrap();
        (b.join("root"), b.join("data"))
    }

    #[test]
    fn the_app_plugin_wins_and_is_exported_once() {
        let (root, data) = dirs("ok");
        let mut calls = 0;
        let mk = |calls: &mut i32, to: &Path| {
            *calls += 1;
            std::fs::create_dir_all(to).unwrap();
            Ok(())
        };
        let (p, n) = choose(&root, &data, |to| mk(&mut calls, to));
        assert_eq!((p, n), (Some(app_dir(&data)), None));
        let (p, _) = choose(&root, &data, |to| mk(&mut calls, to));
        assert_eq!((p, calls), (Some(app_dir(&data)), 1));
    }

    #[test]
    fn a_failed_export_falls_back_to_the_project_plugin_with_a_notice() {
        let (root, data) = dirs("fail");
        let (p, n) = choose(&root, &data, |_| Err("boom".into()));
        assert_eq!(p, Some(root.join(".claude/skills/ig")));
        assert!(n.unwrap().contains("boom"));
        assert!(!app_dir(&data).exists());
        std::fs::remove_dir_all(root.join(".claude")).unwrap();
        assert_eq!(choose(&root, &data, |_| Err("x".into())).0, None);
    }
}
