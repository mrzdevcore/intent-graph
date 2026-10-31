//! Pass 1: per-file parse, node extraction, export tables and unresolved references.

use crate::embedded::{self, MarkupRef};
use globset::GlobSet;
use ig_core::model::{Metrics, Node, Range, innermost_at, sha1_bytes};
pub use ig_core::model::{intents_in, normalized_hash};
use oxc_allocator::Allocator;
use oxc_ast::ast::*;
use oxc_ast_visit::{Visit, walk};
use oxc_semantic::{Scoping, SemanticBuilder, SymbolId};
use oxc_span::{GetSpan, SourceType, Span};
use oxc_syntax::module_record::{
    ExportExportName, ExportImportName, ExportLocalName, ImportImportName, ModuleRecord,
};
use oxc_syntax::scope::ScopeFlags;
use std::collections::HashMap;

mod callee;
mod http;
mod visit_impl;
mod visitor;

use callee::export_tables;

/// Node's own modules: imported bare, but no package.
const NODE_BUILTINS: &[&str] = &[
    "assert",
    "async_hooks",
    "buffer",
    "child_process",
    "cluster",
    "crypto",
    "dns",
    "events",
    "fs",
    "http",
    "http2",
    "https",
    "module",
    "net",
    "os",
    "path",
    "perf_hooks",
    "process",
    "querystring",
    "readline",
    "stream",
    "string_decoder",
    "timers",
    "tls",
    "tty",
    "url",
    "util",
    "v8",
    "vm",
    "worker_threads",
    "zlib",
];

#[derive(Debug, Clone, PartialEq)]
pub enum ImportName {
    Named(String),
    Default,
    Namespace,
}

impl ImportName {
    pub fn named(n: &str) -> ImportName {
        if n == "default" {
            ImportName::Default
        } else {
            ImportName::Named(n.into())
        }
    }
}

#[derive(Debug, Clone)]
pub enum Callee {
    /// A node of this file.
    Node(String),
    /// An import binding: specifier and imported name.
    Import(String, ImportName),
    /// `x.m()` where `x` is a non-namespace import binding: resolved only if `x` turns out to be
    /// a namespace (for example `export * as x`); guessed by `m` otherwise.
    ImportMember(String, ImportName, String),
    /// `this.m()` / `super.m()` inside class `class` (node id).
    Member {
        class: String,
        name: String,
        skip_self: bool,
    },
    /// Anything else: matched by bare name.
    Guess(String),
    /// An identifier bound nowhere in the file (a global): an `autoImports` name, else guessed.
    Free(String),
    /// A template tag or word neither imported nor local: resolved only through `autoImports`.
    Auto(String),
    /// `x.m()` / `x.m` where `x` has a declared or constructed type: method `m` of that type (or,
    /// for an interface, of its implementations).
    Typed(Box<Callee>, String),
}

#[derive(Debug, Clone)]
pub struct RawRef {
    pub from: String,
    pub kind: &'static str,
    pub callee: Callee,
    pub line: u32,
    /// Inside a loop body or an iterating callback (`.map`, `.forEach`, …).
    pub in_loop: bool,
}

#[derive(Debug, Clone)]
pub struct ImportRef {
    pub spec: String,
    pub line: u32,
    pub namespace: bool,
    pub reexport: bool,
}

#[derive(Debug, Clone)]
pub enum ExportTarget {
    Node(String),
    Reexport(String, ImportName),
    Namespace(String),
}

#[derive(Debug, Clone, Default)]
pub struct ExportTable {
    pub names: HashMap<String, ExportTarget>,
    pub stars: Vec<String>,
}

pub use ig_core::http::HttpCall;

#[derive(Debug, Default)]
pub struct FileFacts {
    pub file: String,
    pub nodes: Vec<Node>,
    pub parse_error: Option<String>,
    pub imports: Vec<ImportRef>,
    /// specifier → repo-relative target file
    pub resolved: HashMap<String, String>,
    pub exports: ExportTable,
    pub refs: Vec<RawRef>,
    /// class node id → its `extends` callee
    pub classes: Vec<(String, Option<Callee>)>,
    /// Source the patterns run on (the extracted script for embedded files).
    pub script: String,
    pub http: Vec<HttpCall>,
    /// Bare imports that resolve to no repo file: (package name, line).
    pub packages: Vec<(String, u32)>,
    /// `const NAME = '<url-ish string>'` anywhere in the file, for route paths built from constants.
    pub url_consts: HashMap<String, String>,
    /// Callee name → calls whose first argument looks like a URL.
    pub url_callees: HashMap<String, u32>,
}

pub struct FileInput<'c> {
    pub path: &'c str,
    pub source: &'c str,
    pub package: Option<&'c str>,
    pub embedded: bool,
    pub is_test: bool,
    pub test_calls: &'c [String],
    /// Callee names whose first argument is a URL (`blueprint.http.calls`).
    pub http_calls: &'c [String],
}

pub struct Lines(Vec<usize>);

