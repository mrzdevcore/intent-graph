//! Base-graph orchestration and TS checkout preparation on top of `ig_git` (moves to ig-app and
//! the TS frontend in Tasks 6–7).

use crate::config::Config;
use crate::graph;
use crate::model::Graph;
use anyhow::Result;
use ig_core::ports::Vcs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

pub use ig_git::{GitVcs, git};

static TMP_COUNTER: AtomicU32 = AtomicU32::new(0);

/// Links the working tree's node_modules (root and package dirs) and missing tsconfig `extends`
/// targets into the checkout whose ig root is `target_root`.
pub fn link_node_modules(root: &Path, config: &Config, target_root: &Path) {
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    for src in node_modules_sources(&root, config) {
        let nm = src.join("node_modules");
        let Ok(rel) = src.strip_prefix(&root) else {
            continue;
        };
        let target_dir = target_root.join(rel);
        if nm.is_dir() && target_dir.is_dir() && !target_dir.join("node_modules").exists() {
            #[cfg(unix)]
            let _ = std::os::unix::fs::symlink(&nm, target_dir.join("node_modules"));
        }
        link_missing_extends(&src, &target_dir);
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

/// The base-graph cache key of the working tree at `root` (`ig_core::cache::cache_key`).
pub fn base_cache_key(root: &Path, config: &Config) -> String {
    let decls: Vec<(String, String)> = crate::autoimports::files(root, &config.auto_imports)
        .0
        .into_iter()
        .map(|f| {
            let text = std::fs::read_to_string(root.join(&f)).unwrap_or_default();
            (f, text)
        })
        .collect();
    ig_core::cache::cache_key(&config.graph_key_input(), &decls)
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
pub fn base_graph(root: &Path, repo: &GitVcs, sha: &str, config: &Config) -> Result<Graph> {
    let cache_dir = root.join(".ig/cache");
    let path = cache_dir.join(format!("{sha}-{}.json", base_cache_key(root, config)));
    if let Ok(text) = std::fs::read_to_string(&path)
        && let Ok(g) = serde_json::from_str::<Graph>(&text)
    {
        return Ok(g);
    }
    crate::check::progress(&format!("base graph {}…", &sha[..sha.len().min(7)]));
    let g = graph_at(root, repo, sha, config)?;
    std::fs::create_dir_all(&cache_dir)?;
    let tmp = cache_dir.join(format!(
        ".tmp-{}-{}",
        std::process::id(),
        TMP_COUNTER.fetch_add(1, Ordering::SeqCst)
    ));
    std::fs::write(&tmp, serde_json::to_vec(&g)?)?;
    std::fs::rename(&tmp, &path)?;
    Ok(g)
}

/// Builds the graph of commit `sha` in a temporary worktree.
fn graph_at(root: &Path, repo: &GitVcs, sha: &str, config: &Config) -> Result<Graph> {
    let wt = repo.checkout(sha)?;
    let Some(r) = wt.root() else {
        // The ig root did not exist at that commit.
        let mut g = Graph::empty();
        g.commit = Some(sha.to_string());
        return Ok(g);
    };
    link_node_modules(root, config, r);
    link_missing_auto_imports(root, r, config);
    Ok(graph::build(r, config, Some(sha.to_string()))?.graph)
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
        let before = base_cache_key(&root, &config);
        std::fs::write(
            root.join(".nuxt/imports.d.ts"),
            "export { a } from '../a'\n",
        )
        .unwrap();
        assert_ne!(before, base_cache_key(&root, &config));
    }
}
