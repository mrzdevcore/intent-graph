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
    pub workspace: Workspace,
    /// Facts that are not graph data, for `ig setup`.
    pub extras: Extras,
}

#[derive(Default)]
pub struct Extras {
    /// External package → number of modules importing it.
    pub packages: BTreeMap<String, usize>,
    /// Callee name → calls whose first argument looks like a URL.
    pub url_callees: BTreeMap<String, u32>,
    /// HTTP client calls that matched no route: (file, line, method, url).
    pub http_unmatched: Vec<(String, u32, String, String)>,
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
                http_calls: &config.blueprint.http_calls,
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
    let mut edge_lists: Vec<Vec<Edge>> = facts.par_iter().map(|f| index.edges_of(f)).collect();
    let (system_nodes, blueprint_edges, renames, http_unmatched) = blueprint_links(&facts, config);
    let mut extras = Extras {
        http_unmatched,
        ..Extras::default()
    };
    for f in facts.iter().filter(|f| !tests.is_match(&f.file)) {
        let mut seen = HashSet::new();
        for (p, _) in &f.packages {
            if seen.insert(p) {
                *extras.packages.entry(p.clone()).or_default() += 1;
            }
        }
        for (k, v) in &f.url_callees {
            *extras.url_callees.entry(k.clone()).or_default() += v;
        }
    }
    edge_lists.push(blueprint_edges);
    for f in &mut facts {
        for n in &mut f.nodes {
            if let Some(name) = renames.get(&n.id) {
                n.name = name.clone();
            }
        }
    }

    let mut nodes: Vec<Node> = system_nodes;
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
        extras,
    })
}

/// URL path segments with dynamic parts (`${x}`, `{param}`, `:param`) as `:`, without scheme, host,
/// query or fragment.
pub fn url_segments(url: &str) -> Vec<String> {
    let mut u = url;
    if let Some(i) = u.find("://") {
        u = &u[i + 3..];
        u = u.find('/').map(|j| &u[j..]).unwrap_or("");
    }
    let u = u.split(['?', '#']).next().unwrap_or("");
    u.split('/')
        .filter(|s| !s.is_empty())
        .map(|s| {
            if s.contains('\u{0}') {
                "\u{0}".to_string()
            } else if s.contains(':') || s.contains('{') || s.contains('*') {
                ":".to_string()
            } else {
                s.to_string()
            }
        })
        .collect()
}

fn seg_match(route: &[String], call: &[String], loose: bool) -> bool {
    route.len() == call.len()
        && route
            .iter()
            .zip(call)
            .all(|(r, c)| r == c || (loose && r == ":"))
}

