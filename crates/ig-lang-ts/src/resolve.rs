//! Pass 2 of the TS frontend: per-package oxc resolvers (tsconfig paths), export tables,
//! `autoImports` (free, auto and stale names, externals), typed members and implementers.

use crate::autoimports::AutoImports;
use crate::extract::{self, Callee, ExportTarget, FileFacts, FileInput, ImportName};
use crate::patterns;
use crate::tsconfig::read_jsonc;
use globset::GlobSet;
use ig_core::config::Config;
use ig_core::model::{Edge, ParseError};
use ig_core::ports::{self, FrontendOutput};
use ig_core::workspace::{self, SourceFile, WorkspaceInfo};
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
    let Some(v) = read_jsonc(path) else {
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

/// `pkg` or `pkg/sub` → a repo-relative file.
fn resolve_workspace_import(ws: &WorkspaceInfo, spec: &str) -> Option<String> {
    let pkg = ws
        .packages
        .iter()
        .filter(|p| spec == p.name || spec.starts_with(&format!("{}/", p.name)))
        .max_by_key(|p| p.name.len())?;
    let sub = spec[pkg.name.len()..].trim_start_matches('/');
    if !sub.is_empty() {
        for ext in [".ts", ".tsx"] {
            let cand = workspace::join(&pkg.dir, &format!("src/{sub}{ext}"));
            if ws.root.join(&cand).is_file() {
                return Some(cand);
            }
        }
    }
    pkg.source_entry.clone()
}

/// Extracts `files` (pass 1, in parallel) and resolves their edges against the global index
/// (pass 2). Edges are not yet merged nor filtered against known nodes.
pub fn build(
    files: &[(SourceFile, String)],
    ws: &WorkspaceInfo,
    config: &Config,
) -> FrontendOutput {
    // The app compiled `tests` before calling the frontend: it is valid here.
    let tests = workspace::globset(&config.tests).unwrap_or_else(|_| GlobSet::empty());
    let flattened = crate::tsconfig::Flattened::new();
    let mut tsconfig_warnings = vec![];
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
            let (flat, warnings) = flattened.flatten(i, &ts, ws);
            tsconfig_warnings.extend(warnings);
            Resolver::new(resolver_options(flat))
        })
        .collect();
    let file_set: HashSet<&str> = ws.files.iter().map(|f| f.path.as_str()).collect();
    let compiled = patterns::compile(&config.patterns);
    let mut parse_errors = crate::tsconfig::parse_errors(tsconfig_warnings, &ws.root);

    // Pass 1.
    let mut facts: Vec<FileFacts> = files
        .par_iter()
        .map(|(f, source)| {
            let abs = ws.root.join(&f.path);
            let embedded = config.embedded.iter().any(|e| {
                f.path
                    .to_ascii_lowercase()
                    .ends_with(&e.to_ascii_lowercase())
            });
            let pkg = &ws.packages[f.package];
            let input = FileInput {
                path: &f.path,
                source,
                package: Some(&pkg.name),
                embedded,
                is_test: extract::is_test_file(&f.path, &tests),
                test_calls: &config.test_calls,
                http_calls: &config.blueprint.http_calls,
            };
            let dir = abs.parent().unwrap_or(&ws.root).to_path_buf();
            let resolve = |spec: &str| -> Option<String> {
                if !spec.starts_with('.')
                    && !spec.starts_with('/')
                    && let Some(t) = resolve_workspace_import(ws, spec)
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
            let script = embedded.then(|| facts.script.clone());
            patterns::apply(
                &compiled,
                &f.path,
                script.as_deref().unwrap_or(source),
                &mut facts,
            );
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
    let (auto, auto_errors) = crate::autoimports::load(&ws.root, &config.auto_imports, ws);
    parse_errors.extend(auto_errors);
    let index = Index::new(&facts, &auto);
    let edge_lists: Vec<Vec<Edge>> = facts.par_iter().map(|f| index.edges_of(f)).collect();
    let mut stale: BTreeMap<&str, std::collections::BTreeSet<&str>> = BTreeMap::new();
    for r in facts
        .iter()
        .filter(|f| !tests.is_match(&f.file))
        .flat_map(|f| &f.refs)
    {
        if let Callee::Free(name) = &r.callee
            && let Some((_, decl)) = index.stale_hit(name)
        {
            stale.entry(decl).or_default().insert(name);
        }
    }
    for (decl, names) in stale {
        let list: Vec<&str> = names.iter().take(10).copied().collect();
        parse_errors.push(ParseError {
            file: decl.into(),
            message: format!(
                "autoImports: {} names not in {decl} resolved from its directories (run nuxt prepare): {}",
                names.len(),
                list.join(", ")
            ),
        });
    }
    let packages = facts
        .iter()
        .flat_map(|f| {
            f.packages
                .iter()
                .map(|(p, l)| (f.file.clone(), p.clone(), *l))
        })
        .collect();
    let facts = facts
        .into_iter()
        .map(|f| ports::FileFacts {
            file: f.file,
            nodes: f.nodes,
            parse_error: f.parse_error,
            http: f.http,
            url_consts: f.url_consts,
            url_callees: f.url_callees,
        })
        .collect();
    FrontendOutput {
        facts,
        edges: edge_lists.into_iter().flatten().collect(),
        parse_errors,
        packages,
        notices: vec![],
    }
}

struct Index<'f> {
    files: HashMap<&'f str, &'f FileFacts>,
    /// (parent id, name) → method node id
    children: HashMap<(&'f str, &'f str), &'f str>,
    /// bare name → unique non-module, non-test node id (None when ambiguous)
    by_name: HashMap<&'f str, Option<&'f str>>,
    /// class id → (defining file, extends callee)
    classes: HashMap<&'f str, (&'f FileFacts, Option<&'f Callee>)>,
    /// interface or class id → classes that implement or extend it
    implementers: HashMap<String, Vec<String>>,
    auto: &'f AutoImports,
    /// name → the unique exported node of that name in a dir an `autoImports` file covers, with
    /// that file (None when ambiguous): names a stale declaration file doesn't list yet.
    stale: HashMap<&'f str, Option<(&'f str, &'f str)>>,
}

impl<'f> Index<'f> {
    fn new(facts: &'f [FileFacts], auto: &'f AutoImports) -> Index<'f> {
        let mut ix = Index {
            files: HashMap::new(),
            children: HashMap::new(),
            by_name: HashMap::new(),
            classes: HashMap::new(),
            implementers: HashMap::new(),
            auto,
            stale: HashMap::new(),
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
                if !n.is_module()
                    && n.kind != "test"
                    && n.exported
                    && let Some(decl) =
                        auto.covering(n.file.rsplit_once('/').map_or("", |(d, _)| d))
                {
                    ix.stale
                        .entry(&n.name)
                        .and_modify(|v| *v = None)
                        .or_insert(Some((n.id.as_str(), decl)));
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
        let mut implementers: HashMap<String, Vec<String>> = HashMap::new();
        for f in facts {
            for r in f
                .refs
                .iter()
                .filter(|r| r.kind == "implements" || r.kind == "extends")
            {
                if let Some(t) = ix.target(f, &r.callee) {
                    implementers.entry(t).or_default().push(r.from.clone());
                }
            }
        }
        ix.implementers = implementers;
        ix
    }

    /// The id a non-member callee resolves to (possibly an interface or type id, which is no node).
    fn target(&self, f: &FileFacts, c: &Callee) -> Option<String> {
        match c {
            Callee::Node(id) => Some(id.clone()),
            Callee::Import(spec, n) => self.import(f, spec, n),
            _ => None,
        }
    }

    /// Method `m` of class `t` as a `calls` edge. Otherwise (an interface, or a base class
    /// lacking `m`) method `m` of the classes implementing or extending it (a `dispatch`
    /// edge: the code depends on the interface, so arch rules and review ignore it). Confidence is
    /// `guessed` when several implementations match. A class with neither: class `t` itself
    /// (`m` is a property).
    fn typed(
        &self,
        f: &FileFacts,
        t: &Callee,
        m: &str,
    ) -> Vec<(String, &'static str, &'static str)> {
        let Some(tid) = self.target(f, t) else {
            return vec![];
        };
        if self.classes.contains_key(tid.as_str())
            && let Some(to) = self.method(&tid, m, false)
        {
            return vec![(to, "calls", "resolved")];
        }
        let hits: Vec<String> = self
            .implementers
            .get(&tid)
            .into_iter()
            .flatten()
            .filter_map(|c| self.method(c, m, false))
            .collect();
        if hits.is_empty() && self.classes.contains_key(tid.as_str()) {
            // Not a method (a property like `this.db.knex`): the code still depends on the class.
            return vec![(tid, "calls", "resolved")];
        }
        let conf = if hits.len() == 1 {
            "resolved"
        } else {
            "guessed"
        };
        hits.into_iter().map(|h| (h, "dispatch", conf)).collect()
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

    /// The unique non-module, non-test node named `name`.
    fn guess(&self, name: &str) -> Option<String> {
        self.by_name
            .get(name)
            .copied()
            .flatten()
            .map(str::to_string)
    }

    /// A free call to `name`: its `autoImports` node, else the one a stale declaration file
    /// misses. A package-sourced name resolves to nothing, never to a by-name guess.
    fn free(&self, name: &str) -> (Option<String>, &'static str) {
        if let Some(t) = self.auto(name) {
            return (Some(t), "resolved");
        }
        if self.auto.is_external(name) {
            return (None, "resolved");
        }
        match self.stale_hit(name) {
            Some((id, _)) => (Some(id.to_string()), "resolved"),
            None => (self.guess(name), "guessed"),
        }
    }

    /// The unique node a stale declaration file misses for a name it doesn't declare.
    fn stale_hit(&self, name: &str) -> Option<(&'f str, &'f str)> {
        if self.auto.get(name).is_some() || self.auto.is_external(name) {
            return None;
        }
        self.stale.get(name).copied().flatten()
    }

    /// The node an `autoImports` name stands for.
    fn auto(&self, name: &str) -> Option<String> {
        let (file, export) = self.auto.get(name)?;
        self.export(file, &ImportName::named(export), &mut HashSet::new())
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
            if let Callee::Typed(t, m) = &r.callee {
                for (to, kind, conf) in self.typed(f, t, m) {
                    out.push(Edge {
                        from: r.from.clone(),
                        to,
                        kind: kind.into(),
                        confidence: conf.into(),
                        file: f.file.clone(),
                        line: r.line,
                        namespace: false,
                        reexport: false,
                    });
                }
                continue;
            }
            let (to, conf) = match &r.callee {
                Callee::Node(id) => (Some(id.clone()), "resolved"),
                Callee::Import(spec, n) => (self.import(f, spec, n), "resolved"),
                Callee::ImportMember(spec, n, prop) => match self.import(f, spec, n) {
                    Some(t) if self.files.contains_key(t.as_str()) => (
                        self.export(&t, &ImportName::named(prop), &mut HashSet::new()),
                        "resolved",
                    ),
                    _ => (self.guess(prop), "guessed"),
                },
                Callee::Member {
                    class,
                    name,
                    skip_self,
                } => (self.method(class, name, *skip_self), "resolved"),
                Callee::Typed(..) => unreachable!(),
                Callee::Auto(name) => (self.auto(name), "resolved"),
                Callee::Free(name) => self.free(name),
                Callee::Guess(name) => (self.guess(name), "guessed"),
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
