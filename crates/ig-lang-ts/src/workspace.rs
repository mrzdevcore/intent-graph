//! What a JavaScript tree looks like: its workspace packages (`pnpm-workspace.yaml` or
//! `package.json` workspaces, else the root `package.json`), a package's source entry, and which
//! files are source.

use crate::tsconfig::{parse_jsonc, read_jsonc};
use ig_core::config::Config;
use ig_core::ports::Files;
use ig_core::workspace::{Package, globset, join, normalize};
use serde_json::Value;
use std::path::Path;

/// TypeScript and JavaScript sources (not declaration files), and the `embedded` extensions.
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

fn workspace_patterns(files: &dyn Files) -> Option<Vec<String>> {
    if let Some(text) = files.read("pnpm-workspace.yaml") {
        let v: serde_yaml_ng::Value = serde_yaml_ng::from_str(&text).ok()?;
        let pkgs = v.get("packages")?.as_sequence()?;
        return Some(
            pkgs.iter()
                .filter_map(|x| x.as_str().map(str::to_string))
                .collect(),
        );
    }
    let pj = parse_jsonc(&files.read("package.json")?)?;
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

/// The workspace packages (folders the patterns match that hold a `package.json`, outside
/// `.`-folders), else the root's own `package.json`, else the outermost `package.json` folders a
/// few levels down; named by their `package.json`, else their folder.
pub fn packages(files: &dyn Files, config: &Config) -> Vec<Package> {
    let name = |dir: &str| {
        let pj = files
            .read(&join(dir, "package.json"))
            .and_then(|t| parse_jsonc(&t));
        pj.and_then(|v| v.get("name")?.as_str().map(str::to_string))
            .unwrap_or_else(|| dir.rsplit('/').next().unwrap_or(dir).to_string())
    };
    let clean = |p: &str| p.trim_start_matches("./").trim_end_matches('/').to_string();
    let declared = workspace_patterns(files);
    if declared.is_none() && !files.exists("package.json") {
        return nested(files, config)
            .into_iter()
            .map(|dir| Package {
                name: name(&dir),
                dir,
            })
            .collect();
    }
    let patterns = declared.unwrap_or_default();
    let (neg, pos): (Vec<_>, Vec<_>) = patterns.iter().partition(|p| p.starts_with('!'));
    let exclude = globset(&neg.iter().map(|p| clean(&p[1..])).collect::<Vec<_>>());
    let manifests: Vec<String> = pos
        .iter()
        .map(|p| format!("{}/package.json", clean(p)))
        .collect();
    let dirs: Vec<String> = match (files.glob(&manifests), exclude) {
        (Ok(found), Ok(exclude)) => found
            .iter()
            .filter_map(|m| m.strip_suffix("/package.json"))
            .filter(|d| !d.split('/').any(|s| s.starts_with('.')) && !exclude.is_match(d))
            .map(str::to_string)
            .collect(),
        _ => vec![],
    };
    if dirs.is_empty() {
        return match files.exists("package.json") {
            true => vec![Package {
                name: name("."),
                dir: ".".into(),
            }],
            false => vec![],
        };
    }
    dirs.into_iter()
        .map(|dir| Package {
            name: name(&dir),
            dir,
        })
        .collect()
}

const OUTPUT: [&str; 6] = ["dist", "build", "out", "coverage", "target", "vendor"];

fn nested(files: &dyn Files, config: &Config) -> Vec<String> {
    let ignore = globset(&config.ignore).ok();
    let ignored = |d: &str| {
        std::iter::successors(Some(d), |p| p.rsplit_once('/').map(|(d, _)| d))
            .any(|p| ignore.as_ref().is_some_and(|i| i.is_match(p)))
    };
    // ponytail: three levels covers web/, apps/web and services/x/client without walking build output
    let manifests: Vec<String> = ["*/", "*/*/", "*/*/*/"]
        .iter()
        .map(|d| format!("{d}package.json"))
        .collect();
    let mut dirs: Vec<String> = (files.glob(&manifests).unwrap_or_default().iter())
        .filter_map(|m| m.strip_suffix("/package.json"))
        .filter(|d| {
            !d.split('/')
                .any(|s| s.starts_with('.') || OUTPUT.contains(&s))
        })
        .filter(|d| !ignored(d))
        .map(str::to_string)
        .collect();
    dirs.sort_by_key(|d| d.matches('/').count());
    let mut out: Vec<String> = vec![];
    for d in dirs {
        if !out.iter().any(|o| d.starts_with(&format!("{o}/"))) {
            out.push(d);
        }
    }
    out.sort();
    out
}

fn first_string(v: &Value) -> Option<String> {
    match v {
        Value::String(s) => Some(s.clone()),
        Value::Object(m) => m.values().find_map(first_string),
        Value::Array(a) => a.iter().find_map(first_string),
        _ => None,
    }
}

/// The package at `dir`'s source entry: `exports["."]` (the first string in its conditions), else
/// `exports` as a string, else `main`, mapped from `outDir` back to `rootDir` when its tsconfig
/// sets both; else `src/index.ts`.
pub fn source_entry(root: &Path, dir: &str) -> Option<String> {
    let abs = root.join(dir);
    let pj = read_jsonc(&abs.join("package.json")).unwrap_or(Value::Null);
    let tsconfig = read_jsonc(&abs.join("tsconfig.json"));
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
    let co = tsconfig.as_ref().and_then(|t| t.get("compilerOptions"));
    let option = |k: &str| {
        co.and_then(|c| c.get(k))
            .and_then(Value::as_str)
            .map(normalize)
    };
    let (out_dir, root_dir) = (option("outDir"), option("rootDir"));
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

#[cfg(test)]
mod tests {
    use super::*;
    use ig_core::testing::MemFiles;

    #[test]
    fn workspace_packages_else_the_root() {
        let f = MemFiles::with(&[
            (
                "pnpm-workspace.yaml",
                "packages:\n  - apps/*\n  - '!apps/old'\n",
            ),
            ("package.json", r#"{"name": "mono"}"#),
            ("apps/web/package.json", r#"{"name": "@acme/web"}"#),
            ("apps/api/package.json", "{}"),
            ("apps/old/package.json", "{}"),
            ("apps/docs/readme.md", ""),
        ]);
        let got: Vec<(String, String)> = packages(&f, &Config::default())
            .into_iter()
            .map(|p| (p.dir, p.name))
            .collect();
        assert_eq!(
            got,
            [
                ("apps/api".into(), "api".into()),
                ("apps/web".into(), "@acme/web".into())
            ]
        );
        let one = MemFiles::with(&[("package.json", r#"{"name": "app"}"#)]);
        assert_eq!(
            packages(&one, &Config::default()),
            [Package {
                name: "app".into(),
                dir: ".".into()
            }]
        );
        let none = MemFiles::with(&[("main.py", "")]);
        assert_eq!(packages(&none, &Config::default()), []);
    }

    #[test]
    fn without_a_root_package_the_outermost_nested_ones() {
        let f = MemFiles::with(&[
            ("go.mod", "module shop\n"),
            ("web/package.json", r#"{"name": "@shop/web"}"#),
            ("web/packages/ui/package.json", "{}"),
            ("tools/lint/package.json", "{}"),
            ("services/x/client/package.json", "{}"),
            ("a/b/c/d/package.json", "{}"),
            (".github/action/package.json", "{}"),
            ("web/node_modules/vite/package.json", "{}"),
            ("dist/app/package.json", "{}"),
            ("generated/sdk/package.json", "{}"),
        ]);
        let config = Config {
            ignore: vec!["**/generated/**".into()],
            ..Config::default()
        };
        let got: Vec<(String, String)> = packages(&f, &config)
            .into_iter()
            .map(|p| (p.dir, p.name))
            .collect();
        assert_eq!(
            got,
            [
                ("services/x/client".into(), "client".into()),
                ("tools/lint".into(), "lint".into()),
                ("web".into(), "@shop/web".into()),
            ]
        );
        let with_root = MemFiles::with(&[("package.json", "{}"), ("web/package.json", "{}")]);
        let got = packages(&with_root, &Config::default());
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].dir, ".");
    }
}
