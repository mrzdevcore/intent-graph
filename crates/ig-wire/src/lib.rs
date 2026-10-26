//! The composition root shared by the driving adapters (`ig` and `ig-desktop`): the real adapters
//! plugged into `ig_app::Env`, the plan helpers, and the debounced watcher behind live rebuilds.

use anyhow::{Context, Result};
use ig_app::Env;
use ig_app::view_data::{Picked, ShownPlan};
use ig_checkers::ProcessRunner;
use ig_core::ports::{LanguageFrontend, Vcs};
use ig_fs::FsFiles;
use ig_git::GitVcs;
use ig_lang_ts::TsFrontend;
use std::collections::BTreeSet;
use std::path::Path;
use std::sync::mpsc;
use std::time::Duration;

pub use notify::RecommendedWatcher;

static FRONTENDS: &[&dyn LanguageFrontend] = &[&TsFrontend];

/// A progress line on stderr, only when it is a terminal (hooks and JSON consumers stay clean).
fn progress(msg: &str) {
    use std::io::IsTerminal;
    if std::io::stderr().is_terminal() {
        eprintln!("ig: {msg}");
    }
}

/// A check's notices: always on stderr.
fn notice(msg: &str) {
    eprintln!("ig: {msg}");
}

/// The adapters for an ig root: its files and its git repository (if any).
pub struct Wired {
    files: FsFiles,
    vcs: Option<GitVcs>,
}

impl Wired {
    pub fn new(root: &Path) -> Wired {
        Wired {
            files: FsFiles::new(root),
            vcs: GitVcs::open(root),
        }
    }

    pub fn root(&self) -> &Path {
        &self.files.root
    }

    pub fn files(&self) -> &FsFiles {
        &self.files
    }

    pub fn vcs(&self) -> Option<&GitVcs> {
        self.vcs.as_ref()
    }

    pub fn env(&self) -> Env<'_> {
        Env {
            root: &self.files.root,
            files: &self.files,
            vcs: self.vcs.as_ref().map(|v| v as &dyn Vcs),
            runner: &ProcessRunner,
            frontends: FRONTENDS,
            progress: &progress,
            notice: &notice,
        }
    }
}

/// The plan at `path` (relative to the working directory): its root-relative path and text.
pub fn read_plan(root: &Path, path: &Path) -> Result<(String, String)> {
    let text = std::fs::read_to_string(path).with_context(|| format!("{}", path.display()))?;
    let rel = path
        .canonicalize()
        .ok()
        .and_then(|p| ig_core::workspace::rel(root, &p))
        .unwrap_or_else(|| path.to_string_lossy().replace('\\', "/"));
    Ok((rel, text))
}

/// The picked plan, read.
pub fn shown_plan(root: &Path, (plan, applying): &Picked) -> Option<ShownPlan> {
    plan.as_ref().map(|p| ShownPlan {
        path: p.clone(),
        applying: *applying,
        read: read_plan(root, p),
    })
}

/// Paths a live view never rebuilds for: ig's own output, git internals, dependencies.
pub fn ignored(root: &Path, p: &Path) -> bool {
    let rel = p.strip_prefix(root).unwrap_or(p);
    rel.components().any(|c| {
        matches!(
            c.as_os_str().to_str(),
            Some(".ig" | ".git" | "node_modules")
        )
    })
}

/// What changed in one quiet period: `full` when anything but plan notes (`*.notes.*`) and the
/// files of a feature folder (all but its `plan.md`) did, `show` when `.ig/show.json` did (an
/// agent's `ig_show`), `designs` the slugs of the feature folders touched.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Changed {
    pub full: bool,
    pub show: bool,
    pub designs: BTreeSet<String>,
}

/// The feature whose design file (design.json, notes, screens, assets, or a temp file of an
/// atomic write) `rel` is; its `plan.md` is not one.
fn design_slug(rel: &Path) -> Option<String> {
    let parts: Vec<_> = rel.components().map(|c| c.as_os_str().to_str()).collect();
    match parts[..] {
        [Some("plans"), Some(s), Some(f)] if f != "plan.md" => Some(s.into()),
        [Some("plans"), Some(s), Some(_), ..] if parts.len() > 3 => Some(s.into()),
        _ => None,
    }
}

