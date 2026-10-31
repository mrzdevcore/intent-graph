//! The `Files` adapter: the real filesystem, workspace discovery, globs and atomic writes.

mod discover;

pub use discover::{BUILD_DIRS, discover};

use ig_core::config::Config;
use ig_core::ports::{Files, PortError};
use ig_core::workspace::{Layout, NEVER_WALKED, WorkspaceInfo, globset, rel};
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

impl FsFiles {
    /// `dir` (an existing folder) resolves inside the project's root.
    fn inside(&self, dir: &Path, path: &str) -> Result<(), PortError> {
        if self.root.as_os_str().is_empty() {
            return Ok(());
        }
        let root = self.root.canonicalize().map_err(io)?;
        if !dir.canonicalize().map_err(io)?.starts_with(&root) {
            return Err(PortError::Io(format!(
                "{path}: refused, a symlink leads outside the project"
            )));
        }
        Ok(())
    }

    /// `p`'s folder resolves inside the root and `p` itself is not a symlink: a write or a
    /// removal never reaches outside the project. An empty root (the CLI's working directory,
    /// for paths the user names) confines nothing.
    fn contained(&self, p: &Path, path: &str) -> Result<(), PortError> {
        if self.root.as_os_str().is_empty() {
            return Ok(());
        }
        if let Some(d) = p.parent() {
            self.inside(d, path)?;
        }
        if std::fs::symlink_metadata(p).is_ok_and(|m| m.file_type().is_symlink()) {
            return Err(PortError::Io(format!("{path}: refused, it is a symlink")));
        }
        Ok(())
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
        // the deepest existing folder first, so no folder is ever made outside the project (a
        // root not made yet has none of its own)
        let mut have = p.parent();
        while let Some(d) = have.filter(|d| !d.exists()) {
            have = d.parent();
        }
        if let Some(d) = have.filter(|d| d.starts_with(&self.root)) {
            self.inside(d, path)?;
        }
        if let Some(dir) = p.parent() {
            std::fs::create_dir_all(dir).map_err(io)?;
        }
        self.contained(&p, path)?;
        write_atomic(&p, bytes).map_err(io)
    }
    fn remove(&self, path: &str) -> Result<(), PortError> {
        let p = self.root.join(path);
        if p.parent().is_some_and(|d| d.exists()) {
            self.contained(&p, path)?;
        }
        match std::fs::remove_file(p) {
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
        // no deeper than the deepest pattern reaches, unless one has `**`
        let deep = patterns.iter().any(|p| p.contains("**"));
        let depth = patterns
            .iter()
            .map(|p| p.split('/').count())
            .max()
            .unwrap_or(0);
        let mut stack = vec![(self.root.clone(), 1)];
        while let Some((dir, at)) = stack.pop() {
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
                    let walked = !NEVER_WALKED.contains(&name.to_str().unwrap_or(""));
                    if walked && (deep || at < depth) {
                        stack.push((p, at + 1));
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
    fn discover(&self, config: &Config, layout: &Layout) -> Result<WorkspaceInfo, PortError> {
        discover(&self.root, config, layout)
    }
}

/// Writes through `<path>.tmp-<pid>` and a rename. The parent must exist.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let tmp = path.with_extension(format!("tmp-{}", std::process::id()));
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, path)
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

    #[cfg(unix)]
    #[test]
    fn writes_never_leave_the_project_through_a_symlink() {
        let (d, out) = (tmp("escape"), tmp("escape-out"));
        let f = FsFiles::new(&d);
        std::fs::create_dir_all(d.join("plans/x")).unwrap();
        std::os::unix::fs::symlink(&out, d.join("plans/x/screens")).unwrap();
        std::fs::write(out.join("a.json"), "theirs").unwrap();
        let e = f.write_atomic("plans/x/screens/a.json", b"{}").unwrap_err();
        assert!(e.to_string().contains("outside the project"), "{e}");
        assert!(
            f.write_atomic("plans/x/screens/deep/b.json", b"{}")
                .is_err()
        );
        assert!(f.remove("plans/x/screens/a.json").is_err());
        assert!(f.write_atomic("../escape-out/c.json", b"{}").is_err());
        // a symlinked file is refused too, wherever it points
        std::os::unix::fs::symlink(out.join("a.json"), d.join("plans/x/l.json")).unwrap();
        assert!(f.write_atomic("plans/x/l.json", b"{}").is_err());
        let left: Vec<_> = std::fs::read_dir(&out)
            .unwrap()
            .flatten()
            .map(|e| e.file_name())
            .collect();
        assert_eq!(
            std::fs::read_to_string(out.join("a.json")).unwrap(),
            "theirs"
        );
        let _ = (std::fs::remove_dir_all(&d), std::fs::remove_dir_all(&out));
        assert_eq!(left, ["a.json"]);
        // a project reached through a symlinked root still writes
        let (real, link) = (
            tmp("real-root"),
            std::env::temp_dir().join(format!("ig-fs-root-link-{}", std::process::id())),
        );
        let _ = std::fs::remove_file(&link);
        std::os::unix::fs::symlink(&real, &link).unwrap();
        FsFiles::new(&link)
            .write_atomic("plans/y/design.json", b"{}")
            .unwrap();
        let _ = (std::fs::remove_file(&link), std::fs::remove_dir_all(&real));
    }
}
