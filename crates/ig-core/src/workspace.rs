//! Workspace packages, posix path helpers and the glob flavour ig uses everywhere (pure; discovery is in ig-fs).

use globset::{Glob, GlobBuilder, GlobSet, GlobSetBuilder};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq)]
pub struct Package {
    pub name: String,
    /// Repo-relative posix dir ("." for the root).
    pub dir: String,
}

/// What the language frontends tell discovery: the packages they declare (none: the root is one),
/// the manifest files that keep an undeclared nested package out (`package.json`), and which
/// files are source.
pub struct Layout<'a> {
    pub packages: Vec<Package>,
    pub fences: Vec<&'static str>,
    pub source: &'a (dyn Fn(&str) -> bool + Sync),
}

/// Folders never walked, in any language: installed dependencies and tool state.
pub const NEVER_WALKED: [&str; 7] = [
    "node_modules",
    ".venv",
    "venv",
    "__pycache__",
    "site-packages",
    ".git",
    ".ig",
];

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

/// JSON with its trailing commas (`[1, 2,]`, `{"a": 1,}`) removed, outside strings.
pub fn strip_trailing_commas(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let (mut in_str, mut esc) = (false, false);
    for (i, &c) in b.iter().enumerate() {
        if in_str {
            if esc {
                esc = false
            } else if c == b'\\' {
                esc = true
            } else if c == b'"' {
                in_str = false
            }
        } else if c == b'"' {
            in_str = true
        } else if c == b',' {
            let next = b[i + 1..].iter().find(|x| !x.is_ascii_whitespace());
            if matches!(next, Some(b'}') | Some(b']')) {
                continue;
            }
        }
        out.push(c);
    }
    String::from_utf8(out).unwrap_or_default()
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
    fn jsonc() {
        assert_eq!(
            super::strip_trailing_commas("{\"a\": [1, 2,], \"b\": \",]\",}"),
            "{\"a\": [1, 2], \"b\": \",]\"}"
        );
    }

    #[test]
    fn normalize() {
        assert_eq!(super::normalize("a/./b/../c"), "a/c");
    }
}