/// Watches `root` and, after 300 ms of quiet, calls `on_change`. Ignored paths never count, except
/// `.ig/show.json`. Stops when the watcher is dropped.
pub fn watch(
    root: &Path,
    mut on_change: impl FnMut(Changed) + Send + 'static,
) -> notify::Result<RecommendedWatcher> {
    use notify::Watcher;
    let (tx, rx) = mpsc::channel::<notify::Result<notify::Event>>();
    let mut watcher = notify::recommended_watcher(move |e| {
        let _ = tx.send(e);
    })?;
    watcher.watch(root, notify::RecursiveMode::Recursive)?;
    let root = root.to_path_buf();
    let show = root.join(ig_app::show::SHOW);
    std::thread::spawn(move || {
        while let Ok(e) = rx.recv() {
            let (mut any, mut c) = (false, Changed::default());
            let mut see = |e: notify::Result<notify::Event>| {
                for p in e.iter().flat_map(|e| &e.paths) {
                    if *p == show {
                        any = true;
                        c.show = true;
                    } else if !ignored(&root, p) {
                        any = true;
                        if let Some(slug) = design_slug(p.strip_prefix(&root).unwrap_or(p)) {
                            c.designs.insert(slug);
                        } else {
                            c.full |= !p
                                .file_name()
                                .is_some_and(|f| f.to_string_lossy().contains(".notes."));
                        }
                    }
                }
            };
            see(e);
            while let Ok(e) = rx.recv_timeout(Duration::from_millis(300)) {
                see(e);
            }
            if any {
                on_change(c);
            }
        }
    });
    Ok(watcher)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn watch_reports_a_source_change_and_skips_ignored_paths() {
        let dir = std::env::temp_dir().join(format!("ig-wire-watch-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join(".ig")).unwrap();
        let dir = dir.canonicalize().unwrap();
        let (tx, rx) = mpsc::channel();
        let _w = watch(&dir, move |c| {
            let _ = tx.send(c);
        })
        .unwrap();
        std::thread::sleep(Duration::from_millis(200));
        std::fs::write(dir.join(".ig/view.html"), "x").unwrap();
        assert!(rx.recv_timeout(Duration::from_secs(2)).is_err(), ".ig");
        std::fs::write(dir.join("a.ts"), "export const a = 1;").unwrap();
        assert_eq!(
            rx.recv_timeout(Duration::from_secs(5)),
            Ok(Changed {
                full: true,
                ..Changed::default()
            })
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn watch_reports_show_changes_despite_the_ig_ignore() {
        let dir = std::env::temp_dir().join(format!("ig-wire-show-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join(".ig")).unwrap();
        let dir = dir.canonicalize().unwrap();
        let (tx, rx) = mpsc::channel();
        let _w = watch(&dir, move |c| {
            let _ = tx.send(c);
        })
        .unwrap();
        std::thread::sleep(Duration::from_millis(200));
        std::fs::write(dir.join(".ig/other"), "x").unwrap();
        assert!(
            rx.recv_timeout(Duration::from_secs(2)).is_err(),
            ".ig/other"
        );
        std::fs::write(dir.join(".ig/show.json"), "{}").unwrap();
        assert_eq!(
            rx.recv_timeout(Duration::from_secs(5)),
            Ok(Changed {
                show: true,
                ..Changed::default()
            })
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn watch_classifies_design_screen_and_asset_changes() {
        let dir = std::env::temp_dir().join(format!("ig-wire-design-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("plans/a/screens")).unwrap();
        let dir = dir.canonicalize().unwrap();
        let (tx, rx) = mpsc::channel();
        let _w = watch(&dir, move |c| {
            let _ = tx.send(c);
        })
        .unwrap();
        std::thread::sleep(Duration::from_millis(200));
        std::fs::write(dir.join("plans/a/design.json"), "{}").unwrap();
        std::fs::write(dir.join("plans/a/screens/s.json"), "{}").unwrap();
        let want = Changed {
            designs: BTreeSet::from(["a".to_string()]),
            ..Changed::default()
        };
        assert_eq!(rx.recv_timeout(Duration::from_secs(5)), Ok(want));
        std::fs::write(dir.join("plans/a/plan.md"), "# A").unwrap();
        assert!(rx.recv_timeout(Duration::from_secs(5)).unwrap().full);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
