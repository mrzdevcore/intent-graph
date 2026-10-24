//! Tolerant tsconfig loading. Only `baseUrl` and `paths` matter for resolution, so `ig` follows the
//! `extends` chain itself: a missing link (an uninstalled package, a not-yet-generated `.nuxt/`) is
//! skipped with a warning instead of failing every resolution in the package. Workspace packages are
//! found by name, so `extends: "@acme/tsconfig/base.json"` works without `node_modules`.

use crate::workspace::{self, Workspace};
use serde_json::{Map, Value, json};
use std::path::{Path, PathBuf};

/// Removes `.` and `..` without touching the filesystem, so `.nuxt/..` stays put even when `.nuxt`
/// is a symlink (as in worktrees).
fn lexical(p: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                out.pop();
            }
            c => out.push(c),
        }
    }
    out
}

#[derive(Default)]
struct Flat {
    base_url: Option<PathBuf>,
    paths: Vec<(String, Vec<PathBuf>)>,
}

fn resolve_extends(from_dir: &Path, spec: &str, ws: &Workspace) -> Option<PathBuf> {
    let file_or_json = |p: PathBuf| -> Option<PathBuf> {
        if p.is_file() {
            return Some(p);
        }
        let j = PathBuf::from(format!("{}.json", p.display()));
        if j.is_file() {
            return Some(j);
        }
        let t = p.join("tsconfig.json");
        t.is_file().then_some(t)
    };
    if spec.starts_with('.') || spec.starts_with('/') {
        return file_or_json(from_dir.join(spec));
    }
    if let Some(p) = ws
        .packages
        .iter()
        .filter(|p| spec == p.name || spec.starts_with(&format!("{}/", p.name)))
        .max_by_key(|p| p.name.len())
    {
        let sub = spec[p.name.len()..].trim_start_matches('/');
        if let Some(f) = file_or_json(ws.root.join(&p.dir).join(sub)) {
            return Some(f);
        }
    }
    let mut dir = Some(from_dir);
    while let Some(d) = dir {
        if let Some(f) = file_or_json(d.join("node_modules").join(spec)) {
            return Some(f);
        }
        dir = d.parent();
    }
    None
}

fn load(
    path: &Path,
    ws: &Workspace,
    seen: &mut Vec<PathBuf>,
    warnings: &mut Vec<(PathBuf, String)>,
) -> Flat {
    let mut flat = Flat::default();
    if seen.contains(&path.to_path_buf()) || seen.len() > 32 {
        return flat;
    }
    seen.push(path.to_path_buf());
    let Some(v) = workspace::read_jsonc(path) else {
        warnings.push((
            path.to_path_buf(),
            "cannot be parsed; its paths are ignored".into(),
        ));
        return flat;
    };
    let dir = path.parent().unwrap_or(Path::new("."));
    let extends: Vec<&str> = match v.get("extends") {
        Some(Value::String(s)) => vec![s.as_str()],
        Some(Value::Array(a)) => a.iter().filter_map(Value::as_str).collect(),
        _ => vec![],
    };
    for e in extends {
        match resolve_extends(dir, e, ws) {
            Some(p) => {
                let parent = load(&p, ws, seen, warnings);
                if parent.base_url.is_some() {
                    flat.base_url = parent.base_url;
                }
                if !parent.paths.is_empty() {
                    flat.paths = parent.paths;
                }
            }
            None => warnings.push((
                path.to_path_buf(),
                format!("extends {e:?}, which is not found; its paths are ignored"),
            )),
        }
    }
    let co = v.get("compilerOptions");
    if let Some(b) = co.and_then(|c| c.get("baseUrl")).and_then(Value::as_str) {
        flat.base_url = Some(lexical(&dir.join(b)));
    }
    if let Some(Value::Object(paths)) = co.and_then(|c| c.get("paths")) {
        let rel_to = flat.base_url.clone().unwrap_or_else(|| dir.to_path_buf());
        flat.paths = paths
            .iter()
            .map(|(k, targets)| {
                let ts = targets
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .map(|t| lexical(&rel_to.join(t)))
                    .collect();
                (k.clone(), ts)
            })
            .collect();
    }
    flat
}

/// Temporary flattened tsconfigs, removed on drop.
pub struct Flattened {
    dir: PathBuf,
}

impl Drop for Flattened {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

static COUNTER: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

impl Flattened {
    pub fn new() -> Flattened {
        let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Flattened {
            dir: std::env::temp_dir().join(format!("ig-ts-{}-{n}", std::process::id())),
        }
    }

    /// Writes a flattened config for `path` and returns it (None when it has no baseUrl or paths),
    /// plus warnings as (tsconfig path, message).
    pub fn flatten(
        &self,
        idx: usize,
        path: &Path,
        ws: &Workspace,
    ) -> (Option<PathBuf>, Vec<(PathBuf, String)>) {
        let mut warnings = vec![];
        let flat = load(path, ws, &mut vec![], &mut warnings);
        if flat.base_url.is_none() && flat.paths.is_empty() {
            return (None, warnings);
        }
        let mut co = Map::new();
        if let Some(b) = &flat.base_url {
            co.insert("baseUrl".into(), json!(b));
        }
        let paths: Map<String, Value> = flat
            .paths
            .iter()
            .map(|(k, v)| (k.clone(), json!(v)))
            .collect();
        co.insert("paths".into(), Value::Object(paths));
        let out = self.dir.join(format!("{idx}.json"));
        let ok = std::fs::create_dir_all(&self.dir).is_ok()
            && std::fs::write(&out, json!({ "compilerOptions": co }).to_string()).is_ok();
        (ok.then_some(out), warnings)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    #[test]
    fn follows_workspace_extends_and_skips_missing() {
        let dir = std::env::temp_dir().join(format!("ig-tsconfig-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let w = |p: &str, s: &str| {
            let f = dir.join(p);
            std::fs::create_dir_all(f.parent().unwrap()).unwrap();
            std::fs::write(f, s).unwrap();
        };
        w(
            "pnpm-workspace.yaml",
            "packages: ['packages/*', 'apps/*']\n",
        );
        w(
            "packages/tsconfig/package.json",
            r#"{"name":"@acme/tsconfig"}"#,
        );
        w(
            "packages/tsconfig/base.json",
            r#"{"compilerOptions":{"baseUrl":".","paths":{"~/*":["./x/*"]}}}"#,
        );
        w("apps/web/package.json", r#"{"name":"web"}"#);
        w(
            "apps/web/tsconfig.json",
            r#"{"extends":["@acme/tsconfig/base.json","./.nuxt/tsconfig.json"],"compilerOptions":{"paths":{"@/*":["./src/*"]}}}"#,
        );
        let ws = Workspace::discover(&dir, &Config::default()).unwrap();
        let f = Flattened::new();
        let (out, warnings) = f.flatten(0, &ws.root.join("apps/web/tsconfig.json"), &ws);
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].1.contains(".nuxt/tsconfig.json"));
        let v: Value =
            serde_json::from_str(&std::fs::read_to_string(out.unwrap()).unwrap()).unwrap();
        let co = &v["compilerOptions"];
        assert!(
            co["baseUrl"]
                .as_str()
                .unwrap()
                .ends_with("packages/tsconfig")
        );
        // Child paths replace inherited ones and resolve against the inherited baseUrl.
        assert!(co["paths"]["~/*"].is_null());
        assert!(
            co["paths"]["@/*"][0]
                .as_str()
                .unwrap()
                .ends_with("packages/tsconfig/src/*")
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
