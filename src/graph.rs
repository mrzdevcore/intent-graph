//! Two-pass graph build: pass 1 extracts per file, pass 2 resolves edges against the global index.

use crate::config::Config;
use crate::extract::{self, Callee, ExportTarget, FileFacts, FileInput, ImportName};
use crate::model::{Edge, Graph, Node, ParseError, TOOL_VERSION};
use crate::patterns;
use crate::workspace::{self, Workspace};
use anyhow::Result;
use oxc_resolver::{
    ResolveOptions, Resolver, TsconfigDiscovery, TsconfigOptions, TsconfigReferences,
};
use rayon::prelude::*;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};

fn resolver_options(tsconfig: Option<PathBuf>) -> ResolveOptions {
    let exts = [".ts", ".tsx", ".js", ".jsx", ".mts", ".cts", ".mjs", ".cjs"];
    ResolveOptions {
        tsconfig: tsconfig.map(|config_file| {
            TsconfigDiscovery::Manual(TsconfigOptions {
                config_file,
                references: TsconfigReferences::Auto,
            })
        }),
        extensions: exts.iter().map(|s| s.to_string()).collect(),
        extension_alias: vec![
            (
                ".js".into(),
                vec![".ts".into(), ".tsx".into(), ".js".into(), ".jsx".into()],
            ),
            (".mjs".into(), vec![".mts".into(), ".mjs".into()]),
            (".cjs".into(), vec![".cts".into(), ".cjs".into()]),
        ],
        condition_names: vec![
            "types".into(),
            "import".into(),
            "require".into(),
            "node".into(),
            "default".into(),
        ],
        main_files: vec!["index".into()],
        ..ResolveOptions::default()
    }
}

/// The tsconfig to configure resolution from: a solution-style config (no paths/baseUrl, only
/// references) defers to its first reference.
fn effective_tsconfig(path: &Path) -> PathBuf {
    let Some(v) = workspace::read_jsonc(path) else {
        return path.to_path_buf();
    };
    let co = v.get("compilerOptions");
    let has_paths = co.is_some_and(|c| c.get("paths").is_some() || c.get("baseUrl").is_some());
    if !has_paths
        && let Some(r) = v
            .get("references")
            .and_then(|r| r.get(0))
            .and_then(|r| r.get("path"))
            .and_then(|p| p.as_str())
    {
        let p = path.parent().unwrap_or(Path::new(".")).join(r);
        let p = if p.is_dir() {
            p.join("tsconfig.json")
        } else {
            p
        };
        if p.is_file() {
            return p;
        }
    }
    path.to_path_buf()
}

pub struct Built {
    pub graph: Graph,
    /// Pattern-matching source per file (extracted script for embedded files); only kept when asked.
    pub workspace: Workspace,
}

