//! `ig_fs` discovery for the binary.

use crate::config::Config;
use anyhow::Result;
use std::path::{Path, PathBuf};

pub use ig_core::workspace::{Package, SourceFile, globset, is_source_ext, join, normalize, rel};

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
}
