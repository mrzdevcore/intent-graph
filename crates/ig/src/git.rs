//! Base-graph orchestration on top of `ig_git` and the TS frontend's checkout preparation (moves
//! to ig-app in Task 7).

use crate::config::Config;
use crate::graph;
use crate::model::Graph;
use anyhow::Result;
use ig_core::ports::{LanguageFrontend, Vcs};
use ig_core::workspace::WorkspaceInfo;
use ig_lang_ts::TsFrontend;
use std::path::Path;
use std::sync::atomic::{AtomicU32, Ordering};

pub use ig_git::{GitVcs, git};

static TMP_COUNTER: AtomicU32 = AtomicU32::new(0);

/// The base-graph cache key of the working tree at `root` (`ig_core::cache::cache_key`).
pub fn base_cache_key(root: &Path, config: &Config) -> String {
    let decls: Vec<(String, String)> = TsFrontend::declaration_files(root, config)
        .into_iter()
        .map(|f| {
            let text = std::fs::read_to_string(root.join(&f)).unwrap_or_default();
            (f, text)
        })
        .collect();
    ig_core::cache::cache_key(&config.graph_key_input(), &decls)
}

/// The working tree's packages, for `prepare_checkout` (just the root when discovery fails).
pub fn work_info(root: &Path, config: &Config) -> WorkspaceInfo {
    ig_fs::discover(root, config).unwrap_or_else(|_| WorkspaceInfo {
        root: root.to_path_buf(),
        packages: vec![],
        files: vec![],
    })
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
    TsFrontend::new(config).prepare_checkout(&work_info(root, config), r, config, true)?;
    Ok(graph::build(r, config, Some(sha.to_string()))?.graph)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_key_tracks_auto_import_contents() {
        let root = crate::graph::tmp_with(&[(".nuxt/imports.d.ts", "export {}\n")]);
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
