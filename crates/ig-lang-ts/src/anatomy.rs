//! UI components as their code declares them (`Anatomy`): a single-file component's props,
//! emits, state, composables, functions and template (Vue's `<script setup>` and Options API);
//! each React function component's props, hooks, inner functions and the JSX it draws. Syntax
//! only: a props type is read when declared in the same file, nothing else is resolved.

use crate::embedded::{self, MarkupRef};
use crate::extract::{Lines, line_count};
use ig_core::model::{Anatomy, Part, Range};
use oxc_allocator::Allocator;
use oxc_ast::ast::*;
use oxc_ast_visit::{Visit, walk};
use oxc_parser::Parser;
use oxc_span::{GetSpan, SourceType, Span};
use regex::Regex;
use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

static TEMPLATE_OPEN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)<template\b[^>]*>").unwrap());
static TEMPLATE_CLOSE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)</template\s*>").unwrap());

/// The most characters of code kept as a part's detail.
const DETAIL: usize = 60;
/// Vue's reactive state calls; their result is the part's kind.
const VUE_STATE: &[&str] = &[
    "ref",
    "shallowRef",
    "reactive",
    "shallowReactive",
    "computed",
    "readonly",
    "toRef",
    "toRefs",
    "storeToRefs",
    "customRef",
];
const VUE_WATCH: &[&str] = &["watch", "watchEffect", "watchPostEffect", "watchSyncEffect"];
/// The Options API's lifecycle hooks.
const VUE_LIFECYCLE: &[&str] = &[
    "beforeCreate",
    "created",
    "beforeMount",
    "mounted",
    "beforeUpdate",
    "updated",
    "beforeUnmount",
    "unmounted",
    "beforeDestroy",
    "destroyed",
    "activated",
    "deactivated",
    "errorCaptured",
];
/// Hooks holding state, by name; `useCallback` makes a function, effects run.
const REACT_STATE: &[&str] = &["useState", "useReducer", "useMemo", "useRef"];
const REACT_EFFECT: &[&str] = &["useEffect", "useLayoutEffect", "useInsertionEffect"];

/// The components in `source` (the file at `path`): the file itself when `sfc` names its
/// single-file extension (".vue"), else each React function component of a .tsx or .jsx file.
pub fn read(path: &str, source: &str, sfc: Option<&str>) -> Vec<Anatomy> {
    let lines = Lines::new(source);
    if let Some(ext) = sfc {
        return vec![single_file(path, source, ext, &lines)];
    }
    let lower = path.to_ascii_lowercase();
    if !(lower.ends_with(".tsx") || lower.ends_with(".jsx")) {
        return vec![];
    }
    let alloc = Allocator::default();
    let st = SourceType::from_path(path).unwrap_or_else(|_| SourceType::tsx());
    let program = Parser::new(&alloc, source, st).parse().program;
    let r = Reader::new(source, &lines, &program.body);
    let mut out = vec![];
    for s in &program.body {
        r.react_statement(path, s, &mut out);
    }
    out
}

/// `name.vue` → `name`.
fn stem(path: &str) -> &str {
    let base = path.rsplit('/').next().unwrap_or(path);
    base.split_once('.').map_or(base, |p| p.0)
}

fn single_file(path: &str, source: &str, ext: &str, lines: &Lines) -> Anatomy {
    let script = embedded::extract_script(source);
    let st = if script.tsx {
        SourceType::tsx()
    } else if script.ts {
        SourceType::ts()
    } else {
        SourceType::mjs()
    };
    let alloc = Allocator::default();
    let program = Parser::new(&alloc, &script.source, st).parse().program;
    let r = Reader::new(&script.source, lines, &program.body);
    let mut a = Anatomy {
        framework: if ext.eq_ignore_ascii_case(".vue") {
            "vue".into()
        } else {
            ext.trim_start_matches('.').to_ascii_lowercase()
        },
        name: stem(path).into(),
        file: path.into(),
        range: Range {
            start_line: 1,
            end_line: line_count(source),
        },
        inputs: vec![],
        state: vec![],
        uses: vec![],
        functions: vec![],
        renders: vec![],
        template: template(source, lines),
    };
    for s in &program.body {
        r.setup_statement(s, &mut a);
    }
    let mut seen = HashSet::new();
    for (m, at) in embedded::scan_markup(source) {
        if let MarkupRef::Tag(t) = m {
            let name = embedded::pascal(&t);
            if seen.insert(name.clone()) {
                a.renders.push(part(&name, "tag", lines.line(at), None));
            }
        }
    }
    a
}

