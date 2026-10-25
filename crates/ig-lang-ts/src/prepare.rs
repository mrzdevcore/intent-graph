//! Checkout preparation: links what TS resolution needs but git lacks.

use crate::tsconfig::read_jsonc;
use ig_core::config::Config;
use ig_core::workspace::WorkspaceInfo;
use std::path::{Path, PathBuf};

/// Links the working tree's node_modules (root and package dirs) and missing tsconfig `extends`
/// targets into the checkout whose ig root is `target_root`.
pub fn link_node_modules(work: &WorkspaceInfo, target_root: &Path) {
    let root = work
        .root
        .canonicalize()
        .unwrap_or_else(|_| work.root.clone());
    for src in node_modules_sources(&root, work) {
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

/// node_modules dirs to link into worktrees: the root and every package dir.
fn node_modules_sources(root: &Path, work: &WorkspaceInfo) -> Vec<PathBuf> {
    let mut v = vec![root.to_path_buf()];
    v.extend(
        work.packages
            .iter()
            .filter(|p| p.dir != ".")
            .map(|p| root.join(&p.dir)),
    );
    v
}

/// A tsconfig often extends or references generated, gitignored files (Nuxt's `.nuxt/`). When one is
/// missing from the worktree but present in the working tree, link the missing top-level entry.
fn link_missing_extends(src: &Path, target: &Path) {
    let Some(ts) = read_jsonc(&target.join("tsconfig.json")) else {
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
pub fn link_missing_auto_imports(src: &Path, target: &Path, config: &Config) {
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
}
