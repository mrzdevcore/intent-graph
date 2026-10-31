//! The composition root shared by the driving adapters (`ig` and `ig-desktop`): the real adapters
//! plugged into `ig_app::Env`, and the debounced watcher behind live rebuilds.

use ig_app::Env;
use ig_core::ports::{LanguageFrontend, Vcs};
use ig_core::workspace::NEVER_WALKED;
use ig_fs::FsFiles;
use ig_git::GitVcs;
use ig_lang_py::PyFrontend;
use ig_lang_rs::RsFrontend;
use ig_lang_ts::TsFrontend;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Duration;

pub use notify::RecommendedWatcher;

/// Every language ig reads, in the order they claim a file.
pub static FRONTENDS: &[&dyn LanguageFrontend] = &[&TsFrontend, &PyFrontend, &RsFrontend];

/// A progress line on stderr, only when it is a terminal (hooks and JSON consumers stay clean).
/// It clears the line first: `ig view --read`'s progress bar may be on it, and is redrawn below.
fn progress(msg: &str) {
    use std::io::IsTerminal;
    if std::io::stderr().is_terminal() {
        eprintln!("\r\x1b[2Kig: {msg}");
    }
}

/// Notices: always on stderr.
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
            frontends: FRONTENDS,
            progress: &progress,
            notice: &notice,
        }
    }
}

/// Paths a live view never rebuilds for: ig's own output, git internals, dependencies.
pub fn ignored(root: &Path, p: &Path) -> bool {
    let rel = p.strip_prefix(root).unwrap_or(p);
    (rel.components()).any(|c| NEVER_WALKED.contains(&c.as_os_str().to_str().unwrap_or("")))
}

/// The `rel` paths git ignores, by .gitignore, .git/info/exclude and the global excludes, checked
/// in chunks. Outside a repository, on a git timeout, or above `MAX_CHECKED` paths: none (rebuild).
fn git_ignored(root: &Path, rel: &BTreeSet<PathBuf>) -> BTreeSet<PathBuf> {
    git_ignored_with("git", Duration::from_secs(5), root, rel)
}

fn git_ignored_with(
    git: &str,
    deadline: Duration,
    root: &Path,
    rel: &BTreeSet<PathBuf>,
) -> BTreeSet<PathBuf> {
    const MAX_CHECKED: usize = 20_000;
    const CHUNK: usize = 500;
    if rel.len() > MAX_CHECKED {
        return BTreeSet::new();
    }
    let all: Vec<_> = rel.iter().collect();
    let mut out = BTreeSet::new();
    for chunk in all.chunks(CHUNK) {
        match check_chunk(git, deadline, root, chunk) {
            Some(found) => out.extend(found),
            None => return BTreeSet::new(),
        }
    }
    out
}

/// One `git check-ignore` run; None when git is missing, fails to run, or outlives `deadline`.
fn check_chunk(
    git: &str,
    deadline: Duration,
    root: &Path,
    chunk: &[&PathBuf],
) -> Option<Vec<PathBuf>> {
    use std::io::{Read, Write};
    use std::process::{Command, Stdio};
    let mut input = Vec::new();
    for p in chunk {
        input.extend_from_slice(p.to_string_lossy().as_bytes());
        input.push(0);
    }
    let mut child = Command::new(git)
        .current_dir(root)
        .args(["check-ignore", "--stdin", "-z"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let mut stdin = child.stdin.take()?;
    let mut stdout = child.stdout.take()?;
    let writer = std::thread::spawn(move || stdin.write_all(&input));
    let reader = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = stdout.read_to_end(&mut buf);
        buf
    });
    let start = std::time::Instant::now();
    let done = loop {
        match child.try_wait() {
            Ok(Some(_)) => break true,
            Ok(None) if start.elapsed() < deadline => std::thread::sleep(Duration::from_millis(5)),
            _ => break false,
        }
    };
    if !done {
        let _ = child.kill();
        let _ = child.wait();
    }
    if !done {
        return None; // the threads end with the pipes; a grandchild holding them is not waited for
    }
    let _ = writer.join();
    let buf = reader.join().ok()?;
    Some({
        buf.split(|b| *b == 0)
            .filter(|s| !s.is_empty())
            .map(|s| PathBuf::from(String::from_utf8_lossy(s).into_owned()))
            .collect()
    })
}

