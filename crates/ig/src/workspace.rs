//! `ig_fs` discovery plus workspace import resolution (moves to the TS frontend in Task 6).

use crate::config::Config;
use anyhow::Result;
use std::path::{Path, PathBuf};

pub use ig_core::workspace::{
    Package, SourceFile, glob, globset, is_source_ext, join, normalize, rel,
};
pub use ig_fs::read_jsonc;

pub struct Workspace {
    pub root: PathBuf,
    pub packages: Vec<Package>,
    pub files: Vec<SourceFile>,
}

impl Workspace {
    pub fn discover(root: &Path, config: &Config) -> Result<Workspace> {
        let w = ig_fs::discover(root, config)?;
        Ok(Workspace {
            root: w.root,
            packages: w.packages,
            files: w.files,
        })
    }

    /// `pkg` or `pkg/sub` → a repo-relative file.
    pub fn resolve_workspace_import(&self, spec: &str) -> Option<String> {
        let pkg = self
            .packages
            .iter()
            .filter(|p| spec == p.name || spec.starts_with(&format!("{}/", p.name)))
            .max_by_key(|p| p.name.len())?;
        let sub = spec[pkg.name.len()..].trim_start_matches('/');
        if !sub.is_empty() {
            for ext in [".ts", ".tsx"] {
                let cand = join(&pkg.dir, &format!("src/{sub}{ext}"));
                if self.root.join(&cand).is_file() {
                    return Some(cand);
                }
            }
        }
        pkg.source_entry.clone()
    }
}
