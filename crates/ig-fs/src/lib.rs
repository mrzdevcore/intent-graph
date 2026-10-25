//! The `Files` adapter: the real filesystem, workspace discovery, globs and atomic writes.

mod discover;

pub use discover::discover;

use ig_core::config::Config;
use ig_core::ports::{Files, PortError};
use ig_core::workspace::{WorkspaceInfo, globset, rel};
use serde_json::Value;
use std::io::Read;
use std::path::{Path, PathBuf};

/// Files under `root`.
pub struct FsFiles {
    pub root: PathBuf,
}

impl FsFiles {
    pub fn new(root: impl Into<PathBuf>) -> FsFiles {
        FsFiles { root: root.into() }
    }
}

fn io(e: impl std::fmt::Display) -> PortError {
    PortError::Io(e.to_string())
}

impl Files for FsFiles {
    fn read_bytes(&self, path: &str) -> Option<Vec<u8>> {
        std::fs::read(self.root.join(path)).ok()
    }
    fn write_atomic(&self, path: &str, bytes: &[u8]) -> Result<(), PortError> {
        let p = self.root.join(path);
        if let Some(dir) = p.parent() {
            std::fs::create_dir_all(dir).map_err(io)?;
        }
        write_atomic(&p, bytes).map_err(io)
    }
    fn remove(&self, path: &str) -> Result<(), PortError> {
        match std::fs::remove_file(self.root.join(path)) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(io(e)),
            _ => Ok(()),
        }
    }
    fn exists(&self, path: &str) -> bool {
        self.root.join(path).is_file()
    }
    fn glob(&self, patterns: &[String]) -> Result<Vec<String>, PortError> {
        let set = globset(patterns).map_err(io)?;
        let mut out = vec![];
        if patterns.is_empty() {
            return Ok(out);
        }
        let mut stack = vec![self.root.clone()];
        while let Some(dir) = stack.pop() {
            let Ok(rd) = std::fs::read_dir(&dir) else {
                continue;
            };
            for e in rd.flatten() {
                let p = e.path();
                let Some(r) = rel(&self.root, &p) else {
                    continue;
                };
                // `DirEntry::file_type` does not follow symlinks: they are neither.
                let Ok(ft) = e.file_type() else { continue };
                if ft.is_dir() {
                    let name = e.file_name();
                    if !matches!(name.to_str(), Some("node_modules" | ".git" | ".ig")) {
                        stack.push(p);
                    }
                } else if ft.is_file() && set.is_match(&r) {
                    out.push(r);
                }
            }
        }
        out.sort();
        Ok(out)
    }
    fn at(&self, root: &Path) -> Box<dyn Files> {
        Box::new(FsFiles::new(self.root.join(root)))
    }
    fn discover(&self, config: &Config) -> Result<WorkspaceInfo, PortError> {
        discover(&self.root, config)
    }
}

/// Writes through `<path>.tmp-<pid>` and a rename. The parent must exist.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let tmp = path.with_extension(format!("tmp-{}", std::process::id()));
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, path)
}

/// Parses JSON with comments and trailing commas.
pub fn read_jsonc(path: &Path) -> Option<Value> {
    let text = std::fs::read_to_string(path).ok()?;
    let mut stripped = String::new();
    json_comments::StripComments::new(text.as_bytes())
        .read_to_string(&mut stripped)
        .ok()?;
    serde_json::from_str(&strip_trailing_commas(&stripped)).ok()
}

fn strip_trailing_commas(s: &str) -> String {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("ig-fs-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn fs_files_meets_the_contract() {
        let d = tmp("contract");
        ig_core::testing::files_contract(&FsFiles::new(&d));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[cfg(unix)]
    #[test]
    fn glob_does_not_follow_symlinks() {
        let d = tmp("symlink");
        let f = FsFiles::new(&d);
        f.write_atomic("real/a.ts", b"").unwrap();
        std::os::unix::fs::symlink(d.join("real"), d.join("link")).unwrap();
        std::os::unix::fs::symlink(d.join("real/a.ts"), d.join("b.ts")).unwrap();
        let found = f.glob(&["**/*.ts".into()]).unwrap();
        let _ = std::fs::remove_dir_all(&d);
        assert_eq!(found, ["real/a.ts"]);
    }

    #[test]
    fn jsonc() {
        assert_eq!(
            strip_trailing_commas("{\"a\": [1, 2,], \"b\": \",]\",}"),
            "{\"a\": [1, 2], \"b\": \",]\"}"
        );
    }
}
