//! Package and source file discovery.

use crate::read_jsonc;
use ig_core::config::Config;
use ig_core::ports::PortError;
use ig_core::workspace::{
    Package, SourceFile, WorkspaceInfo, globset, is_source_ext, join, normalize, rel,
};
use serde_json::Value;
use std::collections::BTreeSet;
use std::path::Path;

fn io(e: impl std::fmt::Display) -> PortError {
    PortError::Io(e.to_string())
}

fn workspace_patterns(root: &Path) -> Option<Vec<String>> {
    if let Ok(text) = std::fs::read_to_string(root.join("pnpm-workspace.yaml")) {
        let v: serde_yaml_ng::Value = serde_yaml_ng::from_str(&text).ok()?;
        let pkgs = v.get("packages")?.as_sequence()?;
        return Some(
            pkgs.iter()
                .filter_map(|x| x.as_str().map(str::to_string))
                .collect(),
        );
    }
    let pj = read_jsonc(&root.join("package.json"))?;
    let ws = pj.get("workspaces")?;
    let arr = ws
        .as_array()
        .or_else(|| ws.get("packages").and_then(Value::as_array))?;
    Some(
        arr.iter()
            .filter_map(|x| x.as_str().map(str::to_string))
            .collect(),
    )
}

fn find_package_dirs(root: &Path, patterns: &[String]) -> Result<Vec<String>, PortError> {
    let clean = |p: &str| p.trim_start_matches("./").trim_end_matches('/').to_string();
    let (neg, pos): (Vec<_>, Vec<_>) = patterns.iter().partition(|p| p.starts_with('!'));
    let pos: Vec<String> = pos.iter().map(|p| clean(p)).collect();
    let neg: Vec<String> = neg.iter().map(|p| clean(&p[1..])).collect();
    let include = globset(&pos).map_err(io)?;
    let exclude = globset(&neg).map_err(io)?;
    let unbounded = pos.iter().any(|p| p.contains("**"));
    let max_depth = pos.iter().map(|p| p.split('/').count()).max().unwrap_or(0);
    let mut out = vec![];
    let mut stack = vec![(root.to_path_buf(), 0usize)];
    while let Some((dir, depth)) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&dir) else {
            continue;
        };
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if name == "node_modules"
                || name.starts_with('.')
                || !e.file_type().map(|t| t.is_dir()).unwrap_or(false)
            {
                continue;
            }
            let p = e.path();
            let r = rel(root, &p).unwrap_or_default();
            if include.is_match(&r) && !exclude.is_match(&r) && p.join("package.json").is_file() {
                out.push(r);
            }
            if unbounded || depth + 1 < max_depth {
                stack.push((p, depth + 1));
            }
        }
    }
    out.sort();
    Ok(out)
}

fn first_string(v: &Value) -> Option<String> {
    match v {
        Value::String(s) => Some(s.clone()),
        Value::Object(m) => m.values().find_map(first_string),
        Value::Array(a) => a.iter().find_map(first_string),
        _ => None,
    }
}

fn source_entry(root: &Path, dir: &str, pj: &Value, tsconfig: Option<&Value>) -> Option<String> {
    let exports = pj.get("exports");
    let raw = match exports {
        Some(Value::String(s)) => Some(s.clone()),
        Some(Value::Object(m)) => match m.get(".") {
            Some(dot) => first_string(dot),
            None if !m.keys().any(|k| k.starts_with('.')) => first_string(exports.unwrap()),
            None => None,
        },
        _ => None,
    }
    .or_else(|| pj.get("main").and_then(Value::as_str).map(str::to_string));
    let exists = |p: &str| root.join(p).is_file();
    let fallback = join(dir, "src/index.ts");
    let Some(raw) = raw else {
        return exists(&fallback).then_some(fallback);
    };
    let mut p = normalize(raw.trim_start_matches("./"));
    let co = tsconfig.and_then(|t| t.get("compilerOptions"));
    let out_dir = co
        .and_then(|c| c.get("outDir"))
        .and_then(Value::as_str)
        .map(normalize);
    let root_dir = co
        .and_then(|c| c.get("rootDir"))
        .and_then(Value::as_str)
        .map(normalize);
    let full = join(dir, &p);
    let is_source = [".ts", ".tsx", ".js", ".jsx", ".mjs", ".cjs"]
        .iter()
        .any(|e| p.ends_with(e))
        && !p.ends_with(".d.ts");
    if is_source
        && exists(&full)
        && out_dir
            .as_deref()
            .is_none_or(|o| !(p.starts_with(&format!("{o}/"))))
    {
        return Some(full);
    }
    if let (Some(o), Some(r)) = (out_dir, root_dir) {
        if let Some(rest) = p.strip_prefix(&format!("{o}/")) {
            p = if r == "." {
                rest.to_string()
            } else {
                format!("{r}/{rest}")
            };
        }
        let stem = [".d.ts", ".js", ".mjs", ".cjs", ".jsx", ".ts", ".tsx"]
            .iter()
            .find_map(|e| p.strip_suffix(e))
            .unwrap_or(&p)
            .to_string();
        for ext in [".ts", ".tsx"] {
            let cand = join(dir, &format!("{stem}{ext}"));
            if exists(&cand) {
                return Some(cand);
            }
        }
    }
    exists(&fallback).then_some(fallback)
}