/// From the first `<template>` to the last `</template>`.
fn template(source: &str, lines: &Lines) -> Option<Range> {
    let open = TEMPLATE_OPEN.find(source)?;
    let close = TEMPLATE_CLOSE.find_iter(source).last()?;
    (close.end() > open.start()).then(|| Range {
        start_line: lines.line(open.start()),
        end_line: lines.line(close.end().saturating_sub(1)),
    })
}

fn part(name: &str, kind: &str, line: u32, detail: Option<String>) -> Part {
    Part {
        name: name.into(),
        kind: kind.into(),
        line,
        detail,
    }
}

/// The name a call is made by: `f` of `f()` and of `a.f()`.
fn callee<'a>(e: &'a Expression<'a>) -> Option<&'a str> {
    match e {
        Expression::Identifier(id) => Some(id.name.as_str()),
        Expression::StaticMemberExpression(m) => Some(m.property.name.as_str()),
        _ => None,
    }
}

fn is_pascal(name: &str) -> bool {
    name.starts_with(|c: char| c.is_ascii_uppercase())
}

/// `useThing`, not `user`.
fn is_hook(name: &str) -> bool {
    name.strip_prefix("use")
        .is_some_and(|r| r.starts_with(|c: char| c.is_ascii_uppercase()))
}

/// The names a binding pattern binds, outermost first.
fn bound(p: &BindingPattern) -> Vec<String> {
    match p {
        BindingPattern::BindingIdentifier(b) => vec![b.name.to_string()],
        BindingPattern::ObjectPattern(o) => {
            o.properties.iter().flat_map(|p| bound(&p.value)).collect()
        }
        BindingPattern::ArrayPattern(a) => a.elements.iter().flatten().flat_map(bound).collect(),
        BindingPattern::AssignmentPattern(a) => bound(&a.left),
    }
}

/// Members of a type: (name, type as written, at).
type Members = Vec<(String, Option<String>, Span)>;

struct Reader<'s> {
    code: &'s str,
    lines: &'s Lines,
    /// The file's interfaces and object type aliases, by name.
    types: HashMap<String, Members>,
}

