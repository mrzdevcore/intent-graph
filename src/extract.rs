//! Pass 1: per-file parse, node extraction, export tables and unresolved references.

use crate::embedded::{self, MarkupRef};
use crate::model::{Metrics, Node, Range, sha1_bytes, sha1_hex};
use globset::GlobSet;
use oxc_allocator::Allocator;
use oxc_ast::ast::*;
use oxc_ast_visit::{Visit, walk};
use oxc_semantic::{Scoping, SemanticBuilder, SymbolId};
use oxc_span::{GetSpan, SourceType, Span};
use oxc_syntax::module_record::{ExportExportName, ExportImportName, ExportLocalName, ImportImportName, ModuleRecord};
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
        if n == "default" { ImportName::Default } else { ImportName::Named(n.into()) }
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
    Member { class: String, name: String, skip_self: bool },
    /// Anything else: matched by bare name.
    Guess(String),
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
    pub is_test: bool,
}

pub struct FileInput<'c> {
    pub path: &'c str,
    pub source: &'c str,
    pub package: Option<&'c str>,
    pub embedded: bool,
    pub is_test: bool,
    pub test_calls: &'c [String],
}

pub struct Lines(Vec<usize>);

impl Lines {
    pub fn new(src: &str) -> Lines {
        let mut v = vec![0];
        v.extend(src.bytes().enumerate().filter(|(_, b)| *b == b'\n').map(|(i, _)| i + 1));
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
    default_ident: Option<String>,
    _a: std::marker::PhantomData<&'a ()>,
}

/// A callee before symbols are mapped to nodes (declarations may come later in the file).
#[derive(Clone)]
enum Pending {
    Sym(SymbolId, String),
    Ready(Callee),
}

impl<'s, 'a> Extractor<'s, 'a> {
    fn line(&self, off: u32) -> u32 {
        self.lines.line(off as usize)
    }

    fn leading_comments(&self, start: u32) -> String {
        let mut pos = start as usize;
        let mut text = String::new();
        let idx = self.comments.partition_point(|c| c.span.end as usize <= pos);
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
    fn push_node(&mut self, kind: &str, name: &str, anchor: Span, extra_comments: Option<Span>, test: bool) -> usize {
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
            for c in self.comments.iter().filter(|c| c.span.start >= sp.start && c.span.end <= sp.end) {
                intents_in(&self.src[c.span.start as usize..c.span.end as usize], &mut intents);
            }
        }
        let (start, end) = (self.line(anchor.start), self.line(anchor.end.saturating_sub(1).max(anchor.start)));
        let parent = self.nodes[self.innermost_node()].id.clone();
        let text = &self.src[anchor.start as usize..anchor.end as usize];
        self.nodes.push(Node {
            id,
            kind: kind.into(),
            name: name.into(),
            file: self.file.into(),
            range: Range { start_line: start, end_line: end },
            parent: Some(parent),
            package: self.package.map(str::to_string),
            exported: false,
            hash: normalized_hash(text),
            metrics: Metrics { loc: end - start + 1, complexity: 1 },
            intents,
        });
        let idx = self.nodes.len() - 1;
        self.scopes.push(Scope { qual, node: Some(idx), is_test: test });
        idx
    }

    fn register(&mut self, idx: usize, binding: Option<&BindingIdentifier>, top_level: bool) {
        let id = self.nodes[idx].id.clone();
        if let Some(b) = binding {
            if let Some(s) = b.symbol_id.get() {
                self.sym_node.insert(s, id.clone());
            }
        }
        if top_level {
            self.top_names.entry(self.nodes[idx].name.clone()).or_insert(id);
        }
    }

    fn at_top(&self) -> bool {
        self.scopes.is_empty()
    }

    fn ident_callee(&self, id: &IdentifierReference) -> Pending {
        let name = id.name.as_str().to_string();
        match id.reference_id.get().and_then(|r| self.scoping.get_reference(r).symbol_id()) {
            Some(s) => Pending::Sym(s, name),
            None => Pending::Ready(Callee::Guess(name)),
        }
    }

