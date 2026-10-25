//! Workspace packages, posix path helpers and the glob flavour ig uses everywhere (pure; discovery is in ig-fs).

use globset::{Glob, GlobBuilder, GlobSet, GlobSetBuilder};
use std::path::{Path, PathBuf};

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

pub fn is_source_ext(name: &str, embedded: &[String]) -> bool {
    let lower = name.to_ascii_lowercase();
    if lower.ends_with(".d.ts") || lower.ends_with(".d.mts") || lower.ends_with(".d.cts") {
        return false;
    }
    [".ts", ".tsx", ".js", ".jsx", ".mts", ".cts", ".mjs", ".cjs"]
        .iter()
        .any(|e| lower.ends_with(e))
        || embedded
            .iter()
            .any(|e| lower.ends_with(&e.to_ascii_lowercase()))
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

/// `p` relative to `root`, posix ("." when equal); None outside `root`.
pub fn rel(root: &Path, p: &Path) -> Option<String> {
    let r = p.strip_prefix(root).ok()?;
    let s = r.to_string_lossy().replace('\\', "/");
    Some(if s.is_empty() { ".".into() } else { s })
}

/// Lexically normalises a posix path ("a/./b/../c" → "a/c").
pub fn normalize(p: &str) -> String {
    let mut parts: Vec<&str> = vec![];
    for seg in p.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            s => parts.push(s),
        }
    }
    if parts.is_empty() {
        ".".into()
    } else {
        parts.join("/")
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn normalize() {
        assert_eq!(super::normalize("a/./b/../c"), "a/c");
    }
}