/// What changed in one quiet period: `full` when a source-ish file did, `show` when
/// `.ig/show.json` did (an agent's `ig_show`).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Changed {
    pub full: bool,
    pub show: bool,
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
            let mut maybe: BTreeSet<PathBuf> = BTreeSet::new();
            let mut see = |e: notify::Result<notify::Event>| {
                for p in e.iter().flat_map(|e| &e.paths) {
                    if *p == show {
                        any = true;
                        c.show = true;
                    } else if !ignored(&root, p) {
                        maybe.insert(p.strip_prefix(&root).unwrap_or(p).to_path_buf());
                    }
                }
            };
            see(e);
            while let Ok(e) = rx.recv_timeout(Duration::from_millis(300)) {
                see(e);
            }
            // Source-ish changes count unless git ignores every one of them (.nx, dist, coverage).
            if !maybe.is_empty() {
                let skip = git_ignored(&root, &maybe);
                if maybe.iter().any(|p| !skip.contains(p)) {
                    any = true;
                    c.full = true;
                }
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
    fn watch_skips_gitignored_changes_but_not_tracked_sources() {
        let dir = std::env::temp_dir().join(format!("ig-wire-gi-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("dist")).unwrap();
        let dir = dir.canonicalize().unwrap();
        let git = |a: &[&str]| {
            assert!(
                std::process::Command::new("git")
                    .current_dir(&dir)
                    .args(a)
                    .status()
                    .unwrap()
                    .success()
            )
        };
        git(&["init", "-q"]);
        std::fs::write(dir.join(".gitignore"), "dist/\n").unwrap();
        std::fs::create_dir_all(dir.join(".git/info")).unwrap();
        std::fs::write(dir.join(".git/info/exclude"), "cov.out\n").unwrap();
        let (tx, rx) = mpsc::channel();
        let _w = watch(&dir, move |c| {
            let _ = tx.send(c);
        })
        .unwrap();
        std::thread::sleep(Duration::from_millis(200));
        std::fs::write(dir.join("dist/a.js"), "x").unwrap();
        std::fs::write(dir.join("cov.out"), "x").unwrap();
        assert!(rx.recv_timeout(Duration::from_secs(2)).is_err(), "ignored");
        std::fs::write(dir.join("a.ts"), "export const a = 1;").unwrap();
        assert!(rx.recv_timeout(Duration::from_secs(5)).unwrap().full);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn git_ignored_chunks_and_gives_up_on_a_slow_git() {
        let dir = std::env::temp_dir().join(format!("ig-wire-chunk-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let dir = dir.canonicalize().unwrap();
        assert!(
            std::process::Command::new("git")
                .current_dir(&dir)
                .args(["init", "-q"])
                .status()
                .unwrap()
                .success()
        );
        std::fs::write(dir.join(".gitignore"), "dist/\n").unwrap();
        let rel: BTreeSet<PathBuf> = (0..3_000).map(|i| format!("dist/f{i}").into()).collect();
        let secs = Duration::from_secs(5);
        assert_eq!(git_ignored_with("git", secs, &dir, &rel).len(), 3_000);
        let slow = dir.join("slow-git");
        std::fs::write(&slow, "#!/bin/sh\nsleep 5\n").unwrap();
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&slow, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        let t = std::time::Instant::now();
        let none = git_ignored_with(
            slow.to_str().unwrap(),
            Duration::from_millis(200),
            &dir,
            &rel,
        );
        assert!(none.is_empty() && t.elapsed() < Duration::from_secs(3));
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
}