/// `http` edges from client calls to route nodes, and `system` nodes with `uses` edges from the
/// modules importing their packages.
fn blueprint_links(
    facts: &[FileFacts],
    config: &Config,
) -> (
    Vec<Node>,
    Vec<Edge>,
    HashMap<String, String>,
    Vec<(String, u32, String, String)>,
) {
    let bp = &config.blueprint;
    let mut edges = vec![];
    let mut unmatched = vec![];
    // Route node id → display name with `${CONST}` substituted (the id stays as matched).
    let mut renames = HashMap::new();
    let edge = |from: &str, to: &str, kind: &str, conf: &str, file: &str, line: u32| Edge {
        from: from.into(),
        to: to.into(),
        kind: kind.into(),
        confidence: conf.into(),
        file: file.into(),
        line,
        namespace: false,
        reexport: false,
    };
    if !bp.http_calls.is_empty() {
        // Route node names are "METHOD /path" (or just "/path" for any method).
        // `${NAME}` in a route path is replaced by a same-file string constant.
        let substitute = |path: &str, consts: &HashMap<String, String>| -> String {
            let mut out = String::new();
            let mut rest = path;
            while let Some(i) = rest.find("${") {
                out.push_str(&rest[..i]);
                let Some(j) = rest[i..].find('}') else { break };
                let name = rest[i + 2..i + j].trim();
                // Unknown: a segment that never matches, so the route can't be linked by accident.
                out.push_str(consts.get(name).map(String::as_str).unwrap_or("\u{0}"));
                rest = &rest[i + j + 1..];
            }
            out.push_str(rest);
            out
        };
        let routes: Vec<(String, Vec<String>, &str, String)> = facts
            .iter()
            .flat_map(|f| f.nodes.iter().map(move |n| (f, n)))
            .filter(|(_, n)| n.kind == bp.route_kind)
            .map(|(f, n)| {
                let (m, p) = match n.name.split_once(' ') {
                    Some((m, p)) if m.chars().all(|c| c.is_ascii_alphabetic()) => {
                        (m.to_ascii_uppercase(), p)
                    }
                    _ => ("*".to_string(), n.name.as_str()),
                };
                let p = substitute(p.trim().trim_matches(['\'', '"', '`']), &f.url_consts);
                (m, url_segments(&p), n.id.as_str(), p)
            })
            .collect();
        for (m, _, id, p) in &routes {
            if !p.contains('\u{0}') && n_name_differs(facts, id, m, p) {
                renames.insert(
                    id.to_string(),
                    if m == "*" {
                        p.clone()
                    } else {
                        format!("{m} {p}")
                    },
                );
            }
        }
        let routes: Vec<(String, Vec<String>, &str)> =
            routes.into_iter().map(|(m, s, id, _)| (m, s, id)).collect();
        for f in facts {
            for h in &f.http {
                let mut call = url_segments(&h.url);
                while call.first().is_some_and(|s| s == ":") {
                    call.remove(0);
                }
                let prefix = url_segments(&bp.http_prefix);
                if !prefix.is_empty() && call.starts_with(&prefix) {
                    call.drain(..prefix.len());
                }
                let method_ok = |m: &str| m == "*" || m == h.method;
                let pick = |loose: bool| -> Vec<&str> {
                    routes
                        .iter()
                        .filter(|(m, r, _)| method_ok(m) && seg_match(r, &call, loose))
                        .map(|(_, _, id)| *id)
                        .collect()
                };
                // Last resort: the call has extra leading segments (an `/api` prefix added by a proxy).
                let suffix = || -> Vec<&str> {
                    routes
                        .iter()
                        .filter(|(m, r, _)| {
                            method_ok(m)
                                && r.len() >= 2
                                && r.len() < call.len()
                                && seg_match(r, &call[call.len() - r.len()..], true)
                        })
                        .map(|(_, _, id)| *id)
                        .collect()
                };
                let (hits, conf) = match pick(false) {
                    v if !v.is_empty() => (v, "resolved"),
                    _ => match pick(true) {
                        v if !v.is_empty() => (v, "guessed"),
                        _ => (suffix(), "guessed"),
                    },
                };
                if hits.is_empty() {
                    unmatched.push((f.file.clone(), h.line, h.method.clone(), h.url.clone()));
                }
                for id in hits {
                    edges.push(edge(&h.from, id, "http", conf, &f.file, h.line));
                }
            }
        }
    }
    let mut systems = std::collections::BTreeMap::new();
    for f in facts {
        for (pkg, line) in &f.packages {
            if let Some((name, _)) = bp
                .systems
                .iter()
                .find(|(_, pkgs)| pkgs.iter().any(|p| p == pkg))
            {
                let id = format!("system:{name}");
                edges.push(edge(&f.file, &id, "uses", "resolved", &f.file, *line));
                systems.entry(id).or_insert_with(|| name.clone());
            }
        }
    }
    let nodes = systems
        .into_iter()
        .map(|(id, name)| Node {
            hash: crate::model::sha1_hex(&id),
            id,
            kind: "system".into(),
            name,
            file: String::new(),
            range: crate::model::Range {
                start_line: 1,
                end_line: 1,
            },
            parent: None,
            package: None,
            exported: false,
            metrics: crate::model::Metrics {
                loc: 0,
                complexity: 0,
            },
            intents: vec![],
        })
        .collect();
    (nodes, edges, renames, unmatched)
}

fn n_name_differs(facts: &[FileFacts], id: &str, m: &str, p: &str) -> bool {
    let want = if m == "*" {
        p.to_string()
    } else {
        format!("{m} {p}")
    };
    facts
        .iter()
        .flat_map(|f| f.nodes.iter())
        .find(|n| n.id == id)
        .is_some_and(|n| n.name != want)
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
}

