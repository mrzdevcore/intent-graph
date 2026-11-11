//! Package and source file discovery.

use ig_core::config::Config;
use ig_core::ports::PortError;
use ig_core::workspace::{Layout, NEVER_WALKED, Package, SourceFile, WorkspaceInfo, globset, rel};
use std::collections::BTreeSet;
use std::path::Path;
use std::sync::Arc;

fn io(e: impl std::fmt::Display) -> PortError {
    PortError::Io(e.to_string())
}

/// `layout`'s packages (else the root, named after its folder) and their source files, under the
/// canonical `root`.
pub fn discover(root: &Path, config: &Config, layout: &Layout) -> Result<WorkspaceInfo, PortError> {
    discover_with(root, config, layout, git_visible(root).map(Arc::new))
}

pub(crate) fn discover_with(
    root: &Path,
    config: &Config,
    layout: &Layout,
    tracked: Option<Arc<BTreeSet<String>>>,
) -> Result<WorkspaceInfo, PortError> {
    let root = root.canonicalize().map_err(io)?;
    let mut packages = layout.packages.clone();
    if (packages.is_empty() || layout.keep_root) && !packages.iter().any(|p| p.dir == ".") {
        let name = root
            .file_name()
            .map_or_else(|| ".".into(), |n| n.to_string_lossy().to_string());
        packages.insert(
            0,
            Package {
                name,
                dir: ".".into(),
            },
        );
    }
    let files = discover_files(&root, &packages, config, layout, tracked.as_deref())?;
    Ok(WorkspaceInfo {
        root,
        packages,
        files,
    })
}

/// Build output and dependencies, never source, at any depth outside git (`.`-folders are
/// skipped too, and `NEVER_WALKED` everywhere).
pub const BUILD_DIRS: [&str; 10] = [
    "target",
    "dist",
    "build",
    "out",
    "coverage",
    "vendor",
    "node_modules",
    "venv",
    "__pycache__",
    "site-packages",
];

/// Files git would list (tracked or untracked, minus `.gitignore`d). None when git cannot speak
/// for `root`: not a repo, git missing, or nothing listed (as when `root` itself is ignored by an
/// enclosing repo). One git run, its errors silenced.
// ponytail: a root ignored by an enclosing repo yet holding force-added files gets git's list
// (those files only); `git check-ignore` first would cost a second spawn per run
pub(crate) fn git_visible(root: &Path) -> Option<BTreeSet<String>> {
    let out = std::process::Command::new("git")
        .args([
            "ls-files",
            "-z",
            "--cached",
            "--others",
            "--exclude-standard",
        ])
        .current_dir(root)
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output()
        .ok()
        .filter(|o| o.status.success())?;
    let set: BTreeSet<String> = String::from_utf8_lossy(&out.stdout)
        .split('\0')
        .filter(|f| !f.is_empty())
        .map(String::from)
        .collect();
    (!set.is_empty()).then_some(set)
}

fn discover_files(
    root: &Path,
    packages: &[Package],
    config: &Config,
    layout: &Layout,
    tracked: Option<&BTreeSet<String>>,
) -> Result<Vec<SourceFile>, PortError> {
    let ignore = globset(&config.ignore).map_err(io)?;
    let pkg_dirs: BTreeSet<&str> = packages.iter().map(|p| p.dir.as_str()).collect();
    let mut files = vec![];
    for (pi, pkg) in packages.iter().enumerate() {
        let mut stack = vec![root.join(&pkg.dir)];
        while let Some(dir) = stack.pop() {
            let Ok(rd) = std::fs::read_dir(&dir) else {
                continue;
            };
            for e in rd.flatten() {
                let name = e.file_name().to_string_lossy().to_string();
                let p = e.path();
                let Some(r) = rel(root, &p) else { continue };
                let Ok(ft) = e.file_type() else { continue };
                if ft.is_dir() {
                    // in a git repo git decides; only dependencies and tool folders are never walked
                    let skip = NEVER_WALKED.contains(&name.as_str())
                        || (tracked.is_none()
                            && (BUILD_DIRS.contains(&name.as_str()) || name.starts_with('.')));
                    if skip {
                        continue;
                    }
                    if pkg_dirs.contains(r.as_str())
                        || layout.fences.iter().any(|m| p.join(m).is_file())
                    {
                        continue;
                    }
                    if ignore.is_match(&r) {
                        continue;
                    }
                    stack.push(p);
                } else if ft.is_file()
                    && (layout.source)(&r)
                    && !ignore.is_match(&r)
                    && tracked.as_ref().is_none_or(|t| t.contains(&r))
                {
                    files.push(SourceFile {
                        path: r,
                        package: pi,
                    });
                }
            }
        }
    }
    files.sort_by(|a, b| a.path.cmp(&b.path));
    files.dedup_by(|a, b| a.path == b.path);
    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project(name: &str, git: bool) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("ig-disc-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        for f in [
            "src/a.ts",
            "target/x.js",
            "dist/y.js",
            ".next/z.js",
            "src/build/b.ts",
            "generated/g.ts",
        ] {
            std::fs::create_dir_all(d.join(f).parent().unwrap()).unwrap();
            std::fs::write(d.join(f), "export const a = 1;\n").unwrap();
        }
        std::fs::write(d.join(".gitignore"), "generated/\ntarget/\ndist/\n.next/\n").unwrap();
        if git {
            let ok = std::process::Command::new("git")
                .arg("init")
                .arg("-q")
                .current_dir(&d)
                .status()
                .unwrap()
                .success();
            assert!(ok);
        }
        d
    }

    #[test]
    fn git_repo_keeps_tracked_files_in_build_named_folders() {
        let d = project("tracked", true);
        for f in ["vendor/v.ts", "src/build/b.ts"] {
            std::fs::create_dir_all(d.join(f).parent().unwrap()).unwrap();
            std::fs::write(d.join(f), "export const v = 1;\n").unwrap();
        }
        let ok = std::process::Command::new("git")
            .args(["add", "-f", "vendor/v.ts", "src/build/b.ts"])
            .current_dir(&d)
            .status()
            .unwrap()
            .success();
        assert!(ok);
        assert_eq!(paths(&d), ["src/a.ts", "src/build/b.ts", "vendor/v.ts"]);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn root_ignored_by_enclosing_repo_is_analysed() {
        let d = project("enclosed", true);
        std::fs::write(d.join(".gitignore"), "examples/\n").unwrap();
        std::fs::create_dir_all(d.join("examples/x")).unwrap();
        std::fs::write(d.join("examples/x/a.ts"), "export const a = 1;\n").unwrap();
        assert_eq!(paths(&d.join("examples/x")), ["a.ts"]);
        let _ = std::fs::remove_dir_all(&d);
    }

    fn paths(d: &Path) -> Vec<String> {
        let source = |p: &str| p.ends_with(".ts");
        let layout = Layout {
            packages: vec![],
            keep_root: true,
            fences: vec![],
            source: &source,
        };
        let ws = discover(d, &Config::default(), &layout).unwrap();
        ws.files.into_iter().map(|f| f.path).collect()
    }

    #[test]
    fn skips_build_output() {
        let d = project("git", true);
        assert_eq!(paths(&d), ["src/a.ts", "src/build/b.ts"]);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn skips_build_output_without_git() {
        let d = project("nogit", false);
        assert_eq!(paths(&d), ["generated/g.ts", "src/a.ts"]); // src/build/ is build output too
        let _ = std::fs::remove_dir_all(&d);
    }
}