    /// The import binding `id` refers to, if any.
    fn import_of(&self, id: &IdentifierReference) -> Option<(String, ImportName)> {
        let s = id.reference_id.get().and_then(|r| self.scoping.get_reference(r).symbol_id())?;
        if self.scoping.symbol_scope_id(s) != self.scoping.root_scope_id() || !self.scoping.symbol_flags(s).is_import() {
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
                        Some(Some(c)) => Pending::Ready(Callee::Member { class: c.clone(), name: prop, skip_self: false }),
                        _ => Pending::Ready(Callee::Guess(prop)),
                    },
                    Expression::Super(_) => match self.this_ctx.last() {
                        Some(Some(c)) => Pending::Ready(Callee::Member { class: c.clone(), name: prop, skip_self: true }),
                        _ => Pending::Ready(Callee::Guess(prop)),
                    },
                    Expression::Identifier(o) => match self.namespace_of(o) {
                        Some(spec) => Pending::Ready(Callee::Import(spec, ImportName::named(&prop))),
                        None => match self.import_of(o) {
                            Some((spec, n)) => Pending::Ready(Callee::ImportMember(spec, n, prop)),
                            None => Pending::Ready(Callee::Guess(prop)),
                        },
                    },
                    _ => Pending::Ready(Callee::Guess(prop)),
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
                if let TSTypeName::IdentifierReference(o) = &q.left {
                    if let Some(spec) = self.namespace_of(o) {
                        return Some(Pending::Ready(Callee::Import(spec, ImportName::named(&prop))));
                    }
                }
                Some(Pending::Ready(Callee::Guess(prop)))
            }
            _ => None,
        }
    }

    fn add_ref(&mut self, kind: &'static str, callee: Pending, off: u32) {
        let line = self.line(off);
        self.refs.push((None, kind, callee, line));
    }

    fn add_import(&mut self, spec: &str, off: u32, namespace: bool, reexport: bool) {
        let line = self.line(off);
        self.import_refs.push(ImportRef { spec: spec.into(), line, namespace, reexport });
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
            self.refs.push((Some(id.clone()), "extends", c.clone(), self.nodes[idx].range.start_line));
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
        let BindingPattern::BindingIdentifier(b) = &d.id else { return false };
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
                    && c.arguments.first().and_then(|a| a.as_expression()).is_some_and(|a| {
                        matches!(
                            a.get_inner_expression(),
                            Expression::ArrowFunctionExpression(_) | Expression::FunctionExpression(_)
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
                let parent_qual = self.scopes.last().map(|s| format!("{}.", s.qual)).unwrap_or_default();
                self.scopes.push(Scope { qual: format!("{parent_qual}{name}"), node: None, is_test: false });
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
        self.add_import(it.source.value.as_str(), it.span.start, it.exported.is_some(), true);
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
        if it.kind != VariableDeclarationKind::Const || it.declare {
            return walk::walk_variable_declaration(self, it);
        }
        let single = it.declarations.len() == 1;
        let stmt = if single { self.take_export_span(it.span) } else { it.span };
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
            self.this_ctx.push(Some(self.nodes[i].id.clone()));
            walk::walk_class(self, it);
            self.this_ctx.pop();
            self.scopes.pop();
        } else if let Some(id) = self.pending_class.take() {
            let i = self.nodes.iter().rposition(|n| n.id == id).unwrap_or(0);
            self.class_node(it, i);
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
                let body = it.arguments.iter().skip(1).find_map(|a| match a.as_expression()?.get_inner_expression() {
                    Expression::ArrowFunctionExpression(f) => Some(&f.body),
                    Expression::FunctionExpression(f) => f.body.as_ref(),
                    _ => None,
                });
                let extra = body.map(|b| {
                    let end = b.statements.first().map(|s| s.span().start).unwrap_or(b.span.end);
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
        walk::walk_call_expression(self, it);
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
                    JSXMemberExpressionObject::IdentifierReference(id) => match self.namespace_of(id) {
                        Some(spec) => Some(Pending::Ready(Callee::Import(spec, ImportName::named(prop)))),
                        None => Some(self.ident_callee(id)),
                    },
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
        x.top_names.get(local).map(|id| ExportTarget::Node(id.clone()))
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
        let Some(spec) = &e.module_request else { continue };
        let spec = spec.name.as_str().to_string();
        let export = match &e.export_name {
            ExportExportName::Name(n) => n.name.as_str().to_string(),
            ExportExportName::Default(_) => "default".into(),
            ExportExportName::Null => continue,
        };
        let target = match &e.import_name {
            ExportImportName::Name(n) => ExportTarget::Reexport(spec, ImportName::named(n.name.as_str())),
            ExportImportName::All | ExportImportName::AllButDefault => ExportTarget::Namespace(spec),
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
    let script = if input.embedded { Some(embedded::extract_script(input.source)) } else { None };
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
        None => (input.source, SourceType::from_path(input.path).unwrap_or_else(|_| SourceType::mjs())),
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
        kind: if input.embedded { "component" } else { "module" }.into(),
        name: basename(input.path).into(),
        file: input.path.into(),
        range: Range { start_line: 1, end_line },
        parent: None,
        package: input.package.map(str::to_string),
        exported: false,
        hash: sha1_bytes(input.source.as_bytes()),
        metrics: Metrics { loc: end_line, complexity: 1 },
        intents: vec![],
    };
    let mut imports_map = HashMap::new();
    for e in &ret.module_record.import_entries {
        let name = match &e.import_name {
            ImportImportName::Name(n) => ImportName::named(n.name.as_str()),
            ImportImportName::NamespaceObject => ImportName::Namespace,
            ImportImportName::Default(_) => ImportName::Default,
        };
        imports_map.insert(e.local_name.name.as_str().to_string(), (e.module_request.name.as_str().to_string(), name));
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
                x.refs.push((Some(input.path.to_string()), kind, Pending::Ready(c), line));
            }
        }
    }

    let (exports, local_names) = export_tables(&ret.module_record, &x);
    let module_id = input.path.to_string();
    for n in x.nodes.iter_mut().skip(1) {
        if n.parent.as_deref() == Some(&module_id)
            && !matches!(n.kind.as_str(), "method" | "test")
            && local_names.contains(&n.name)
        {
            n.exported = true;
        }
    }

    let finish = |p: &Pending| -> Callee {
        match p {
            Pending::Ready(c) => c.clone(),
            Pending::Sym(s, name) => {
                if let Some(id) = x.sym_node.get(s) {
                    return Callee::Node(id.clone());
                }
                let root = scoping.symbol_scope_id(*s) == scoping.root_scope_id();
                if root && scoping.symbol_flags(*s).is_import() {
                    if let Some((spec, n)) = x.imports_map.get(name) {
                        return Callee::Import(spec.clone(), n.clone());
                    }
                }
                Callee::Guess(name.clone())
            }
        }
    };
    let refs: Vec<RawRef> = x
        .refs
        .iter()
        .map(|(from, kind, p, line)| RawRef {
            from: from.clone().unwrap_or_else(|| innermost_at(&x.nodes, *line).to_string()),
            kind,
            callee: finish(p),
            line: *line,
        })
        .collect();
    let classes = x.classes.iter().map(|(id, e)| (id.clone(), e.as_ref().map(&finish))).collect();
    let mut resolved = HashMap::new();
    for i in &x.import_refs {
        if !resolved.contains_key(&i.spec) {
            if let Some(t) = resolve(&i.spec) {
                resolved.insert(i.spec.clone(), t);
            }
        }
    }
    for (spec, _) in x.imports_map.values() {
        if !resolved.contains_key(spec) {
            if let Some(t) = resolve(spec) {
                resolved.insert(spec.clone(), t);
            }
        }
    }
    FileFacts {
        file: input.path.into(),
        nodes: x.nodes,
        parse_error,
        imports: x.import_refs,
        resolved,
        exports,
        refs,
        classes,
        script: script.map(|s| s.source).unwrap_or_default(),
        is_test: input.is_test,
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