impl<'f> Index<'f> {
    fn new(facts: &'f [FileFacts]) -> Index<'f> {
        let mut ix = Index {
            files: HashMap::new(),
            children: HashMap::new(),
            by_name: HashMap::new(),
            classes: HashMap::new(),
            implementers: HashMap::new(),
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

    /// Method `m` of type `t` (a `calls` edge), else of the classes implementing it (a `dispatch`
    /// edge: the code depends on the interface, so arch rules and review ignore it). Confidence is
    /// `guessed` when several implementations match.
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
            && let Some(x) = self.method(&tid, m, false)
        {
            return vec![(x, "calls", "resolved")];
        }
        let hits: Vec<String> = self
            .implementers
            .get(&tid)
            .into_iter()
            .flatten()
            .filter_map(|c| self.method(c, m, false))
            .collect();
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
                Callee::Typed(..) => unreachable!(),
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

    #[test]
    fn typed_calls_through_injection_and_interfaces() {
        let dir = std::env::temp_dir().join(format!("ig-typed-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let w = |p: &str, s: &str| {
            let f = dir.join(p);
            std::fs::create_dir_all(f.parent().unwrap()).unwrap();
            std::fs::write(f, s).unwrap();
        };
        w("package.json", r#"{"name":"app"}"#);
        w(
            "src/domain/repo.ts",
            "export interface Repo { find(id: string): string }\n",
        );
        w(
            "src/infra/pg-repo.ts",
            "import type { Repo } from '../domain/repo';\nexport class PgRepo implements Repo {\n  find(id: string) { return id; }\n}\n",
        );
        w(
            "src/use-cases/get.ts",
            "import type { Repo } from '../domain/repo';\nexport class GetUseCase {\n  constructor(private readonly repo: Repo) {}\n  execute(id: string) { return this.repo.find(id); }\n}\n",
        );
        w(
            "src/routes.ts",
            "import { GetUseCase } from './use-cases/get';\nimport { PgRepo } from './infra/pg-repo';\nexport function routes() {\n  const useCase = new GetUseCase(new PgRepo());\n  return [{ method: 'GET', path: '/x', handler: useCase.execute.bind(useCase) }];\n}\n",
        );
        let g = build(&dir, &Config::default(), None).unwrap().graph;
        let edge = |from: &str, to: &str| {
            g.edges
                .iter()
                .find(|e| e.from == from && e.to == to)
                .map(|e| e.confidence.clone())
        };
        let dispatch = g.edges.iter().find(|e| {
            e.from == "src/use-cases/get.ts#GetUseCase.execute"
                && e.to == "src/infra/pg-repo.ts#PgRepo.find"
        });
        assert_eq!(
            dispatch.map(|e| (e.kind.as_str(), e.confidence.as_str())),
            Some(("dispatch", "resolved")),
            "{:#?}",
            g.edges
        );
        assert_eq!(
            edge(
                "src/routes.ts#routes",
                "src/use-cases/get.ts#GetUseCase.execute"
            )
            .as_deref(),
            Some("resolved")
        );
        // Interfaces are not nodes, and edges to them are dropped.
        assert!(!g.nodes.iter().any(|n| n.id == "src/domain/repo.ts#Repo"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn url_normalisation() {
        assert_eq!(
            url_segments("/api/lines/:/notes?x=1"),
            ["api", "lines", ":", "notes"]
        );
        assert_eq!(url_segments("https://h:8080/api/{id}/x"), ["api", ":", "x"]);
        assert_eq!(url_segments("/api/item-:/x"), ["api", ":", "x"]);
        // Strict: segments equal. Loose: a route parameter also matches a literal in the call,
        // but a dynamic call segment never matches a literal route segment.
        assert!(!seg_match(
            &url_segments("/api/:/:"),
            &url_segments("/api/1/:"),
            false
        ));
        assert!(seg_match(
            &url_segments("/api/:/:"),
            &url_segments("/api/1/:"),
            true
        ));
        assert!(!seg_match(
            &url_segments("/api/:/available"),
            &url_segments("/api/:/:"),
            true
        ));
    }

    #[test]
    fn blueprint_links_http_and_systems() {
        let dir = std::env::temp_dir().join(format!("ig-blueprint-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let w = |p: &str, s: &str| {
            let f = dir.join(p);
            std::fs::create_dir_all(f.parent().unwrap()).unwrap();
            std::fs::write(f, s).unwrap();
        };
        w("package.json", r#"{"name":"app"}"#);
        w(
            "src/client.ts",
            "export class Notes {\n  constructor(private readonly fetch: any) {}\n  async add(line: number, batch: string) {\n    return this.fetch(`/api/lines/${line}/batches/${batch}/notes`, { method: 'POST' });\n  }\n  async list(line: number) {\n    let url = `/api/lines/${line}/batches`;\n    url += '?x=1';\n    return this.fetch(url, {});\n  }\n}\n",
        );
        w(
            "src/routes.ts",
            "import knex from 'knex';\nexport const routes = [\n  { method: 'POST', path: '/api/lines/{line}/batches/{batch}/notes', handler: () => knex('notes') },\n  { method: 'GET', path: '/api/lines/{line}/batches', handler: () => 1 },\n];\n",
        );
        let cfg = Config::parse(
            "patterns:\n  - kind: route\n    match: \"{ method: $METHOD, path: $PATH, $$$REST }\"\n    id: \"{METHOD} {PATH}\"\nblueprint:\n  http: { calls: [fetch] }\n  systems:\n    PostgreSQL: [knex, pg]\n",
        )
        .unwrap();
        let g = build(&dir, &cfg, None).unwrap().graph;
        let has = |from: &str, kind: &str, to: &str| {
            g.edges
                .iter()
                .any(|e| e.from == from && e.kind == kind && e.to == to)
        };
        assert!(
            has(
                "src/client.ts#Notes.add",
                "http",
                "src/routes.ts#POST /api/lines/{line}/batches/{batch}/notes"
            ),
            "{:#?}",
            g.edges
        );
        assert!(has(
            "src/client.ts#Notes.list",
            "http",
            "src/routes.ts#GET /api/lines/{line}/batches"
        ));
        assert!(has("src/routes.ts", "uses", "system:PostgreSQL"));
        assert!(
            g.nodes
                .iter()
                .any(|n| n.id == "system:PostgreSQL" && n.kind == "system")
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
