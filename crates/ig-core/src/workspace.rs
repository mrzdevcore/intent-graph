//! Workspace packages and the glob flavour ig uses everywhere (pure; discovery stays in the binary).

use globset::{Glob, GlobBuilder, GlobSet, GlobSetBuilder};
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct Package {
    pub name: String,
    /// Repo-relative posix dir ("." for the root).
    pub dir: String,
    pub tsconfig: Option<PathBuf>,
    /// Repo-relative source entry file.
    pub source_entry: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SourceFile {
    /// Repo-relative posix path.
    pub path: String,
    pub package: usize,
}

/// A discovered tree: its root, packages and source files.
#[derive(Debug, Clone)]
pub struct WorkspaceInfo {
    pub root: PathBuf,
    pub packages: Vec<Package>,
    pub files: Vec<SourceFile>,
}

pub fn glob(pattern: &str) -> Result<Glob, globset::Error> {
    GlobBuilder::new(pattern).literal_separator(true).build()
}

pub fn globset(patterns: &[String]) -> Result<GlobSet, globset::Error> {
    let mut b = GlobSetBuilder::new();
    for p in patterns {
        b.add(glob(p)?);
    }
    b.build()
}

pub fn join(dir: &str, path: &str) -> String {
    if dir == "." {
        path.to_string()
    } else {
        format!("{dir}/{path}")
    }
}