/// Builds the graph of the tree at `root` (which may be a worktree).
pub fn build(root: &Path, config: &Config, commit: Option<String>) -> Result<Built> {
    let ws = Workspace::discover(root, config)?;
    let tests = workspace::globset(&config.tests)?;
    let flattened = crate::tsconfig::Flattened::new();
    let mut tsconfig_errors = vec![];
    let resolvers: Vec<Resolver> = ws
        .packages
        .iter()
        .enumerate()
        .map(|(i, p)| {
            // Separate resolvers, not `clone_with_options`: a manual tsconfig is cached in the
            // shared cache, so clones would all use whichever package's tsconfig loaded first.
            let Some(ts) = p.tsconfig.as_deref().map(effective_tsconfig) else {
                return Resolver::new(resolver_options(None));
            };
            let (flat, warnings) = flattened.flatten(i, &ts, &ws);
            for (file, message) in warnings {
                let file =
                    workspace::rel(&ws.root, &file).unwrap_or_else(|| file.display().to_string());
                tsconfig_errors.push(ParseError { file, message });
            }
            Resolver::new(resolver_options(flat))
        })
        .collect();
    let file_set: HashSet<&str> = ws.files.iter().map(|f| f.path.as_str()).collect();
    let compiled = patterns::compile(&config.patterns);
    let mut parse_errors = tsconfig_errors;

    // Pass 1.
    let mut facts: Vec<FileFacts> = ws
        .files
        .par_iter()
        .map(|f| {
            let abs = ws.root.join(&f.path);
            let source = std::fs::read(&abs)
                .map(|b| String::from_utf8_lossy(&b).into_owned())
                .unwrap_or_default();
            let embedded = config.embedded.iter().any(|e| {
                f.path
                    .to_ascii_lowercase()
                    .ends_with(&e.to_ascii_lowercase())
            });
            let pkg = &ws.packages[f.package];
            let input = FileInput {
                path: &f.path,
                source: &source,
                package: Some(&pkg.name),
                embedded,
                is_test: extract::is_test_file(&f.path, &tests),
                test_calls: &config.test_calls,
            };
            let dir = abs.parent().unwrap_or(&ws.root).to_path_buf();
            let resolve = |spec: &str| -> Option<String> {
                if !spec.starts_with('.')
                    && !spec.starts_with('/')
                    && let Some(t) = ws.resolve_workspace_import(spec)
                {
                    return file_set.contains(t.as_str()).then_some(t);
                }
                if spec.starts_with('.')
                    && config.embedded.iter().any(|e| spec.ends_with(e.as_str()))
                {
                    let dir_rel = f.path.rsplit_once('/').map(|(d, _)| d).unwrap_or(".");
                    let t = workspace::normalize(&workspace::join(dir_rel, spec));
                    return file_set.contains(t.as_str()).then_some(t);
                }
                let r = resolvers[f.package].resolve(&dir, spec).ok()?;
                let t = workspace::rel(&ws.root, &r.full_path())?;
                file_set.contains(t.as_str()).then_some(t)
            };
            let mut facts = extract::extract(&input, &resolve);
            let code = if embedded {
                facts.script.clone()
            } else {
                source
            };
            patterns::apply(&compiled, &f.path, &code, &mut facts);
            facts
        })
        .collect();
    facts.sort_by(|a, b| a.file.cmp(&b.file));
    for (i, msg) in &compiled.errors {
        parse_errors.push(ParseError {
            file: "ig.yaml".into(),
            message: format!("patterns[{i}]: {msg}"),
        });
    }

    // Pass 2.
    let index = Index::new(&facts);
    let edge_lists: Vec<Vec<Edge>> = facts.par_iter().map(|f| index.edges_of(f)).collect();

    let mut nodes: Vec<Node> = vec![];
    for f in &mut facts {
        if let Some(m) = &f.parse_error {
            parse_errors.push(ParseError {
                file: f.file.clone(),
                message: m.clone(),
            });
        }
        nodes.append(&mut f.nodes);
    }
    nodes.sort_by(|a, b| a.id.cmp(&b.id));
    let known: HashSet<&str> = nodes.iter().map(|n| n.id.as_str()).collect();
    let mut merged: BTreeMap<(String, String, String), Edge> = BTreeMap::new();
    for e in edge_lists.into_iter().flatten() {
        if !known.contains(e.to.as_str()) || !known.contains(e.from.as_str()) {
            continue;
        }
        let key = (e.from.clone(), e.kind.clone(), e.to.clone());
        match merged.get_mut(&key) {
            Some(x) => {
                x.namespace |= e.namespace;
                x.reexport |= e.reexport;
                if e.resolved() {
                    x.confidence = e.confidence.clone();
                }
                if (e.file.as_str(), e.line) < (x.file.as_str(), x.line) {
                    x.file = e.file;
                    x.line = e.line;
                }
            }
            None => {
                merged.insert(key, e);
            }
        }
    }
    let edges = merged.into_values().collect();
    parse_errors.sort_by(|a, b| (&a.file, &a.message).cmp(&(&b.file, &b.message)));
    parse_errors.dedup();
    let graph = Graph {
        schema_version: 1,
        tool_version: TOOL_VERSION.into(),
        commit,
        nodes,
        edges,
        parse_errors,
    };
    Ok(Built {
        graph,
        workspace: ws,
    })
}

struct Index<'f> {
    files: HashMap<&'f str, &'f FileFacts>,
    /// (parent id, name) → method node id
    children: HashMap<(&'f str, &'f str), &'f str>,
    /// bare name → unique non-module, non-test node id (None when ambiguous)
    by_name: HashMap<&'f str, Option<&'f str>>,
    /// class id → (defining file, extends callee)
    classes: HashMap<&'f str, (&'f FileFacts, Option<&'f Callee>)>,
}

