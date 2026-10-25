//! The `Vcs` adapter: base/head resolution, history and worktrees. Shells out to `git`.

use ig_core::ports::{Checkout, PortError, Vcs};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU32, Ordering};

pub fn git(dir: &Path, args: &[&str]) -> Result<String, PortError> {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .map_err(|_| PortError::Vcs("running git".into()))?;
    if !out.status.success() {
        return Err(PortError::Vcs(format!(
            "git {}: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        )));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim_end().to_string())
}

fn git_bytes(dir: &Path, args: &[&str]) -> Option<Vec<u8>> {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .ok()?;
    out.status.success().then_some(out.stdout)
}

fn verify(dir: &Path, rev: &str) -> Option<String> {
    git(
        dir,
        &[
            "rev-parse",
            "--verify",
            "--quiet",
            &format!("{rev}^{{commit}}"),
        ],
    )
    .ok()
    .filter(|s| !s.is_empty())
}

/// The git checkout an ig root lives in.
pub struct GitVcs {
    /// The ig root.
    root: PathBuf,
    top: PathBuf,
    /// Posix path of the ig root under `top` ("" when equal).
    prefix: String,
}

impl GitVcs {
    /// None when `root` is not inside a git checkout.
    pub fn open(root: &Path) -> Option<GitVcs> {
        let top = PathBuf::from(git(root, &["rev-parse", "--show-toplevel"]).ok()?)
            .canonicalize()
            .ok()?;
        let root = root.canonicalize().ok()?;
        let prefix = root
            .strip_prefix(&top)
            .ok()?
            .to_string_lossy()
            .replace('\\', "/");
        Some(GitVcs { root, top, prefix })
    }

    /// Posix path of the ig root under the repo top level ("" when equal).
    pub fn prefix(&self) -> &str {
        &self.prefix
    }

    /// `path` relative to the ig root, as a path under the repo top level.
    fn top_path(&self, path: &str) -> String {
        if self.prefix.is_empty() {
            path.to_string()
        } else {
            format!("{}/{path}", self.prefix)
        }
    }
}

static WT_COUNTER: AtomicU32 = AtomicU32::new(0);

/// A temporary detached worktree, removed on drop.
struct Worktree {
    top: PathBuf,
    dir: PathBuf,
}

impl Drop for Worktree {
    fn drop(&mut self) {
        let d = self.dir.to_string_lossy().to_string();
        let _ = git(&self.top, &["worktree", "remove", "--force", &d]);
        let _ = std::fs::remove_dir_all(&self.dir);
        let _ = git(&self.top, &["worktree", "prune"]);
    }
}

impl Vcs for GitVcs {
    /// Outside a git checkout every call fails (None, empty or a notice).
    fn at(&self, root: &Path) -> Box<dyn Vcs> {
        Box::new(GitVcs::open(root).unwrap_or_else(|| GitVcs {
            root: root.into(),
            top: root.into(),
            prefix: String::new(),
        }))
    }

    fn head(&self) -> Option<String> {
        verify(&self.top, "HEAD")
    }

    fn resolve_base(
        &self,
        base: Option<&str>,
        notices: &mut Vec<String>,
    ) -> Result<Option<String>, PortError> {
        if let Some(b) = base {
            return match verify(&self.top, b) {
                Some(s) => Ok(Some(s)),
                None => Err(PortError::Vcs(format!("--base {b}: not a commit"))),
            };
        }
        if let Ok(env) = std::env::var("CI_MERGE_REQUEST_DIFF_BASE_SHA")
            && !env.is_empty()
        {
            match verify(&self.top, &env) {
                Some(s) => return Ok(Some(s)),
                None => notices.push(format!(
                    "CI_MERGE_REQUEST_DIFF_BASE_SHA {env} does not verify; falling back"
                )),
            }
        }
        for r in ["origin/HEAD", "main", "master"] {
            if verify(&self.top, r).is_some()
                && let Ok(mb) = git(&self.top, &["merge-base", "HEAD", r])
                && !mb.is_empty()
            {
                return Ok(Some(mb));
            }
        }
        match self.head() {
            Some(h) => {
                notices.push("no origin/HEAD, main or master: comparing against HEAD".into());
                Ok(Some(h))
            }
            None => {
                notices.push("repository has no commits: every node counts as added".into());
                Ok(None)
            }
        }
    }

    fn verify(&self, rev: &str) -> Option<String> {
        verify(&self.top, rev)
    }

    fn show(&self, rev: &str, path: &str) -> Option<String> {
        let b = git_bytes(
            &self.top,
            &["show", &format!("{rev}:{}", self.top_path(path))],
        )?;
        Some(String::from_utf8_lossy(&b).into_owned())
    }

    fn changed_files(&self, base: Option<&str>, notices: &mut Vec<String>) -> Vec<String> {
        let mut runs = vec![vec!["ls-files", "-z", "--others", "--exclude-standard"]];
        if let Some(b) = base {
            runs.push(vec!["diff", "-z", "--name-only", "--relative", b]);
        }
        let mut out = vec![];
        for args in runs {
            match git(&self.root, &args) {
                Ok(s) => out.extend(s.split('\0').filter(|f| !f.is_empty()).map(String::from)),
                Err(e) => notices.push(format!("changed files for checkers: {e}")),
            }
        }
        out
    }

    fn is_clean(&self) -> bool {
        git(&self.top, &["status", "--porcelain"])
            .map(|s| s.is_empty())
            .unwrap_or(false)
    }

    /// `git log --relative` from the ig root.
    fn history(&self, n: usize) -> Vec<Vec<String>> {
        let n = n.to_string();
        let Ok(log) = git(
            &self.root,
            &[
                "log",
                "--relative",
                "--name-only",
                "--format=%x00",
                "-n",
                &n,
            ],
        ) else {
            return vec![];
        };
        log.split('\0')
            .map(|commit| {
                commit
                    .lines()
                    .map(str::trim)
                    .filter(|l| !l.is_empty())
                    .map(String::from)
                    .collect()
            })
            .collect()
    }

    fn checkout(&self, rev: &str) -> Result<Checkout, PortError> {
        let n = WT_COUNTER.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!("ig-wt-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let d = dir.to_string_lossy().to_string();
        git(
            &self.top,
            &["worktree", "add", "--detach", "--quiet", &d, rev],
        )?;
        let wt = Worktree {
            top: self.top.clone(),
            dir: dir.canonicalize().unwrap_or(dir),
        };
        let root = match self.prefix.as_str() {
            "" => wt.dir.clone(),
            p => wt.dir.join(p),
        };
        Ok(Checkout::new(root.is_dir().then_some(root), Box::new(wt)))
    }
}

#[cfg(test)]
mod tests;