impl<'s> Reader<'s> {
    fn new<'a>(code: &'s str, lines: &'s Lines, body: &[Statement<'a>]) -> Reader<'s> {
        let mut r = Reader {
            code,
            lines,
            types: HashMap::new(),
        };
        for s in body {
            let d = match s {
                Statement::ExportNamedDeclaration(e) => e.declaration.as_ref(),
                _ => s.as_declaration(),
            };
            match d {
                Some(Declaration::TSInterfaceDeclaration(i)) => {
                    let m = r.members(&i.body.body);
                    r.types.insert(i.id.name.to_string(), m);
                }
                Some(Declaration::TSTypeAliasDeclaration(t)) => {
                    if let TSType::TSTypeLiteral(l) = &t.type_annotation {
                        let m = r.members(&l.members);
                        r.types.insert(t.id.name.to_string(), m);
                    }
                }
                _ => {}
            }
        }
        r
    }

    fn line(&self, s: Span) -> u32 {
        self.lines.line(s.start as usize)
    }

    /// The code of `s` on one line, cut at `DETAIL` characters.
    fn text(&self, s: Span) -> String {
        let t = self
            .code
            .get(s.start as usize..s.end as usize)
            .unwrap_or("");
        let t = t.split_whitespace().collect::<Vec<_>>().join(" ");
        if t.chars().count() > DETAIL {
            format!("{}…", t.chars().take(DETAIL - 1).collect::<String>())
        } else {
            t
        }
    }

    fn members(&self, sigs: &[TSSignature]) -> Members {
        sigs.iter()
            .filter_map(|s| match s {
                TSSignature::TSPropertySignature(p) => Some((
                    format!(
                        "{}{}",
                        p.key.static_name()?,
                        if p.optional { "?" } else { "" }
                    ),
                    p.type_annotation
                        .as_ref()
                        .map(|t| self.text(t.type_annotation.span())),
                    p.span,
                )),
                TSSignature::TSMethodSignature(m) => {
                    Some((m.key.static_name()?.to_string(), None, m.span))
                }
                _ => None,
            })
            .collect()
    }

    /// The members of a type argument: a literal's, or a local interface's or alias's.
    fn type_members(&self, t: &TSType) -> Option<Members> {
        match t {
            TSType::TSTypeLiteral(l) => Some(self.members(&l.members)),
            TSType::TSTypeReference(r) => match &r.type_name {
                TSTypeName::IdentifierReference(id) => self.types.get(id.name.as_str()).cloned(),
                _ => None,
            },
            _ => None,
        }
    }

    /// The keys of `['a', 'b']` or `{ a: String }`, with the value as written.
    fn keys(&self, e: &Expression) -> Members {
        match e {
            Expression::ArrayExpression(a) => a
                .elements
                .iter()
                .filter_map(|x| match x {
                    ArrayExpressionElement::StringLiteral(s) => {
                        Some((s.value.to_string(), None, s.span))
                    }
                    _ => None,
                })
                .collect(),
            Expression::ObjectExpression(o) => o
                .properties
                .iter()
                .filter_map(|p| match p {
                    ObjectPropertyKind::ObjectProperty(p) => Some((
                        p.key.static_name()?.to_string(),
                        (!p.method && !p.shorthand).then(|| self.text(p.value.span())),
                        p.span,
                    )),
                    ObjectPropertyKind::SpreadProperty(_) => None,
                })
                .collect(),
            _ => vec![],
        }
    }

    fn push(&self, to: &mut Vec<Part>, kind: &str, m: Members) {
        for (name, detail, at) in m {
            to.push(part(&name, kind, self.line(at), detail));
        }
    }

    /// A top-level statement of a single-file component's script, or of a `setup()` body.
    fn setup_statement(&self, s: &Statement, a: &mut Anatomy) {
        match s {
            Statement::VariableDeclaration(v) => self.declaration(v, a),
            Statement::ExportNamedDeclaration(e) => {
                if let Some(Declaration::VariableDeclaration(v)) = &e.declaration {
                    self.declaration(v, a);
                } else if let Some(Declaration::FunctionDeclaration(f)) = &e.declaration {
                    self.function(f, a);
                }
            }
            Statement::FunctionDeclaration(f) => self.function(f, a),
            Statement::ExpressionStatement(e) => self.value(&e.expression, &[], s.span(), a),
            Statement::ExportDefaultDeclaration(d) => match &d.declaration {
                ExportDefaultDeclarationKind::ObjectExpression(o) => self.options(o, a),
                ExportDefaultDeclarationKind::CallExpression(c) => {
                    if let Some(Argument::ObjectExpression(o)) = c.arguments.first() {
                        self.options(o, a);
                    }
                }
                _ => {}
            },
            _ => {}
        }
    }

    fn function(&self, f: &Function, a: &mut Anatomy) {
        if let Some(id) = &f.id {
            a.functions
                .push(part(&id.name, "function", self.line(f.span), None));
        }
    }

    fn declaration(&self, v: &VariableDeclaration, a: &mut Anatomy) {
        for d in &v.declarations {
            if let Some(init) = &d.init {
                self.value(init, &bound(&d.id), d.span, a);
            }
        }
    }

    /// What `e`, bound to `names`, declares.
    fn value(&self, e: &Expression, names: &[String], at: Span, a: &mut Anatomy) {
        let e = match e.get_inner_expression() {
            Expression::AwaitExpression(w) => w.argument.get_inner_expression(),
            e => e,
        };
        let line = self.line(at);
        match e {
            Expression::ArrowFunctionExpression(_) | Expression::FunctionExpression(_) => {
                for n in names {
                    a.functions.push(part(n, "function", line, None));
                }
            }
            Expression::CallExpression(c) => {
                let Some(f) = callee(&c.callee) else { return };
                let first = c.arguments.first().and_then(|x| x.as_expression());
                let types = c.type_arguments.as_ref().and_then(|t| t.params.first());
                match f {
                    "withDefaults" => {
                        if let Some(inner) = first {
                            self.value(inner, names, at, a);
                        }
                    }
                    "defineProps" | "defineSlots" => {
                        let kind = if f == "defineProps" { "prop" } else { "slot" };
                        let m = types
                            .and_then(|t| self.type_members(t))
                            .or_else(|| first.map(|e| self.keys(e)))
                            .unwrap_or_default();
                        self.push(&mut a.inputs, kind, m);
                    }
                    "defineEmits" => {
                        let m = match types {
                            Some(TSType::TSTypeLiteral(l)) => self.emits(&l.members),
                            Some(t) => self.type_members(t).unwrap_or_default(),
                            None => first.map(|e| self.keys(e)).unwrap_or_default(),
                        };
                        self.push(&mut a.inputs, "emit", m);
                    }
                    "defineModel" => {
                        let name = match first {
                            Some(Expression::StringLiteral(s)) => s.value.as_str(),
                            _ => "modelValue",
                        };
                        let detail = types.map(|t| self.text(t.span()));
                        a.inputs.push(part(name, "model", line, detail));
                    }
                    f if VUE_STATE.contains(&f) => {
                        let detail = first.map(|e| self.text(e.span()));
                        for n in names {
                            a.state.push(part(n, f, line, detail.clone()));
                        }
                    }
                    f if VUE_WATCH.contains(&f) => {
                        let name = first.map_or(f.to_string(), |e| match e {
                            Expression::ArrowFunctionExpression(_)
                            | Expression::FunctionExpression(_)
                                if f != "watch" =>
                            {
                                f.to_string()
                            }
                            e => self.text(e.span()),
                        });
                        a.state.push(part(&name, "watch", line, None));
                    }
                    f if is_hook(f) || f == "inject" => {
                        let detail = (!names.is_empty()).then(|| names.join(", "));
                        a.uses.push(part(f, "composable", line, detail));
                    }
                    f if f.starts_with("on")
                        && f[2..].starts_with(|c: char| c.is_ascii_uppercase()) =>
                    {
                        a.functions.push(part(f, "lifecycle", line, None));
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }

    /// `{ (e: 'save', id: number): void }` or `{ save: [id: number] }`: the event names.
    fn emits(&self, sigs: &[TSSignature]) -> Members {
        let mut out = vec![];
        for s in sigs {
            match s {
                TSSignature::TSCallSignatureDeclaration(c) => {
                    let ty = c
                        .params
                        .items
                        .first()
                        .and_then(|p| p.type_annotation.as_ref());
                    if let Some(TSType::TSLiteralType(l)) = ty.map(|t| &t.type_annotation)
                        && let TSLiteral::StringLiteral(s) = &l.literal
                    {
                        let rest: Vec<String> = (c.params.items.iter().skip(1))
                            .map(|p| self.text(p.span))
                            .collect();
                        let detail = (!rest.is_empty()).then(|| rest.join(", "));
                        out.push((s.value.to_string(), detail, c.span));
                    }
                }
                TSSignature::TSPropertySignature(p) => {
                    if let Some(name) = p.key.static_name() {
                        let detail = p
                            .type_annotation
                            .as_ref()
                            .map(|t| self.text(t.type_annotation.span()));
                        out.push((name.to_string(), detail, p.span));
                    }
                }
                _ => {}
            }
        }
        out
    }

    /// The Options API: `export default { props, emits, data, computed, watch, methods, setup… }`.
    fn options(&self, o: &ObjectExpression, a: &mut Anatomy) {
        for p in &o.properties {
            let ObjectPropertyKind::ObjectProperty(p) = p else {
                continue;
            };
            let Some(key) = p.key.static_name() else {
                continue;
            };
            let body = match &p.value {
                Expression::FunctionExpression(f) => f.body.as_ref().map(|b| &b.statements),
                Expression::ArrowFunctionExpression(f) => Some(&f.body.statements),
                _ => None,
            };
            match key.as_ref() {
                "props" => self.push(&mut a.inputs, "prop", self.keys(&p.value)),
                "emits" => self.push(&mut a.inputs, "emit", self.keys(&p.value)),
                "inject" => self.push(&mut a.uses, "inject", self.keys(&p.value)),
                "data" => {
                    let ret = body.into_iter().flatten().find_map(|s| match s {
                        Statement::ReturnStatement(r) => r.argument.as_ref(),
                        _ => None,
                    });
                    if let Some(r) = ret {
                        self.push(&mut a.state, "data", self.keys(r.get_inner_expression()));
                    }
                }
                "computed" | "watch" | "methods" => {
                    let (to, kind) = match key.as_ref() {
                        "computed" => (&mut a.state, "computed"),
                        "watch" => (&mut a.state, "watch"),
                        _ => (&mut a.functions, "method"),
                    };
                    let m: Members = self
                        .keys(&p.value)
                        .into_iter()
                        .map(|(n, _, s)| (n, None, s))
                        .collect();
                    self.push(to, kind, m);
                    self.spreads(&p.value, a);
                }
                "setup" => {
                    for s in body.into_iter().flatten() {
                        self.setup_statement(s, a);
                    }
                }
                k if VUE_LIFECYCLE.contains(&k) => {
                    a.functions
                        .push(part(k, "lifecycle", self.line(p.span), None));
                }
                _ => {}
            }
        }
    }

    /// `...mapGetters(['a'])` and the like: the store helpers an options object uses.
    fn spreads(&self, e: &Expression, a: &mut Anatomy) {
        let Expression::ObjectExpression(o) = e else {
            return;
        };
        for p in &o.properties {
            if let ObjectPropertyKind::SpreadProperty(s) = p
                && let Expression::CallExpression(c) = &s.argument
                && let Some(f) = callee(&c.callee)
            {
                let args: Vec<String> = c.arguments.iter().map(|x| self.text(x.span())).collect();
                a.uses
                    .push(part(f, "store", self.line(s.span), Some(args.join(", "))));
            }
        }
    }
}

mod react;
#[cfg(test)]
mod tests;
