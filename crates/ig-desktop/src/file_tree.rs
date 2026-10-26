//! The `file_tree` command: the direct children of a directory, gitignored files left out.

use ig_core::ports::Vcs;
use ig_wire::Wired;
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::Path;

#[derive(Serialize, Debug, PartialEq)]
pub struct Entry {
    /// Posix, relative to the project root.
    pub path: String,
    pub kind: &'static str,
}

/// Every file outside the fallback's skipped directories, relative and posix.
fn walk(root: &Path, rel: &str, out: &mut Vec<String>) {
    let Ok(rd) = std::fs::read_dir(root.join(rel)) else {
        return;
    };
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        let path = if rel.is_empty() {
            name.clone()
        } else {
            format!("{rel}/{name}")
        };
        match e.file_type() {
            Ok(t) if t.is_dir() => {
                if name != ".git" && name != "node_modules" {
                    walk(root, &path, out);
                }
            }
            Ok(t) if t.is_file() => out.push(path),
            _ => {}
        }
    }
}

/// The children of `dir` (None: the root), directories first, then files, alphabetical. Uses
/// `git ls-files -co --exclude-standard`; outside git, a walk that skips `.git` and `node_modules`.
pub fn file_tree_for(root: &Path, dir: Option<&str>) -> Vec<Entry> {
    let w = Wired::new(root);
    let all = match w.vcs() {
        Some(v) => v.list_files(),
        None => {
            let mut all = vec![];
            walk(root, "", &mut all);
            all
        }
    };
    children(&all, dir)
}

/// The children of `dir` (None: the root) among the project-relative `all` paths: directories first, then files.
pub fn children(all: &[String], dir: Option<&str>) -> Vec<Entry> {
    let prefix = dir
        .filter(|d| !d.trim_matches('/').is_empty())
        .map(|d| format!("{}/", d.trim_matches('/')))
        .unwrap_or_default();
    let mut kids: BTreeMap<&str, &'static str> = BTreeMap::new();
    for f in all {
        let Some(rest) = f.strip_prefix(&prefix) else {
            continue;
        };
        // A nested repository is listed as "dir/": no child to show.
        if rest.is_empty() {
            continue;
        }
        match rest.split_once('/') {
            Some((d, _)) => kids.insert(d, "dir"),
            None => kids.insert(rest, "file"),
        };
    }
    let entry = |(n, k): (&&str, &&'static str)| Entry {
        path: format!("{prefix}{n}"),
        kind: k,
    };
    let mut out: Vec<Entry> = kids
        .iter()
        .filter(|(_, k)| **k == "dir")
        .map(entry)
        .collect();
    out.extend(kids.iter().filter(|(_, k)| **k == "file").map(entry));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    fn tmp(tag: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("ig-ft-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d.canonicalize().unwrap()
    }

    fn git(dir: &Path, args: &[&str]) {
        assert!(
            Command::new("git")
                .current_dir(dir)
                .args(args)
                .status()
                .unwrap()
                .success()
        );
    }

    #[test]
    fn the_monorepo_lists_non_graph_files_and_skips_gitignored_ones() {
        let d = tmp("mono");
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../ig/tests/fixtures/monorepo/.");
        assert!(
            Command::new("cp")
                .arg("-R")
                .arg(fixture)
                .arg(&d)
                .status()
                .unwrap()
                .success()
        );
        std::fs::write(d.join("README.md"), "# hi\n").unwrap();
        std::fs::write(d.join(".gitignore"), "secret.txt\ndist/\n").unwrap();
        std::fs::write(d.join("secret.txt"), "x").unwrap();
        std::fs::create_dir_all(d.join("dist")).unwrap();
        std::fs::write(d.join("dist/a.js"), "x").unwrap();
        git(&d, &["init", "-q"]);
        let root = file_tree_for(&d, None);
        let paths: Vec<&str> = root.iter().map(|e| e.path.as_str()).collect();
        assert!(
            paths.contains(&"ig.yaml") && paths.contains(&"README.md"),
            "{paths:?}"
        );
        assert!(
            !paths.contains(&"secret.txt") && !paths.contains(&"dist"),
            "{paths:?}"
        );
        // directories first, then files, each alphabetical
        let first_file = root.iter().position(|e| e.kind == "file").unwrap();
        assert!(root[..first_file].iter().all(|e| e.kind == "dir"));
        assert!(
            root.windows(2)
                .all(|w| w[0].kind != w[1].kind || w[0].path <= w[1].path)
        );
        let apps = file_tree_for(&d, Some("apps"));
        assert!(!apps.is_empty() && apps.iter().all(|e| e.path.starts_with("apps/")));
    }

    #[test]
    fn empty_dir_is_the_root_nested_repos_have_no_empty_child_and_deleted_files_are_gone() {
        let d = tmp("edge");
        git(&d, &["init", "-q"]);
        std::fs::write(d.join("gone.txt"), "x").unwrap();
        std::fs::write(d.join("kept.txt"), "x").unwrap();
        git(&d, &["add", "."]);
        git(
            &d,
            &[
                "-c",
                "user.name=t",
                "-c",
                "user.email=t@t",
                "commit",
                "-qm",
                "c",
            ],
        );
        std::fs::remove_file(d.join("gone.txt")).unwrap();
        std::fs::create_dir_all(d.join("sub")).unwrap();
        git(&d.join("sub"), &["init", "-q"]);
        std::fs::write(d.join("sub/f.txt"), "x").unwrap();
        assert_eq!(file_tree_for(&d, Some("")), file_tree_for(&d, None));
        let root: Vec<_> = file_tree_for(&d, None)
            .into_iter()
            .map(|e| e.path)
            .collect();
        assert_eq!(root, ["sub", "kept.txt"]);
        assert!(file_tree_for(&d, Some("sub")).is_empty());
    }

    #[test]
    fn outside_git_the_walk_skips_git_and_node_modules() {
        let d = tmp("walk");
        for p in ["a/b.txt", "node_modules/x/y.js", ".git/config", "c.txt"] {
            std::fs::create_dir_all(d.join(p).parent().unwrap()).unwrap();
            std::fs::write(d.join(p), "x").unwrap();
        }
        let paths: Vec<_> = file_tree_for(&d, None)
            .into_iter()
            .map(|e| e.path)
            .collect();
        assert_eq!(paths, ["a", "c.txt"]);
    }

    #[test]
    fn twenty_thousand_paths_group_fast() {
        let all: Vec<String> = (0..20_000)
            .map(|i| format!("d{}/f{i}.txt", i % 200))
            .collect();
        let t = std::time::Instant::now();
        let root = children(&all, None);
        let took = t.elapsed();
        assert_eq!(root.len(), 200);
        assert!(took.as_millis() < 200, "{took:?}");
    }

    #[test]
    fn git_end_to_end_stays_under_two_seconds() {
        let d = tmp("perf");
        git(&d, &["init", "-q"]);
        for i in 0..2_000 {
            let dir = d.join(format!("d{}", i % 20));
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join(format!("f{i}.txt")), "").unwrap();
        }
        // untracked files (no `git add`) keep the setup cheap; the timing covers the command only.
        let t = std::time::Instant::now();
        let root = file_tree_for(&d, None);
        let took = t.elapsed();
        assert_eq!(root.len(), 20);
        assert!(took.as_secs() < 2, "{took:?}");
    }
}