/// Packages (workspace patterns of `pnpm-workspace.yaml` or `package.json`, else the root) and
/// their source files, under the canonical `root`.
pub fn discover(root: &Path, config: &Config) -> Result<WorkspaceInfo, PortError> {
    let root = root.canonicalize().map_err(io)?;
    let mut dirs = match workspace_patterns(&root) {
        Some(p) => find_package_dirs(&root, &p)?,
        None => vec![],
    };
    if dirs.is_empty() {
        dirs.push(".".into());
    }
    let mut packages = vec![];
    for dir in &dirs {
        let abs = root.join(dir);
        let pj = read_jsonc(&abs.join("package.json")).unwrap_or(Value::Null);
        let name = pj
            .get("name")
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| {
                abs.file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| ".".into())
            });
        let ts_path = abs.join("tsconfig.json");
        let ts = ts_path.is_file().then_some(ts_path);
        let ts_val = ts.as_deref().and_then(read_jsonc);
        let source_entry = source_entry(&root, dir, &pj, ts_val.as_ref());
        packages.push(Package {
            name,
            dir: dir.clone(),
            tsconfig: ts,
            source_entry,
        });
    }
    let files = discover_files(&root, &packages, config)?;
    Ok(WorkspaceInfo {
        root,
        packages,
        files,
    })
}

/// Build output and dependencies, never source, at any depth (`.`-folders are skipped too).
const BUILD_DIRS: [&str; 15] = [
    "target",
    "dist",
    "build",
    "out",
    ".next",
    ".nuxt",
    ".svelte-kit",
    ".turbo",
    ".vercel",
    ".output",
    "coverage",
    "node_modules",
    "vendor",
    ".git",
    ".ig",
];

/// Files git would list (tracked or untracked, minus `.gitignore`d); None outside a git repo.
fn git_visible(root: &Path) -> Option<BTreeSet<String>> {
    let out = std::process::Command::new("git")
        .args([
            "ls-files",
            "-z",
            "--cached",
            "--others",
            "--exclude-standard",
        ])
        .current_dir(root)
        .output()
        .ok()
        .filter(|o| o.status.success())?;
    Some(
        String::from_utf8_lossy(&out.stdout)
            .split('\0')
            .filter(|f| !f.is_empty())
            .map(String::from)
            .collect(),
    )
}

fn discover_files(
    root: &Path,
    packages: &[Package],
    config: &Config,
) -> Result<Vec<SourceFile>, PortError> {
    let ignore = globset(&config.ignore).map_err(io)?;
    let pkg_dirs: BTreeSet<&str> = packages.iter().map(|p| p.dir.as_str()).collect();
    let tracked = git_visible(root);
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
                    if BUILD_DIRS.contains(&name.as_str()) || name.starts_with('.') {
                        continue;
                    }
                    if pkg_dirs.contains(r.as_str()) || p.join("package.json").is_file() {
                        continue;
                    }
                    if ignore.is_match(&r) {
                        continue;
                    }
                    stack.push(p);
                } else if ft.is_file()
                    && is_source_ext(&name, &config.embedded)
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
        std::fs::write(d.join(".gitignore"), "generated/\n").unwrap();
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

    fn paths(d: &Path) -> Vec<String> {
        let ws = discover(d, &Config::default()).unwrap();
        ws.files.into_iter().map(|f| f.path).collect()
    }

    #[test]
    fn skips_build_output() {
        let d = project("git", true);
        assert_eq!(paths(&d), ["src/a.ts"]);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn skips_build_output_without_git() {
        let d = project("nogit", false);
        assert_eq!(paths(&d), ["generated/g.ts", "src/a.ts"]); // src/build/ is build output too
        let _ = std::fs::remove_dir_all(&d);
    }
}
