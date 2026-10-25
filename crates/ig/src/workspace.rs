//! Package discovery, source file discovery and workspace import resolution.

use crate::config::Config;
use anyhow::Result;
use serde_json::Value;
use std::collections::BTreeSet;
use std::io::Read;
use std::path::{Path, PathBuf};

pub use ig_core::workspace::{Package, SourceFile, glob, globset, is_source_ext, join};

pub struct Workspace {
    pub root: PathBuf,
    pub packages: Vec<Package>,
    pub files: Vec<SourceFile>,
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

fn find_package_dirs(root: &Path, patterns: &[String]) -> Result<Vec<String>> {
    let clean = |p: &str| p.trim_start_matches("./").trim_end_matches('/').to_string();
    let (neg, pos): (Vec<_>, Vec<_>) = patterns.iter().partition(|p| p.starts_with('!'));
    let pos: Vec<String> = pos.iter().map(|p| clean(p)).collect();
    let neg: Vec<String> = neg.iter().map(|p| clean(&p[1..])).collect();
    let include = globset(&pos)?;
    let exclude = globset(&neg)?;
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

impl Workspace {
    pub fn discover(root: &Path, config: &Config) -> Result<Workspace> {
        let root = root.canonicalize()?;
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
        Ok(Workspace {
            root,
            packages,
            files,
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

fn discover_files(root: &Path, packages: &[Package], config: &Config) -> Result<Vec<SourceFile>> {
    let ignore = globset(&config.ignore)?;
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
                    if name == "node_modules" || name == "dist" || name.starts_with('.') {
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

    #[test]
    fn jsonc_and_normalize() {
        assert_eq!(
            strip_trailing_commas("{\"a\": [1, 2,], \"b\": \",]\",}"),
            "{\"a\": [1, 2], \"b\": \",]\"}"
        );
        assert_eq!(normalize("a/./b/../c"), "a/c");
    }
}