impl<'f> Index<'f> {
    fn new(facts: &'f [FileFacts]) -> Index<'f> {
        let mut ix = Index {
            files: HashMap::new(),
            children: HashMap::new(),
            by_name: HashMap::new(),
            classes: HashMap::new(),
        };
        for f in facts {
            ix.files.insert(&f.file, f);
            for n in &f.nodes {
                if let Some(p) = &n.parent
                    && n.kind == "method"
                {
                    ix.children
                        .entry((p.as_str(), n.name.as_str()))
                        .or_insert(&n.id);
                }
                if !n.is_module() && n.kind != "test" {
                    ix.by_name
                        .entry(&n.name)
                        .and_modify(|v| *v = None)
                        .or_insert(Some(n.id.as_str()));
                }
            }
            for (id, ext) in &f.classes {
                ix.classes.insert(id, (f, ext.as_ref()));
            }
        }
        ix
    }

    fn export(
        &self,
        file: &str,
        name: &ImportName,
        seen: &mut HashSet<(String, String)>,
    ) -> Option<String> {
        let key = match name {
            ImportName::Named(n) => n.as_str(),
            ImportName::Default => "default",
            ImportName::Namespace => return Some(file.to_string()),
        };
        if !seen.insert((file.to_string(), key.to_string())) {
            return None;
        }
        let f = self.files.get(file)?;
        if let Some(t) = f.exports.names.get(key) {
            return match t {
                ExportTarget::Node(id) => Some(id.clone()),
                ExportTarget::Namespace(spec) => f.resolved.get(spec).cloned(),
                ExportTarget::Reexport(spec, n) => self.export(f.resolved.get(spec)?, n, seen),
            };
        }
        if key == "default" {
            // A single-file component without an explicit default export is the component itself.
            return (f.nodes.first().is_some_and(|m| m.kind == "component"))
                .then(|| file.to_string());
        }
        f.exports
            .stars
            .iter()
            .find_map(|spec| self.export(f.resolved.get(spec)?, name, seen))
    }

    fn import(&self, f: &FileFacts, spec: &str, name: &ImportName) -> Option<String> {
        let target = f.resolved.get(spec)?;
        self.export(target, name, &mut HashSet::new())
    }

    fn base_of(&self, class: &str) -> Option<String> {
        let (f, ext) = self.classes.get(class)?;
        match ext.as_ref()? {
            Callee::Node(id) => Some(id.clone()),
            Callee::Import(spec, n) => self.import(f, spec, n),
            _ => None,
        }
    }

    fn method(&self, class: &str, name: &str, skip_self: bool) -> Option<String> {
        let mut seen = HashSet::new();
        let mut cur = if skip_self {
            self.base_of(class)?
        } else {
            class.to_string()
        };
        loop {
            if !seen.insert(cur.clone()) {
                return None;
            }
            if let Some(m) = self.children.get(&(cur.as_str(), name)) {
                return Some(m.to_string());
            }
            cur = self.base_of(&cur)?;
        }
    }

    fn edges_of(&self, f: &FileFacts) -> Vec<Edge> {
        let mut out = vec![];
        let module = &f.file;
        for i in &f.imports {
            if let Some(t) = f.resolved.get(&i.spec) {
                out.push(Edge {
                    from: module.clone(),
                    to: t.clone(),
                    kind: "imports".into(),
                    confidence: "resolved".into(),
                    file: f.file.clone(),
                    line: i.line,
                    namespace: i.namespace,
                    reexport: i.reexport,
                });
            }
        }
        for r in &f.refs {
            let (to, conf) = match &r.callee {
                Callee::Node(id) => (Some(id.clone()), "resolved"),
                Callee::Import(spec, n) => (self.import(f, spec, n), "resolved"),
                Callee::ImportMember(spec, n, prop) => match self.import(f, spec, n) {
                    Some(t) if self.files.contains_key(t.as_str()) => (
                        self.export(&t, &ImportName::named(prop), &mut HashSet::new()),
                        "resolved",
                    ),
                    _ => (
                        self.by_name
                            .get(prop.as_str())
                            .copied()
                            .flatten()
                            .map(str::to_string),
                        "guessed",
                    ),
                },
                Callee::Member {
                    class,
                    name,
                    skip_self,
                } => (self.method(class, name, *skip_self), "resolved"),
                Callee::Guess(name) => (
                    self.by_name
                        .get(name.as_str())
                        .copied()
                        .flatten()
                        .map(str::to_string),
                    "guessed",
                ),
            };
            if let Some(to) = to {
                out.push(Edge {
                    from: r.from.clone(),
                    to,
                    kind: r.kind.into(),
                    confidence: conf.into(),
                    file: f.file.clone(),
                    line: r.line,
                    namespace: false,
                    reexport: false,
                });
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Each package resolves with its own tsconfig (a shared resolver cache once made every package
    /// use whichever tsconfig loaded first).
    #[test]
    fn per_package_tsconfig_paths() {
        let dir = std::env::temp_dir().join(format!("ig-paths-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let w = |p: &str, s: &str| {
            let f = dir.join(p);
            std::fs::create_dir_all(f.parent().unwrap()).unwrap();
            std::fs::write(f, s).unwrap();
        };
        w("pnpm-workspace.yaml", "packages: ['pkgs/*']\n");
        for (p, alias) in [("a", "@x"), ("b", "@y")] {
            w(
                &format!("pkgs/{p}/package.json"),
                &format!(r#"{{"name":"{p}"}}"#),
            );
            w(
                &format!("pkgs/{p}/tsconfig.json"),
                &format!(
                    r#"{{"extends":"./.nuxt/missing.json","compilerOptions":{{"paths":{{"{alias}/*":["./src/*"]}}}}}}"#
                ),
            );
            w(&format!("pkgs/{p}/src/util.ts"), "export const u = 1;\n");
            w(
                &format!("pkgs/{p}/src/main.ts"),
                &format!("import {{ u }} from '{alias}/util';\n"),
            );
        }
        let g = build(&dir, &Config::default(), None).unwrap().graph;
        for p in ["a", "b"] {
            let from = format!("pkgs/{p}/src/main.ts");
            assert!(
                g.edges
                    .iter()
                    .any(|e| e.from == from && e.to == format!("pkgs/{p}/src/util.ts")),
                "{p} alias unresolved"
            );
        }
        assert_eq!(g.parse_errors.len(), 2, "{:?}", g.parse_errors);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
