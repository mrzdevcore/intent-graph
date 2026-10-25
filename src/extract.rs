//! Pass 1: per-file parse, node extraction, export tables and unresolved references.

use crate::embedded::{self, MarkupRef};
use crate::model::{Metrics, Node, Range, sha1_bytes, sha1_hex};
use globset::GlobSet;
use oxc_allocator::Allocator;
use oxc_ast::ast::*;
use oxc_ast_visit::{Visit, walk};
use oxc_semantic::{Scoping, SemanticBuilder, SymbolId};
use oxc_span::{GetSpan, SourceType, Span};
use oxc_syntax::module_record::{
    ExportExportName, ExportImportName, ExportLocalName, ImportImportName, ModuleRecord,
};
use oxc_syntax::scope::ScopeFlags;
use regex::Regex;
use std::collections::HashMap;
use std::sync::LazyLock;

static INTENT: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"@intent\s+([A-Z]+-\d+(?:\s*,\s*[A-Z]+-\d+)*)").unwrap());

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

/// An HTTP client call: method and URL with dynamic parts as `:`.
#[derive(Debug, Clone, PartialEq)]
pub struct HttpCall {
    pub from: String,
    pub method: String,
    pub url: String,
    pub line: u32,
}

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

pub fn normalized_hash(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut ws = false;
    for c in text.trim().chars() {
        if c.is_whitespace() {
            ws = true;
        } else {
            if ws {
                out.push(' ');
                ws = false;
            }
            out.push(c);
        }
    }
    sha1_hex(&out)
}