impl Lines {
    pub fn new(src: &str) -> Lines {
        let mut v = vec![0];
        v.extend(
            src.bytes()
                .enumerate()
                .filter(|(_, b)| *b == b'\n')
                .map(|(i, _)| i + 1),
        );
        Lines(v)
    }
    /// 1-based line of a byte offset.
    pub fn line(&self, off: usize) -> u32 {
        self.0.partition_point(|&s| s <= off) as u32
    }
}

pub fn line_count(src: &str) -> u32 {
    let s = src.strip_suffix('\n').unwrap_or(src);
    (s.bytes().filter(|b| *b == b'\n').count() as u32 + 1).max(1)
}

fn basename(p: &str) -> &str {
    p.rsplit('/').next().unwrap_or(p)
}

struct Scope {
    qual: String,
    node: Option<usize>,
    is_test: bool,
}

struct Extractor<'s, 'a> {
    src: &'s str,
    file: &'s str,
    package: Option<&'s str>,
    lines: Lines,
    comments: &'s [Comment],
    scoping: &'s Scoping,
    is_test: bool,
    test_calls: &'s [String],
    nodes: Vec<Node>,
    ids: HashMap<String, u32>,
    scopes: Vec<Scope>,
    sym_node: HashMap<SymbolId, String>,
    top_names: HashMap<String, String>,
    imports_map: HashMap<String, (String, ImportName)>,
    import_refs: Vec<ImportRef>,
    refs: Vec<(Option<String>, &'static str, Pending, u32, bool)>,
    classes: Vec<(String, Option<Pending>)>,
    this_ctx: Vec<Option<String>>,
    method_value: Option<Span>,
    pending_class: Option<String>,
    export_span: Option<Span>,
    fn_depth: u32,
    /// Loops (and iterating callbacks) around the current position.
    loop_depth: u32,
    http_calls: &'s [String],
    url_vars: HashMap<SymbolId, String>,
    url_consts: HashMap<String, String>,
    /// Declared or constructed types of variables and parameters.
    typed: HashMap<SymbolId, Pending>,
    url_callees: HashMap<String, u32>,
    /// class node id → field name → type
    fields: HashMap<String, HashMap<String, Pending>>,
    http: Vec<HttpCall>,
    default_ident: Option<String>,
    _a: std::marker::PhantomData<&'a ()>,
}

/// A callee before symbols are mapped to nodes (declarations may come later in the file).
#[derive(Clone)]
enum Pending {
    Sym(SymbolId, String),
    Ready(Callee),
    Typed(Box<Pending>, String),
}

/// Parses and extracts one file. `resolve` maps a specifier to a repo-relative file.
pub fn extract(input: &FileInput, resolve: &dyn Fn(&str) -> Option<String>) -> FileFacts {
    let script = if input.embedded {
        Some(embedded::extract_script(input.source))
    } else {
        None
    };
    let (code, source_type) = match &script {
        Some(s) => (
            s.source.as_str(),
            if s.tsx {
                SourceType::tsx()
            } else if s.ts {
                SourceType::ts()
            } else {
                SourceType::mjs()
            },
        ),
        None => (
            input.source,
            SourceType::from_path(input.path).unwrap_or_else(|_| SourceType::mjs()),
        ),
    };
    let alloc = Allocator::default();
    let ret = oxc_parser::Parser::new(&alloc, code, source_type).parse();
    let parse_error = ret.errors.first().map(|e| e.message.to_string());
    let program = ret.program;
    let semantic = SemanticBuilder::new().build(&program).semantic;
    let scoping = semantic.into_scoping();
    let lines = Lines::new(code);
    let end_line = line_count(input.source);
    let module = Node {
        id: input.path.into(),
        kind: if input.embedded {
            "component"
        } else {
            "module"
        }
        .into(),
        name: basename(input.path).into(),
        file: input.path.into(),
        range: Range {
            start_line: 1,
            end_line,
        },
        parent: None,
        package: input.package.map(str::to_string),
        exported: false,
        hash: sha1_bytes(input.source.as_bytes()),
        metrics: Metrics {
            loc: end_line,
            complexity: 1,
        },
        intents: vec![],
    };
    let mut imports_map = HashMap::new();
    for e in &ret.module_record.import_entries {
        let name = match &e.import_name {
            ImportImportName::Name(n) => ImportName::named(n.name.as_str()),
            ImportImportName::NamespaceObject => ImportName::Namespace,
            ImportImportName::Default(_) => ImportName::Default,
        };
        imports_map.insert(
            e.local_name.name.as_str().to_string(),
            (e.module_request.name.as_str().to_string(), name),
        );
    }
    let mut x = Extractor {
        src: code,
        file: input.path,
        package: input.package,
        lines,
        comments: &program.comments,
        scoping: &scoping,
        is_test: input.is_test,
        test_calls: input.test_calls,
        nodes: vec![module],
        ids: HashMap::new(),
        scopes: vec![],
        sym_node: HashMap::new(),
        top_names: HashMap::new(),
        imports_map,
        import_refs: vec![],
        refs: vec![],
        classes: vec![],
        this_ctx: vec![],
        method_value: None,
        pending_class: None,
        export_span: None,
        fn_depth: 0,
        loop_depth: 0,
        http_calls: input.http_calls,
        url_vars: HashMap::new(),
        url_consts: HashMap::new(),
        typed: HashMap::new(),
        url_callees: HashMap::new(),
        fields: HashMap::new(),
        http: vec![],
        default_ident: None,
        _a: std::marker::PhantomData,
    };
    x.visit_program(&program);

    // Markup references of embedded files come from the component itself.
    if input.embedded {
        let mlines = Lines::new(input.source);
        for (r, off) in embedded::scan_markup(input.source) {
            let line = mlines.line(off);
            let (kind, names) = match &r {
                MarkupRef::Tag(t) => ("renders", vec![t.clone(), embedded::pascal(t)]),
                MarkupRef::Word(w) => ("calls", vec![w.clone()]),
            };
            let callee = names
                .iter()
                .find_map(|n| {
                    if let Some((spec, name)) = x.imports_map.get(n) {
                        return Some(Callee::Import(spec.clone(), name.clone()));
                    }
                    x.top_names.get(n).map(|id| Callee::Node(id.clone()))
                })
                .unwrap_or_else(|| Callee::Auto(names.last().unwrap().clone()));
            x.refs.push((
                Some(input.path.to_string()),
                kind,
                Pending::Ready(callee),
                line,
                false,
            ));
        }
    }

    let (exports, local_names) = export_tables(&ret.module_record, &x);
    let exported: std::collections::HashSet<&String> = local_names
        .iter()
        .filter_map(|l| x.top_names.get(l))
        .collect();
    let exported: std::collections::HashSet<String> = exported.into_iter().cloned().collect();
    for n in x.nodes.iter_mut().skip(1) {
        if exported.contains(&n.id) && !matches!(n.kind.as_str(), "method" | "test") {
            n.exported = true;
        }
    }

    fn finish(p: &Pending, x: &Extractor, scoping: &Scoping) -> Callee {
        match p {
            Pending::Ready(c) => c.clone(),
            Pending::Typed(t, m) => Callee::Typed(Box::new(finish(t, x, scoping)), m.clone()),
            Pending::Sym(s, name) => {
                if let Some(id) = x.sym_node.get(s) {
                    return Callee::Node(id.clone());
                }
                let root = scoping.symbol_scope_id(*s) == scoping.root_scope_id();
                if root
                    && scoping.symbol_flags(*s).is_import()
                    && let Some((spec, n)) = x.imports_map.get(name)
                {
                    return Callee::Import(spec.clone(), n.clone());
                }
                Callee::Guess(name.clone())
            }
        }
    }
    let finish = |p: &Pending| finish(p, &x, &scoping);
    let refs: Vec<RawRef> = x
        .refs
        .iter()
        .map(|(from, kind, p, line, in_loop)| RawRef {
            from: from
                .clone()
                .unwrap_or_else(|| innermost_at(&x.nodes, *line).to_string()),
            kind,
            callee: finish(p),
            line: *line,
            in_loop: *in_loop,
        })
        .collect();
    let classes = x
        .classes
        .iter()
        .map(|(id, e)| (id.clone(), e.as_ref().map(&finish)))
        .collect();
    let mut resolved = HashMap::new();
    for i in &x.import_refs {
        if !resolved.contains_key(&i.spec)
            && let Some(t) = resolve(&i.spec)
        {
            resolved.insert(i.spec.clone(), t);
        }
    }
    for (spec, _) in x.imports_map.values() {
        if !resolved.contains_key(spec)
            && let Some(t) = resolve(spec)
        {
            resolved.insert(spec.clone(), t);
        }
    }
    let mut packages = vec![];
    for i in &x.import_refs {
        let bare = !i.spec.starts_with('.')
            && !i.spec.starts_with('/')
            && !i.spec.starts_with('#')
            && !i.spec.contains(':');
        if bare && !resolved.contains_key(&i.spec) {
            let mut parts = i.spec.split('/');
            let first = parts.next().unwrap_or("");
            let name = if first.starts_with('@') {
                format!("{first}/{}", parts.next().unwrap_or(""))
            } else {
                first.to_string()
            };
            if !NODE_BUILTINS.contains(&name.as_str()) {
                packages.push((name, i.line));
            }
        }
    }
    FileFacts {
        file: input.path.into(),
        http: x.http,
        url_consts: x.url_consts,
        url_callees: x.url_callees,
        packages,
        nodes: x.nodes,
        parse_error,
        imports: x.import_refs,
        resolved,
        exports,
        refs,
        classes,
        script: script.map(|s| s.source).unwrap_or_default(),
    }
}

pub fn is_test_file(path: &str, tests: &GlobSet) -> bool {
    tests.is_match(path)
}

#[cfg(test)]
mod tests;
