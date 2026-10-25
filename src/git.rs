//! Base/head resolution, worktrees and the base-graph cache. Shells out to `git`.

use crate::config::Config;
use crate::graph;
use crate::model::{Graph, TOOL_VERSION, sha1_hex};
use anyhow::{Context, Result, bail};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU32, Ordering};

pub fn git(dir: &Path, args: &[&str]) -> Result<String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .context("running git")?;
    if !out.status.success() {
        bail!(
            "git {}: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim_end().to_string())
}

pub fn git_bytes(dir: &Path, args: &[&str]) -> Option<Vec<u8>> {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .ok()?;
    out.status.success().then_some(out.stdout)
}

pub fn verify(dir: &Path, rev: &str) -> Option<String> {
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

/// The git checkout `root` lives in, and `root`'s path relative to its top level.
pub struct Repo {
    pub top: PathBuf,
    /// Posix path of the ig root under `top` ("" when equal).
    pub prefix: String,
}

impl Repo {
    pub fn open(root: &Path) -> Option<Repo> {
        let top = PathBuf::from(git(root, &["rev-parse", "--show-toplevel"]).ok()?)
            .canonicalize()
            .ok()?;
        let root = root.canonicalize().ok()?;
        let prefix = root
            .strip_prefix(&top)
            .ok()?
            .to_string_lossy()
            .replace('\\', "/");
        Some(Repo { top, prefix })
    }

    /// `path` relative to the ig root, as a path under the repo top level.
    pub fn top_path(&self, path: &str) -> String {
        if self.prefix.is_empty() {
            path.to_string()
        } else {
            format!("{}/{path}", self.prefix)
        }
    }

    pub fn show(&self, rev: &str, path: &str) -> Option<String> {
        let b = git_bytes(
            &self.top,
            &["show", &format!("{rev}:{}", self.top_path(path))],
        )?;
        Some(String::from_utf8_lossy(&b).into_owned())
    }

    pub fn head(&self) -> Option<String> {
        verify(&self.top, "HEAD")
    }

    pub fn is_clean(&self) -> bool {
        git(&self.top, &["status", "--porcelain"])
            .map(|s| s.is_empty())
            .unwrap_or(false)
    }

    /// Picks the base commit; pushes notices for fallbacks.
    pub fn resolve_base(
        &self,
        base: Option<&str>,
        notices: &mut Vec<String>,
    ) -> Result<Option<String>> {
        if let Some(b) = base {
            return match verify(&self.top, b) {
                Some(s) => Ok(Some(s)),
                None => bail!("--base {b}: not a commit"),
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
}

static WT_COUNTER: AtomicU32 = AtomicU32::new(0);

/// A temporary detached worktree, removed on drop.
pub struct Worktree {
    top: PathBuf,
    pub dir: PathBuf,
}

impl Worktree {
    pub fn add(repo: &Repo, rev: &str, link_node_modules_from: &[PathBuf]) -> Result<Worktree> {
        let n = WT_COUNTER.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!("ig-wt-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let d = dir.to_string_lossy().to_string();
        git(
            &repo.top,
            &["worktree", "add", "--detach", "--quiet", &d, rev],
        )?;
        let wt = Worktree {
            top: repo.top.clone(),
            dir: dir.canonicalize().unwrap_or(dir),
        };
        for src in link_node_modules_from {
            let nm = src.join("node_modules");
            let Ok(rel) = src.strip_prefix(&repo.top) else {
                continue;
            };
            let target_dir = wt.dir.join(rel);
            if nm.is_dir() && target_dir.is_dir() && !target_dir.join("node_modules").exists() {
                #[cfg(unix)]
                let _ = std::os::unix::fs::symlink(&nm, target_dir.join("node_modules"));
            }
            link_missing_extends(src, &target_dir);
        }
        Ok(wt)
    }

    /// The ig root inside this worktree.
    pub fn root(&self, repo: &Repo) -> PathBuf {
        if repo.prefix.is_empty() {
            self.dir.clone()
        } else {
            self.dir.join(&repo.prefix)
        }
    }
}

/// A tsconfig often extends or references generated, gitignored files (Nuxt's `.nuxt/`). When one is
/// missing from the worktree but present in the working tree, link the missing top-level entry.
fn link_missing_extends(src: &Path, target: &Path) {
    let Some(ts) = crate::workspace::read_jsonc(&target.join("tsconfig.json")) else {
        return;
    };
    let mut extends: Vec<&str> = match ts.get("extends") {
        Some(serde_json::Value::String(s)) => vec![s.as_str()],
        Some(serde_json::Value::Array(a)) => a.iter().filter_map(|v| v.as_str()).collect(),
        _ => vec![],
    };
    if let Some(refs) = ts.get("references").and_then(|r| r.as_array()) {
        extends.extend(
            refs.iter()
                .filter_map(|r| r.get("path").and_then(|p| p.as_str())),
        );
    }
    for e in extends.into_iter().filter(|e| e.starts_with('.')) {
        if target.join(e).exists() || !src.join(e).exists() {
            continue;
        }
        let first = Path::new(e)
            .components()
            .find(|c| matches!(c, std::path::Component::Normal(_)));
        let Some(first) = first else { continue };
        let prefix: PathBuf = Path::new(e)
            .components()
            .take_while(|c| *c != first)
            .chain([first])
            .collect();
        if !target.join(&prefix).exists() {
            #[cfg(unix)]
            let _ = std::os::unix::fs::symlink(src.join(&prefix), target.join(&prefix));
        }
    }
}

/// `autoImports` declaration files are generated and usually gitignored (`.nuxt/`): link the
/// working tree's copies into the worktree when missing there.
fn link_missing_auto_imports(src: &Path, target: &Path, config: &Config) {
    for f in crate::autoimports::files(src, &config.auto_imports).0 {
        let t = target.join(&f);
        if t.exists() {
            continue;
        }
        if let Some(parent) = t.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        #[cfg(unix)]
        let _ = std::os::unix::fs::symlink(src.join(&f), &t);
    }
}

impl Drop for Worktree {
    fn drop(&mut self) {
        let d = self.dir.to_string_lossy().to_string();
        let _ = git(&self.top, &["worktree", "remove", "--force", &d]);
        let _ = std::fs::remove_dir_all(&self.dir);
        let _ = git(&self.top, &["worktree", "prune"]);
    }
}

/// Bump whenever extraction or resolution changes output, so cached base graphs are rebuilt.
const GRAPH_REVISION: u32 = 14;

/// Config, tool and revision, plus the `autoImports` file contents (base graphs read the
/// working tree's copy when the commit lacks them, e.g. a gitignored `.nuxt/`).
pub fn cache_key(root: &Path, config: &Config) -> String {
    let decls: String = crate::autoimports::files(root, &config.auto_imports)
        .0
        .iter()
        .map(|f| {
            format!(
                "|{f}:{}",
                std::fs::read_to_string(root.join(f)).unwrap_or_default()
            )
        })
        .collect();
    sha1_hex(&format!(
        "{TOOL_VERSION}|{GRAPH_REVISION}|{}{decls}",
        config.graph_key_input()
    ))[..12]
        .to_string()
}

/// node_modules dirs to link into worktrees: the root and every package dir.
pub fn node_modules_sources(root: &Path, config: &Config) -> Vec<PathBuf> {
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let mut v = vec![root.clone()];
    if let Ok(ws) = crate::workspace::Workspace::discover(&root, config) {
        v.extend(
            ws.packages
                .iter()
                .filter(|p| p.dir != ".")
                .map(|p| root.join(&p.dir)),
        );
    }
    v
}

/// Builds (or loads from cache) the graph of commit `sha`.
pub fn base_graph(root: &Path, repo: &Repo, sha: &str, config: &Config) -> Result<Graph> {
    let cache_dir = root.join(".ig/cache");
    let path = cache_dir.join(format!("{sha}-{}.json", cache_key(root, config)));
    if let Ok(text) = std::fs::read_to_string(&path)
        && let Ok(g) = serde_json::from_str::<Graph>(&text)
    {
        return Ok(g);
    }
    let g = graph_at(root, repo, sha, config)?;
    std::fs::create_dir_all(&cache_dir)?;
    let tmp = cache_dir.join(format!(
        ".tmp-{}-{}",
        std::process::id(),
        WT_COUNTER.fetch_add(1, Ordering::SeqCst)
    ));
    std::fs::write(&tmp, serde_json::to_vec(&g)?)?;
    std::fs::rename(&tmp, &path)?;
    Ok(g)
}

/// Builds the graph of commit `sha` in a temporary worktree.
pub fn graph_at(root: &Path, repo: &Repo, sha: &str, config: &Config) -> Result<Graph> {
    let wt = Worktree::add(repo, sha, &node_modules_sources(root, config))?;
    let r = wt.root(repo);
    if !r.is_dir() {
        // The ig root did not exist at that commit.
        let mut g = Graph::empty();
        g.commit = Some(sha.to_string());
        return Ok(g);
    }
    link_missing_auto_imports(root, &r, config);
    Ok(graph::build(&r, config, Some(sha.to_string()))?.graph)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn links_missing_auto_import_files() {
        let src = crate::autoimports::tmp_with(&[(".nuxt/imports.d.ts", "export {}\n")]);
        let target = crate::autoimports::tmp_with(&[("src/a.ts", "")]);
        let config = Config {
            auto_imports: vec![".nuxt/*.d.ts".into()],
            ..Config::default()
        };
        link_missing_auto_imports(&src, &target, &config);
        assert!(target.join(".nuxt/imports.d.ts").is_file());
    }

    #[test]
    fn cache_key_tracks_auto_import_contents() {
        let root = crate::autoimports::tmp_with(&[(".nuxt/imports.d.ts", "export {}\n")]);
        let config = Config {
            auto_imports: vec![".nuxt/*.d.ts".into()],
            ..Config::default()
        };
        let before = cache_key(&root, &config);
        std::fs::write(
            root.join(".nuxt/imports.d.ts"),
            "export { a } from '../a'\n",
        )
        .unwrap();
        assert_ne!(before, cache_key(&root, &config));
    }

    #[test]
    fn base_resolution() {
        let dir = std::env::temp_dir().join(format!("ig-base-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let g = |args: &[&str]| {
            git(
                &dir,
                &[&["-c", "user.email=t@t", "-c", "user.name=t"][..], args].concat(),
            )
            .unwrap()
        };
        g(&["init", "-q", "-b", "trunk"]);
        let repo = Repo::open(&dir).unwrap();
        let mut notices = vec![];
        assert_eq!(repo.resolve_base(None, &mut notices).unwrap(), None);
        assert!(notices[0].contains("no commits"));
        g(&["commit", "-q", "--allow-empty", "-m", "one"]);
        let one = repo.head().unwrap();
        notices.clear();
        assert_eq!(
            repo.resolve_base(None, &mut notices).unwrap(),
            Some(one.clone())
        );
        assert!(notices[0].contains("comparing against HEAD"));
        g(&["branch", "main"]);
        g(&["commit", "-q", "--allow-empty", "-m", "two"]);
        notices.clear();
        assert_eq!(
            repo.resolve_base(None, &mut notices).unwrap(),
            Some(one.clone()),
            "merge-base with main"
        );
        assert!(notices.is_empty());
        assert_eq!(
            repo.resolve_base(Some("HEAD"), &mut notices).unwrap(),
            repo.head()
        );
        assert!(repo.resolve_base(Some("nope"), &mut notices).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
