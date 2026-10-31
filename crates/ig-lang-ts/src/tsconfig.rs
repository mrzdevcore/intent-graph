//! Tolerant tsconfig loading. Only `baseUrl` and `paths` matter for resolution, so `ig` follows the
//! `extends` chain itself: a missing link (an uninstalled package, a not-yet-generated `.nuxt/`) is
//! skipped with a warning instead of failing every resolution in the package. Workspace packages are
//! found by name, so `extends: "@acme/tsconfig/base.json"` works without `node_modules`.

use ig_core::model::ParseError;
use ig_core::workspace::WorkspaceInfo;
use serde_json::{Map, Value, json};
use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};

/// Parses JSON with comments and trailing commas.
pub fn read_jsonc(path: &Path) -> Option<Value> {
    parse_jsonc(&std::fs::read_to_string(path).ok()?)
}

/// `text` as JSON with comments and trailing commas.
pub fn parse_jsonc(text: &str) -> Option<Value> {
    let mut stripped = String::new();
    json_comments::StripComments::new(text.as_bytes())
        .read_to_string(&mut stripped)
        .ok()?;
    serde_json::from_str(&ig_core::workspace::strip_trailing_commas(&stripped)).ok()
}

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

/// A problem with a tsconfig: about one file, or a package specifier in `extends` that is not
/// installed (reported once for every tsconfig that names it).
#[derive(Debug)]
pub enum Warning {
    File(PathBuf, String),
    MissingPackage(String, PathBuf),
}

/// The warnings as findings: one per file problem, one per missing package specifier (on the
/// first tsconfig naming it, with the count).
pub fn parse_errors(warnings: Vec<Warning>, root: &Path) -> Vec<ParseError> {
    let rel =
        |f: &Path| ig_core::workspace::rel(root, f).unwrap_or_else(|| f.display().to_string());
    let mut missing: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut out = vec![];
    for w in warnings {
        match w {
            Warning::File(f, message) => out.push(ParseError {
                file: rel(&f),
                message,
            }),
            Warning::MissingPackage(spec, f) => missing.entry(spec).or_default().push(rel(&f)),
        }
    }
    for (spec, mut files) in missing {
        files.sort();
        files.dedup();
        let n = files.len();
        out.push(ParseError {
            file: files.swap_remove(0),
            message: format!(
                "extends {spec:?}, which is not installed (no node_modules entry; install dependencies): the paths it would bring are ignored in {n} tsconfig{}",
                if n == 1 { "" } else { "s" }
            ),
        });
    }
    out
}

#[derive(Default)]
struct Flat {
    base_url: Option<PathBuf>,
    paths: Vec<(String, Vec<PathBuf>)>,
}

fn resolve_extends(from_dir: &Path, spec: &str, ws: &WorkspaceInfo) -> Option<PathBuf> {
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
    // Node resolution, as TypeScript does it: node_modules walking up; a bare package may name its
    // config in package.json "tsconfig" (ponytail: "exports" maps are not read).
    let mut dir = Some(from_dir);
    while let Some(d) = dir {
        let p = d.join("node_modules").join(spec);
        let field = read_jsonc(&p.join("package.json"))
            .and_then(|v| v.get("tsconfig")?.as_str().map(|t| lexical(&p.join(t))))
            .filter(|t| t.starts_with(lexical(&p)) && t.is_file());
        if let Some(f) = field.or_else(|| file_or_json(p)) {
            return Some(f);
        }
        dir = d.parent();
    }
    None
}

fn load(
    path: &Path,
    ws: &WorkspaceInfo,
    seen: &mut Vec<PathBuf>,
    warnings: &mut Vec<Warning>,
) -> Flat {
    let mut flat = Flat::default();
    if seen.contains(&path.to_path_buf()) || seen.len() > 32 {
        return flat;
    }
    seen.push(path.to_path_buf());
    let Some(v) = read_jsonc(path) else {
        warnings.push(Warning::File(
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
            None if !e.starts_with('.') && !e.starts_with('/') => {
                warnings.push(Warning::MissingPackage(e.to_string(), path.to_path_buf()))
            }
            None => warnings.push(Warning::File(
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
    /// plus warnings (`parse_errors` turns them into findings).
    pub fn flatten(
        &self,
        idx: usize,
        path: &Path,
        ws: &WorkspaceInfo,
    ) -> (Option<PathBuf>, Vec<Warning>) {
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
        let package = |name: &str, dir: &str| ig_core::workspace::Package {
            name: name.into(),
            dir: dir.into(),
        };
        let ws = WorkspaceInfo {
            root: dir.clone(),
            packages: vec![
                package("@acme/tsconfig", "packages/tsconfig"),
                package("web", "apps/web"),
            ],
            files: vec![],
        };
        let f = Flattened::new();
        let (out, warnings) = f.flatten(0, &ws.root.join("apps/web/tsconfig.json"), &ws);
        assert_eq!(warnings.len(), 1);
        assert!(matches!(&warnings[0], Warning::File(_, m) if m.contains(".nuxt/tsconfig.json")));
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

        // Package specifiers resolve through node_modules, walking up: a file in the package, the
        // package root's tsconfig.json, or its package.json "tsconfig" field.
        w(
            "node_modules/@tsconfig/node22/tsconfig.json",
            r#"{"compilerOptions":{"baseUrl":"./a"}}"#,
        );
        w(
            "node_modules/@acme/cfg/package.json",
            r#"{"name":"@acme/cfg","tsconfig":"./cfg/base.json"}"#,
        );
        w(
            "node_modules/@acme/cfg/cfg/base.json",
            r#"{"compilerOptions":{"baseUrl":"./b"}}"#,
        );
        for (name, ext) in [
            ("node1", "@tsconfig/node22/tsconfig.json"),
            ("node2", "@tsconfig/node22"),
            ("node3", "@acme/cfg"),
        ] {
            w(
                &format!("apps/web/{name}.json"),
                &format!(r#"{{"extends":"{ext}"}}"#),
            );
            let (out, warnings) = f.flatten(1, &ws.root.join(format!("apps/web/{name}.json")), &ws);
            assert!(warnings.is_empty(), "{ext}");
            assert!(out.is_some(), "{ext}");
        }

        // A "tsconfig" field pointing outside its package is not followed.
        w(
            "node_modules/@acme/escape/package.json",
            r#"{"tsconfig":"../../../apps/web/node1.json"}"#,
        );
        w("apps/web/escape.json", r#"{"extends":"@acme/escape"}"#);
        let (_, warnings_escape) = f.flatten(1, &ws.root.join("apps/web/escape.json"), &ws);
        assert!(
            matches!(&warnings_escape[..], [Warning::MissingPackage(s, _)] if s == "@acme/escape"),
            "{warnings_escape:?}"
        );

        // A package that is not installed: one finding per specifier, however many tsconfigs.
        let mut all = vec![];
        for app in ["a", "b", "c"] {
            w(
                &format!("micro/{app}/tsconfig.node.json"),
                r#"{"extends":"@tsconfig/node20/tsconfig.json"}"#,
            );
            all.extend(
                f.flatten(
                    2,
                    &ws.root.join(format!("micro/{app}/tsconfig.node.json")),
                    &ws,
                )
                .1,
            );
        }
        all.extend(warnings);
        let errors = parse_errors(all, &ws.root);
        assert_eq!(errors.len(), 2, "{errors:?}");
        let pkg = errors
            .iter()
            .find(|e| e.message.contains("node20"))
            .unwrap();
        assert_eq!(pkg.file, "micro/a/tsconfig.node.json");
        assert!(pkg.message.contains("3 tsconfigs"), "{}", pkg.message);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