pub fn intents_in(text: &str, out: &mut Vec<String>) {
    for c in INTENT.captures_iter(text) {
        for tag in c[1].split(',') {
            let tag = tag.trim().to_string();
            if !out.contains(&tag) {
                out.push(tag);
            }
        }
    }
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
    refs: Vec<(Option<String>, &'static str, Pending, u32)>,
    classes: Vec<(String, Option<Pending>)>,
    this_ctx: Vec<Option<String>>,
    method_value: Option<Span>,
    pending_class: Option<String>,
    export_span: Option<Span>,
    fn_depth: u32,
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

impl<'s, 'a> Extractor<'s, 'a> {
    fn line(&self, off: u32) -> u32 {
        self.lines.line(off as usize)
    }

    fn leading_comments(&self, start: u32) -> String {
        let mut pos = start as usize;
        let mut text = String::new();
        let idx = self
            .comments
            .partition_point(|c| c.span.end as usize <= pos);
        for c in self.comments[..idx].iter().rev() {
            let gap = &self.src[c.span.end as usize..pos];
            if !gap.trim().is_empty() {
                break;
            }
            text.insert_str(0, &self.src[c.span.start as usize..c.span.end as usize]);
            text.insert(0, '\n');
            pos = c.span.start as usize;
        }
        text
    }

    fn innermost_node(&self) -> usize {
        self.scopes.iter().rev().find_map(|s| s.node).unwrap_or(0)
    }

    fn bump(&mut self) {
        let i = self.innermost_node();
        self.nodes[i].metrics.complexity += 1;
    }

    /// Creates a node anchored on `anchor`, pushes its scope, and returns its index.
    fn push_node(
        &mut self,
        kind: &str,
        name: &str,
        anchor: Span,
        extra_comments: Option<Span>,
        test: bool,
    ) -> usize {
        let top = self.scopes.last();
        let qual = match top {
            Some(s) if test && s.is_test => format!("{} > {name}", s.qual),
            Some(s) => format!("{}.{name}", s.qual),
            None => name.to_string(),
        };
        let base = format!("{}#{qual}", self.file);
        let n = self.ids.entry(base.clone()).or_insert(0);
        *n += 1;
        let id = if *n == 1 { base } else { format!("{base}~{n}") };
        let qual = if *n == 1 { qual } else { format!("{qual}~{n}") };
        let mut intents = vec![];
        intents_in(&self.leading_comments(anchor.start), &mut intents);
        if let Some(sp) = extra_comments {
            for c in self
                .comments
                .iter()
                .filter(|c| c.span.start >= sp.start && c.span.end <= sp.end)
            {
                intents_in(
                    &self.src[c.span.start as usize..c.span.end as usize],
                    &mut intents,
                );
            }
        }
        let (start, end) = (
            self.line(anchor.start),
            self.line(anchor.end.saturating_sub(1).max(anchor.start)),
        );
        let parent = self.nodes[self.innermost_node()].id.clone();
        let text = &self.src[anchor.start as usize..anchor.end as usize];
        self.nodes.push(Node {
            id,
            kind: kind.into(),
            name: name.into(),
            file: self.file.into(),
            range: Range {
                start_line: start,
                end_line: end,
            },
            parent: Some(parent),
            package: self.package.map(str::to_string),
            exported: false,
            hash: normalized_hash(text),
            metrics: Metrics {
                loc: end - start + 1,
                complexity: 1,
            },
            intents,
        });
        let idx = self.nodes.len() - 1;
        self.scopes.push(Scope {
            qual,
            node: Some(idx),
            is_test: test,
        });
        idx
    }

    fn register(&mut self, idx: usize, binding: Option<&BindingIdentifier>, top_level: bool) {
        let id = self.nodes[idx].id.clone();
        if let Some(b) = binding
            && let Some(s) = b.symbol_id.get()
        {
            self.sym_node.insert(s, id.clone());
        }
        if top_level {
            self.top_names
                .entry(self.nodes[idx].name.clone())
                .or_insert(id);
        }
    }

    /// Top-level interfaces and type aliases are not nodes, but get an id so `implements` and typed
    /// calls can be traced to their implementations.
    fn type_target(&mut self, id: &BindingIdentifier) {
        if !self.at_top() {
            return;
        }
        let tid = format!("{}#{}", self.file, id.name.as_str());
        self.top_names
            .entry(id.name.as_str().to_string())
            .or_insert_with(|| tid.clone());
        if let Some(s) = id.symbol_id.get() {
            self.sym_node.entry(s).or_insert(tid);
        }
    }

    fn at_top(&self) -> bool {
        self.scopes.is_empty()
    }

    fn ident_callee(&self, id: &IdentifierReference) -> Pending {
        let name = id.name.as_str().to_string();
        match id
            .reference_id
            .get()
            .and_then(|r| self.scoping.get_reference(r).symbol_id())
        {
            Some(s) => Pending::Sym(s, name),
            None => Pending::Ready(Callee::Guess(name)),
        }
    }

    /// The declared or constructed type of `x` or `this.f`, if known.
    fn type_of(&self, e: &Expression) -> Option<Pending> {
        match e {
            Expression::Identifier(id) => {
                let s = id
                    .reference_id
                    .get()
                    .and_then(|r| self.scoping.get_reference(r).symbol_id())?;
                self.typed.get(&s).cloned()
            }
            Expression::StaticMemberExpression(m)
                if matches!(
                    m.object.get_inner_expression(),
                    Expression::ThisExpression(_)
                ) =>
            {
                let class = self.this_ctx.last()?.as_ref()?;
                self.fields
                    .get(class)?
                    .get(m.property.name.as_str())
                    .cloned()
            }
            _ => None,
        }
    }

    fn annotation_type(&self, ta: Option<&TSTypeAnnotation>) -> Option<Pending> {
        match &ta?.type_annotation {
            TSType::TSTypeReference(r) => self.type_name_callee(&r.type_name),
            _ => None,
        }
    }

    fn new_type(&self, e: Option<&Expression>) -> Option<Pending> {
        match e?.get_inner_expression() {
            Expression::NewExpression(n) => self.callee_of(&n.callee),
            _ => None,
        }
    }

    /// Field types of a class: annotated or `new`-initialised properties, and constructor
    /// parameter properties (`private readonly repo: Repo`).
    fn collect_fields(&mut self, class: &Class<'a>, id: &str) {
        let mut f = HashMap::new();
        for el in &class.body.body {
            match el {
                ClassElement::PropertyDefinition(p) => {
                    if let (Some(k), Some(t)) = (
                        p.key.static_name(),
                        self.annotation_type(p.type_annotation.as_deref())
                            .or_else(|| self.new_type(p.value.as_ref())),
                    ) {
                        f.insert(k.to_string(), t);
                    }
                }
                ClassElement::MethodDefinition(m)
                    if m.kind == MethodDefinitionKind::Constructor =>
                {
                    for p in &m.value.params.items {
                        if p.accessibility.is_none() && !p.readonly {
                            continue;
                        }
                        if let (BindingPattern::BindingIdentifier(b), Some(t)) = (
                            &p.pattern,
                            self.annotation_type(p.type_annotation.as_deref()),
                        ) {
                            f.insert(b.name.as_str().to_string(), t);
                        }
                    }
                }
                _ => {}
            }
        }
        if !f.is_empty() {
            self.fields.insert(id.to_string(), f);
        }
    }

    /// The import binding `id` refers to, if any.
    fn import_of(&self, id: &IdentifierReference) -> Option<(String, ImportName)> {
        let s = id
            .reference_id
            .get()
            .and_then(|r| self.scoping.get_reference(r).symbol_id())?;
        if self.scoping.symbol_scope_id(s) != self.scoping.root_scope_id()
            || !self.scoping.symbol_flags(s).is_import()
        {
            return None;
        }
        self.imports_map.get(id.name.as_str()).cloned()
    }

    fn namespace_of(&self, id: &IdentifierReference) -> Option<String> {
        match self.import_of(id)? {
            (spec, ImportName::Namespace) => Some(spec),
            _ => None,
        }
    }

    fn callee_of(&self, e: &Expression) -> Option<Pending> {
        match e.get_inner_expression() {
            Expression::Identifier(id) => Some(self.ident_callee(id)),
            Expression::StaticMemberExpression(m) => {
                let prop = m.property.name.as_str().to_string();
                Some(match m.object.get_inner_expression() {
                    Expression::ThisExpression(_) => match self.this_ctx.last() {
                        Some(Some(c)) => Pending::Ready(Callee::Member {
                            class: c.clone(),
                            name: prop,
                            skip_self: false,
                        }),
                        _ => Pending::Ready(Callee::Guess(prop)),
                    },
                    Expression::Super(_) => match self.this_ctx.last() {
                        Some(Some(c)) => Pending::Ready(Callee::Member {
                            class: c.clone(),
                            name: prop,
                            skip_self: true,
                        }),
                        _ => Pending::Ready(Callee::Guess(prop)),
                    },
                    Expression::Identifier(o) => match self.namespace_of(o) {
                        Some(spec) => {
                            Pending::Ready(Callee::Import(spec, ImportName::named(&prop)))
                        }
                        None => match self.import_of(o) {
                            Some((spec, n)) => Pending::Ready(Callee::ImportMember(spec, n, prop)),
                            None => match self.type_of(m.object.get_inner_expression()) {
                                Some(t) => Pending::Typed(Box::new(t), prop),
                                None => Pending::Ready(Callee::Guess(prop)),
                            },
                        },
                    },
                    obj => match self.type_of(obj) {
                        Some(t) => Pending::Typed(Box::new(t), prop),
                        None => Pending::Ready(Callee::Guess(prop)),
                    },
                })
            }
            _ => None,
        }
    }

    fn type_name_callee(&self, t: &TSTypeName) -> Option<Pending> {
        match t {
            TSTypeName::IdentifierReference(id) => Some(self.ident_callee(id)),
            TSTypeName::QualifiedName(q) => {
                let prop = q.right.name.as_str().to_string();
                if let TSTypeName::IdentifierReference(o) = &q.left
                    && let Some(spec) = self.namespace_of(o)
                {
                    return Some(Pending::Ready(Callee::Import(
                        spec,
                        ImportName::named(&prop),
                    )));
                }
                Some(Pending::Ready(Callee::Guess(prop)))
            }
            _ => None,
        }
    }

    /// A URL-ish string from a literal, template or concatenation; dynamic parts become `:`.
    fn url_of(&self, e: &Expression) -> Option<String> {
        match e.get_inner_expression() {
            Expression::StringLiteral(s) => Some(s.value.as_str().to_string()),
            Expression::TemplateLiteral(t) => {
                let mut out = String::new();
                for (i, q) in t.quasis.iter().enumerate() {
                    out.push_str(q.value.raw.as_str());
                    if i < t.expressions.len() {
                        out.push(':');
                    }
                }
                Some(out)
            }
            Expression::BinaryExpression(b)
                if b.operator == oxc_syntax::operator::BinaryOperator::Addition =>
            {
                let l = self.url_of(&b.left).unwrap_or_else(|| ":".into());
                let r = self.url_of(&b.right).unwrap_or_else(|| ":".into());
                Some(l + &r)
            }
            Expression::Identifier(id) => {
                let s = id
                    .reference_id
                    .get()
                    .and_then(|r| self.scoping.get_reference(r).symbol_id())?;
                self.url_vars.get(&s).cloned()
            }
            _ => None,
        }
    }

    /// Counts callees whose first argument looks like a URL, for `ig setup` (http.calls candidates).
    fn count_url_callee(&mut self, it: &CallExpression<'a>) {
        let name = match it.callee.get_inner_expression() {
            Expression::Identifier(id) => id.name.as_str(),
            Expression::StaticMemberExpression(m) => m.property.name.as_str(),
            _ => return,
        };
        let Some(url) = it
            .arguments
            .first()
            .and_then(|a| a.as_expression())
            .and_then(|a| self.url_of(a))
        else {
            return;
        };
        if url.starts_with('/') || url.contains("://") {
            *self.url_callees.entry(name.to_string()).or_insert(0) += 1;
        }
    }

    fn http_call(&mut self, it: &CallExpression<'a>) {
        let name = match it.callee.get_inner_expression() {
            Expression::Identifier(id) => id.name.as_str(),
            Expression::StaticMemberExpression(m) => m.property.name.as_str(),
            _ => return,
        };
        if !self.http_calls.iter().any(|c| c == name) {
            return;
        }
        let Some(url) = it
            .arguments
            .first()
            .and_then(|a| a.as_expression())
            .and_then(|a| self.url_of(a))
        else {
            return;
        };
        if !url.contains('/') {
            return;
        }
        let method = it
            .arguments
            .get(1)
            .and_then(|a| match a.as_expression()?.get_inner_expression() {
                Expression::ObjectExpression(o) => o.properties.iter().find_map(|p| match p {
                    ObjectPropertyKind::ObjectProperty(p)
                        if p.key.static_name().as_deref() == Some("method") =>
                    {
                        match p.value.get_inner_expression() {
                            Expression::StringLiteral(s) => {
                                Some(s.value.as_str().to_ascii_uppercase())
                            }
                            _ => None,
                        }
                    }
                    _ => None,
                }),
                _ => None,
            })
            .unwrap_or_else(|| "GET".into());
        let line = self.line(it.span.start);
        let from = self.nodes[self.innermost_node()].id.clone();
        self.http.push(HttpCall {
            from,
            method,
            url,
            line,
        });
    }

    fn add_ref(&mut self, kind: &'static str, callee: Pending, off: u32) {
        let line = self.line(off);
        let from = self.nodes[self.innermost_node()].id.clone();
        self.refs.push((Some(from), kind, callee, line));
    }

    fn add_import(&mut self, spec: &str, off: u32, namespace: bool, reexport: bool) {
        let line = self.line(off);
        self.import_refs.push(ImportRef {
            spec: spec.into(),
            line,
            namespace,
            reexport,
        });
    }

    fn take_export_span(&mut self, decl: Span) -> Span {
        match self.export_span {
            Some(e) if e.start <= decl.start && e.end >= decl.end => {
                self.export_span = None;
                e
            }
            _ => decl,
        }
    }

    fn is_test_callee(&self, e: &Expression) -> bool {
        let is = |n: &str| self.test_calls.iter().any(|t| t == n);
        match e.get_inner_expression() {
            Expression::Identifier(id) => is(id.name.as_str()),
            Expression::StaticMemberExpression(m) => {
                matches!(m.object.get_inner_expression(), Expression::Identifier(o) if is(o.name.as_str()))
            }
            _ => false,
        }
    }

    fn class_node(&mut self, class: &Class<'a>, idx: usize) {
        let id = self.nodes[idx].id.clone();
        let ext = class.super_class.as_ref().and_then(|s| self.callee_of(s));
        if let Some(c) = &ext {
            self.refs.push((
                Some(id.clone()),
                "extends",
                c.clone(),
                self.nodes[idx].range.start_line,
            ));
        }
        for imp in &class.implements {
            if let Some(c) = self.type_name_callee(&imp.expression) {
                let line = self.line(imp.span.start);
                self.refs.push((Some(id.clone()), "implements", c, line));
            }
        }
        self.classes.push((id, ext));
    }

    /// Handles `const x = …` declarators that become nodes. Returns true if handled.
    fn declarator(&mut self, d: &VariableDeclarator<'a>, anchor: Span) -> bool {
        let BindingPattern::BindingIdentifier(b) = &d.id else {
            return false;
        };
        let Some(init) = &d.init else { return false };
        let name = b.name.as_str();
        let top = self.at_top();
        match init.get_inner_expression() {
            Expression::ArrowFunctionExpression(_) | Expression::FunctionExpression(_) => {
                let i = self.push_node("function", name, anchor, None, false);
                self.register(i, Some(b), top);
                walk::walk_variable_declarator(self, d);
                self.scopes.pop();
                true
            }
            Expression::ClassExpression(_) => {
                let i = self.push_node("class", name, anchor, None, false);
                self.register(i, Some(b), top);
                self.pending_class = Some(self.nodes[i].id.clone());
                walk::walk_variable_declarator(self, d);
                self.scopes.pop();
                true
            }
            Expression::CallExpression(c)
                if self.fn_depth == 0
                    && c.arguments
                        .first()
                        .and_then(|a| a.as_expression())
                        .is_some_and(|a| {
                            matches!(
                                a.get_inner_expression(),
                                Expression::ArrowFunctionExpression(_)
                                    | Expression::FunctionExpression(_)
                            )
                        }) =>
            {
                let i = self.push_node("function", name, anchor, None, false);
                self.register(i, Some(b), top);
                walk::walk_variable_declarator(self, d);
                self.scopes.pop();
                true
            }
            Expression::ObjectExpression(o) => {
                let parent_qual = self
                    .scopes
                    .last()
                    .map(|s| format!("{}.", s.qual))
                    .unwrap_or_default();
                self.scopes.push(Scope {
                    qual: format!("{parent_qual}{name}"),
                    node: None,
                    is_test: false,
                });
                for p in &o.properties {
                    let ObjectPropertyKind::ObjectProperty(p) = p else {
                        self.visit_object_property_kind(p);
                        continue;
                    };
                    let is_fn = matches!(
                        p.value.get_inner_expression(),
                        Expression::ArrowFunctionExpression(_) | Expression::FunctionExpression(_)
                    );
                    match (is_fn, p.key.static_name()) {
                        (true, Some(key)) => {
                            let key = key.to_string();
                            self.push_node("method", &key, p.span, None, false);
                            self.visit_object_property(p);
                            self.scopes.pop();
                        }
                        _ => self.visit_object_property(p),
                    }
                }
                self.scopes.pop();
                true
            }
            _ => false,
        }
    }
}

impl<'s, 'a> Visit<'a> for Extractor<'s, 'a> {
    fn visit_export_named_declaration(&mut self, it: &ExportNamedDeclaration<'a>) {
        if let Some(src) = &it.source {
            self.add_import(src.value.as_str(), it.span.start, false, true);
        }
        self.export_span = Some(it.span);
        walk::walk_export_named_declaration(self, it);
        self.export_span = None;
    }

    fn visit_export_all_declaration(&mut self, it: &ExportAllDeclaration<'a>) {
        self.add_import(
            it.source.value.as_str(),
            it.span.start,
            it.exported.is_some(),
            true,
        );
    }

    fn visit_import_declaration(&mut self, it: &ImportDeclaration<'a>) {
        let ns = it
            .specifiers
            .iter()
            .flatten()
            .any(|s| matches!(s, ImportDeclarationSpecifier::ImportNamespaceSpecifier(_)));
        self.add_import(it.source.value.as_str(), it.span.start, ns, false);
    }

    fn visit_import_expression(&mut self, it: &ImportExpression<'a>) {
        if let Expression::StringLiteral(s) = &it.source {
            self.add_import(s.value.as_str(), it.span.start, true, false);
        }
        walk::walk_import_expression(self, it);
    }

    fn visit_export_default_declaration(&mut self, it: &ExportDefaultDeclaration<'a>) {
        self.export_span = Some(it.span);
        let top = self.at_top();
        match &it.declaration {
            ExportDefaultDeclarationKind::ArrowFunctionExpression(_)
            | ExportDefaultDeclarationKind::FunctionExpression(_) => {
                let i = self.push_node("function", "default", it.span, None, false);
                self.register(i, None, top);
                walk::walk_export_default_declaration(self, it);
                self.scopes.pop();
            }
            ExportDefaultDeclarationKind::ClassExpression(_) => {
                let i = self.push_node("class", "default", it.span, None, false);
                self.register(i, None, top);
                self.pending_class = Some(self.nodes[i].id.clone());
                walk::walk_export_default_declaration(self, it);
                self.scopes.pop();
            }
            ExportDefaultDeclarationKind::Identifier(id) => {
                self.default_ident = Some(id.name.as_str().to_string());
                walk::walk_export_default_declaration(self, it);
            }
            _ => walk::walk_export_default_declaration(self, it),
        }
        self.export_span = None;
    }

    fn visit_variable_declaration(&mut self, it: &VariableDeclaration<'a>) {
        for d in &it.declarations {
            if let BindingPattern::BindingIdentifier(b) = &d.id {
                let t = self
                    .annotation_type(d.type_annotation.as_deref())
                    .or_else(|| self.new_type(d.init.as_ref()));
                if let (Some(sym), Some(t)) = (b.symbol_id.get(), t) {
                    self.typed.insert(sym, t);
                }
            }
        }
        {
            for d in &it.declarations {
                if let (BindingPattern::BindingIdentifier(b), Some(init)) = (&d.id, &d.init)
                    && let (Some(sym), Some(url)) = (b.symbol_id.get(), self.url_of(init))
                    && url.contains('/')
                {
                    if it.kind == VariableDeclarationKind::Const {
                        self.url_consts
                            .entry(b.name.as_str().to_string())
                            .or_insert_with(|| url.clone());
                    }
                    self.url_vars.insert(sym, url);
                }
            }
        }
        if it.kind != VariableDeclarationKind::Const || it.declare {
            return walk::walk_variable_declaration(self, it);
        }
        let single = it.declarations.len() == 1;
        let stmt = if single {
            self.take_export_span(it.span)
        } else {
            it.span
        };
        for d in &it.declarations {
            let anchor = if single { stmt } else { d.span };
            if !self.declarator(d, anchor) {
                self.visit_variable_declarator(d);
            }
        }
    }

    fn visit_function(&mut self, it: &Function<'a>, flags: ScopeFlags) {
        let is_method = self.method_value == Some(it.span);
        if is_method {
            self.method_value = None;
        } else {
            self.this_ctx.push(None);
        }
        self.fn_depth += 1;
        let decl = it.is_declaration() && it.body.is_some() && !it.declare;
        if decl {
            let anchor = self.take_export_span(it.span);
            let name = it.id.as_ref().map(|i| i.name.as_str()).unwrap_or("default");
            let top = self.at_top();
            let i = self.push_node("function", name, anchor, None, false);
            self.register(i, it.id.as_ref(), top);
            walk::walk_function(self, it, flags);
            self.scopes.pop();
        } else {
            walk::walk_function(self, it, flags);
        }
        self.fn_depth -= 1;
        if !is_method {
            self.this_ctx.pop();
        }
    }

    fn visit_class(&mut self, it: &Class<'a>) {
        if it.is_declaration() && !it.declare {
            let anchor = self.take_export_span(it.span);
            let name = it.id.as_ref().map(|i| i.name.as_str()).unwrap_or("default");
            let top = self.at_top();
            let i = self.push_node("class", name, anchor, None, false);
            self.register(i, it.id.as_ref(), top);
            self.class_node(it, i);
            let id = self.nodes[i].id.clone();
            self.collect_fields(it, &id);
            self.this_ctx.push(Some(id));
            walk::walk_class(self, it);
            self.this_ctx.pop();
            self.scopes.pop();
        } else if let Some(id) = self.pending_class.take() {
            let i = self.nodes.iter().rposition(|n| n.id == id).unwrap_or(0);
            self.class_node(it, i);
            self.collect_fields(it, &id);
            self.this_ctx.push(Some(id));
            walk::walk_class(self, it);
            self.this_ctx.pop();
        } else {
            self.this_ctx.push(None);
            walk::walk_class(self, it);
            self.this_ctx.pop();
        }
    }

    fn visit_method_definition(&mut self, it: &MethodDefinition<'a>) {
        let name = it.key.static_name();
        match (&it.value.body, name) {
            (Some(_), Some(name)) => {
                let name = name.to_string();
                self.push_node("method", &name, it.span, None, false);
                self.method_value = Some(it.value.span);
                walk::walk_method_definition(self, it);
                self.scopes.pop();
            }
            _ => {
                self.method_value = Some(it.value.span);
                walk::walk_method_definition(self, it);
            }
        }
    }

    fn visit_call_expression(&mut self, it: &CallExpression<'a>) {
        if self.is_test && self.is_test_callee(&it.callee) {
            let title = it.arguments.first().and_then(|a| match a {
                Argument::StringLiteral(s) => Some(s.value.as_str().to_string()),
                Argument::TemplateLiteral(t) if t.expressions.is_empty() => {
                    t.quasis.first().map(|q| q.value.raw.as_str().to_string())
                }
                _ => None,
            });
            if let Some(title) = title {
                let body = it.arguments.iter().skip(1).find_map(|a| {
                    match a.as_expression()?.get_inner_expression() {
                        Expression::ArrowFunctionExpression(f) => Some(&f.body),
                        Expression::FunctionExpression(f) => f.body.as_ref(),
                        _ => None,
                    }
                });
                let extra = body.map(|b| {
                    let end = b
                        .statements
                        .first()
                        .map(|s| s.span().start)
                        .unwrap_or(b.span.end);
                    Span::new(b.span.start, end)
                });
                self.push_node("test", &title, it.span, extra, true);
                walk::walk_call_expression(self, it);
                self.scopes.pop();
                return;
            }
        }
        if let Some(c) = self.callee_of(&it.callee) {
            self.add_ref("calls", c, it.span.start);
        }
        self.count_url_callee(it);
        if !self.http_calls.is_empty() {
            self.http_call(it);
        }
        walk::walk_call_expression(self, it);
    }

    fn visit_formal_parameter(&mut self, it: &FormalParameter<'a>) {
        if let (BindingPattern::BindingIdentifier(b), Some(t)) = (
            &it.pattern,
            self.annotation_type(it.type_annotation.as_deref()),
        ) && let Some(sym) = b.symbol_id.get()
        {
            self.typed.insert(sym, t);
        }
        walk::walk_formal_parameter(self, it);
    }

    /// `x.m` used as a value (`handler: controller.get.bind(controller)`) is a use of `m` too.
    fn visit_static_member_expression(&mut self, it: &StaticMemberExpression<'a>) {
        if let Some(t) = self.type_of(it.object.get_inner_expression()) {
            self.add_ref(
                "calls",
                Pending::Typed(Box::new(t), it.property.name.as_str().to_string()),
                it.span.start,
            );
        }
        walk::walk_static_member_expression(self, it);
    }

    fn visit_ts_interface_declaration(&mut self, it: &TSInterfaceDeclaration<'a>) {
        self.type_target(&it.id);
        walk::walk_ts_interface_declaration(self, it);
    }

    fn visit_ts_type_alias_declaration(&mut self, it: &TSTypeAliasDeclaration<'a>) {
        self.type_target(&it.id);
        walk::walk_ts_type_alias_declaration(self, it);
    }

    fn visit_new_expression(&mut self, it: &NewExpression<'a>) {
        if let Some(c) = self.callee_of(&it.callee) {
            self.add_ref("calls", c, it.span.start);
        }
        walk::walk_new_expression(self, it);
    }

    fn visit_jsx_opening_element(&mut self, it: &JSXOpeningElement<'a>) {
        let callee = match &it.name {
            JSXElementName::IdentifierReference(id) => Some(self.ident_callee(id)),
            JSXElementName::MemberExpression(m) => {
                let mut obj = &m.object;
                let mut prop = m.property.name.as_str();
                while let JSXMemberExpressionObject::MemberExpression(inner) = obj {
                    prop = inner.property.name.as_str();
                    obj = &inner.object;
                }
                match obj {
                    JSXMemberExpressionObject::IdentifierReference(id) => {
                        match self.namespace_of(id) {
                            Some(spec) => Some(Pending::Ready(Callee::Import(
                                spec,
                                ImportName::named(prop),
                            ))),
                            None => Some(self.ident_callee(id)),
                        }
                    }
                    _ => None,
                }
            }
            _ => None,
        };
        if let Some(c) = callee {
            self.add_ref("renders", c, it.span.start);
        }
        walk::walk_jsx_opening_element(self, it);
    }

    fn visit_if_statement(&mut self, it: &IfStatement<'a>) {
        self.bump();
        walk::walk_if_statement(self, it);
    }
    fn visit_conditional_expression(&mut self, it: &ConditionalExpression<'a>) {
        self.bump();
        walk::walk_conditional_expression(self, it);
    }
    fn visit_switch_case(&mut self, it: &SwitchCase<'a>) {
        if it.test.is_some() {
            self.bump();
        }
        walk::walk_switch_case(self, it);
    }
    fn visit_for_statement(&mut self, it: &ForStatement<'a>) {
        self.bump();
        walk::walk_for_statement(self, it);
    }
    fn visit_for_in_statement(&mut self, it: &ForInStatement<'a>) {
        self.bump();
        walk::walk_for_in_statement(self, it);
    }
    fn visit_for_of_statement(&mut self, it: &ForOfStatement<'a>) {
        self.bump();
        walk::walk_for_of_statement(self, it);
    }
    fn visit_while_statement(&mut self, it: &WhileStatement<'a>) {
        self.bump();
        walk::walk_while_statement(self, it);
    }
    fn visit_do_while_statement(&mut self, it: &DoWhileStatement<'a>) {
        self.bump();
        walk::walk_do_while_statement(self, it);
    }
    fn visit_catch_clause(&mut self, it: &CatchClause<'a>) {
        self.bump();
        walk::walk_catch_clause(self, it);
    }
    fn visit_logical_expression(&mut self, it: &LogicalExpression<'a>) {
        self.bump();
        walk::walk_logical_expression(self, it);
    }
}

fn export_tables(mr: &ModuleRecord, x: &Extractor) -> (ExportTable, Vec<String>) {
    let mut t = ExportTable::default();
    let mut local_names = vec![];
    let target_of_local = |local: &str| -> Option<ExportTarget> {
        if let Some((spec, name)) = x.imports_map.get(local) {
            return Some(match name {
                ImportName::Namespace => ExportTarget::Namespace(spec.clone()),
                n => ExportTarget::Reexport(spec.clone(), n.clone()),
            });
        }
        x.top_names
            .get(local)
            .map(|id| ExportTarget::Node(id.clone()))
    };
    for e in &mr.local_export_entries {
        let export = match &e.export_name {
            ExportExportName::Name(n) => n.name.as_str().to_string(),
            ExportExportName::Default(_) => "default".into(),
            ExportExportName::Null => continue,
        };
        let local = match &e.local_name {
            ExportLocalName::Name(n) => n.name.as_str().to_string(),
            _ => x.default_ident.clone().unwrap_or_else(|| "default".into()),
        };
        local_names.push(local.clone());
        if let Some(target) = target_of_local(&local) {
            t.names.insert(export, target);
        }
    }
    for e in &mr.indirect_export_entries {
        let Some(spec) = &e.module_request else {
            continue;
        };
        let spec = spec.name.as_str().to_string();
        let export = match &e.export_name {
            ExportExportName::Name(n) => n.name.as_str().to_string(),
            ExportExportName::Default(_) => "default".into(),
            ExportExportName::Null => continue,
        };
        let target = match &e.import_name {
            ExportImportName::Name(n) => {
                ExportTarget::Reexport(spec, ImportName::named(n.name.as_str()))
            }
            ExportImportName::All | ExportImportName::AllButDefault => {
                ExportTarget::Namespace(spec)
            }
            ExportImportName::Null => continue,
        };
        t.names.insert(export, target);
    }
    for e in &mr.star_export_entries {
        if let Some(spec) = &e.module_request {
            t.stars.push(spec.name.as_str().to_string());
        }
    }
    (t, local_names)
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
            let callee = names.iter().find_map(|n| {
                if let Some((spec, name)) = x.imports_map.get(n) {
                    return Some(Callee::Import(spec.clone(), name.clone()));
                }
                x.top_names.get(n).map(|id| Callee::Node(id.clone()))
            });
            if let Some(c) = callee {
                x.refs
                    .push((Some(input.path.to_string()), kind, Pending::Ready(c), line));
            }
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
        .map(|(from, kind, p, line)| RawRef {
            from: from
                .clone()
                .unwrap_or_else(|| innermost_at(&x.nodes, *line).to_string()),
            kind,
            callee: finish(p),
            line: *line,
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
            packages.push((name, i.line));
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

/// The innermost node (smallest line range) containing `line`; the module (index 0) otherwise.
pub fn innermost_at(nodes: &[Node], line: u32) -> &str {
    let mut best = &nodes[0];
    for n in &nodes[1..] {
        if n.range.start_line <= line && line <= n.range.end_line {
            let span = n.range.end_line - n.range.start_line;
            if span <= best.range.end_line - best.range.start_line {
                best = n;
            }
        }
    }
    &best.id
}

pub fn is_test_file(path: &str, tests: &GlobSet) -> bool {
    tests.is_match(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(path: &str, src: &str) -> FileFacts {
        let tc = vec!["describe".to_string(), "it".to_string(), "test".to_string()];
        let input = FileInput {
            path,
            source: src,
            package: Some("p"),
            embedded: false,
            is_test: path.contains(".test."),
            test_calls: &tc,
            http_calls: &[],
        };
        extract(&input, &|_| None)
    }

    fn ids(f: &FileFacts) -> Vec<&str> {
        f.nodes.iter().map(|n| n.id.as_str()).collect()
    }

    #[test]
    fn node_ids() {
        let f = run(
            "a.ts",
            "export function f() { function inner() {} }\nfunction f() {}\nconst o = { m() {}, n: () => 1, x: 1 };\nclass C { constructor() {} get v() { return 1 } set v(x) {} }\nexport default () => 1;\nconst w = wrap(() => 1);\n",
        );
        assert_eq!(
            ids(&f),
            [
                "a.ts",
                "a.ts#f",
                "a.ts#f.inner",
                "a.ts#f~2",
                "a.ts#o.m",
                "a.ts#o.n",
                "a.ts#C",
                "a.ts#C.constructor",
                "a.ts#C.v",
                "a.ts#C.v~2",
                "a.ts#default",
                "a.ts#w"
            ]
        );
        assert!(f.nodes[1].exported && !f.nodes[3].exported && f.nodes[10].exported);
        assert_eq!(f.nodes[2].parent.as_deref(), Some("a.ts#f"));
        assert_eq!(f.nodes[4].parent.as_deref(), Some("a.ts"));
    }

    #[test]
    fn test_ids_and_intents() {
        let f = run(
            "a.test.ts",
            "describe('s', () => {\n  it('x', () => {\n    // @intent A-1, B-2\n    go();\n  });\n  it('x', () => {});\n});\n",
        );
        assert_eq!(
            ids(&f),
            [
                "a.test.ts",
                "a.test.ts#s",
                "a.test.ts#s > x",
                "a.test.ts#s > x~2"
            ]
        );
        assert_eq!(f.nodes[2].intents, ["A-1", "B-2"]);
        assert_eq!(
            f.nodes[2].range,
            Range {
                start_line: 2,
                end_line: 5
            }
        );
    }

    #[test]
    fn hashes_ignore_whitespace_and_leading_comments() {
        let a = run(
            "a.ts",
            "// @intent X-1\nconst f = (a) => {\n  return a + 1;\n};\n",
        );
        let b = run("a.ts", "const f = (a) =>   {   return a + 1; };");
        assert_eq!(a.nodes[1].hash, b.nodes[1].hash);
        assert_eq!(a.nodes[1].intents, ["X-1"]);
        assert_eq!(
            a.nodes[1].range,
            Range {
                start_line: 2,
                end_line: 4
            }
        );
        let c = run("a.ts", "const f = (a) => { return a + 2; };");
        assert_ne!(a.nodes[1].hash, c.nodes[1].hash);
        assert_eq!(a.nodes[0].range.end_line, 4);
    }

    #[test]
    fn complexity_counts_innermost() {
        let f = run(
            "a.ts",
            "function f(a) { if (a && b) {} const g = () => a ?? (b ? 1 : 2); switch (a) { case 1: case 2: default: } }\n",
        );
        let by = |id: &str| {
            f.nodes
                .iter()
                .find(|n| n.id == id)
                .unwrap()
                .metrics
                .complexity
        };
        assert_eq!(by("a.ts#f"), 1 + 2 + 2);
        assert_eq!(by("a.ts#f.g"), 1 + 2);
    }

    #[test]
    fn callees() {
        let f = run(
            "a.ts",
            "import { x } from './x';\nimport * as ns from './ns';\nfunction local() {}\nfunction f(p) { local(); x(); ns.y(); p(); obj.m(); }\nclass A extends B { m() { this.n(); super.o(); } n() {} }\n",
        );
        let c: Vec<String> = f
            .refs
            .iter()
            .map(|r| format!("{} {} {:?}", r.from, r.kind, r.callee))
            .collect();
        let has = |s: &str| assert!(c.iter().any(|x| x.contains(s)), "missing {s} in {c:#?}");
        has(r#"a.ts#f calls Node("a.ts#local")"#);
        has(r#"a.ts#f calls Import("./x", Named("x"))"#);
        has(r#"a.ts#f calls Import("./ns", Named("y"))"#);
        has(r#"a.ts#f calls Guess("p")"#);
        has(r#"a.ts#f calls Guess("m")"#);
        has(r#"a.ts#A.m calls Member { class: "a.ts#A", name: "n", skip_self: false }"#);
        has(r#"a.ts#A.m calls Member { class: "a.ts#A", name: "o", skip_self: true }"#);
        has(r#"a.ts#A extends Guess("B")"#);
    }
}
