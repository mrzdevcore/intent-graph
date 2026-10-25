//! The visitor: node creation, scopes, references and complexity.

use super::*;

impl<'s, 'a> Extractor<'s, 'a> {
    pub(super) fn line(&self, off: u32) -> u32 {
        self.lines.line(off as usize)
    }

    pub(super) fn leading_comments(&self, start: u32) -> String {
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

    pub(super) fn innermost_node(&self) -> usize {
        self.scopes.iter().rev().find_map(|s| s.node).unwrap_or(0)
    }

    pub(super) fn bump(&mut self) {
        let i = self.innermost_node();
        self.nodes[i].metrics.complexity += 1;
    }

    /// Creates a node anchored on `anchor`, pushes its scope, and returns its index.
    pub(super) fn push_node(
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

    pub(super) fn register(
        &mut self,
        idx: usize,
        binding: Option<&BindingIdentifier>,
        top_level: bool,
    ) {
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
    pub(super) fn type_target(&mut self, id: &BindingIdentifier) {
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

    pub(super) fn at_top(&self) -> bool {
        self.scopes.is_empty()
    }

    pub(super) fn add_ref(&mut self, kind: &'static str, callee: Pending, off: u32) {
        let line = self.line(off);
        let from = self.nodes[self.innermost_node()].id.clone();
        self.refs.push((Some(from), kind, callee, line));
    }

    pub(super) fn add_import(&mut self, spec: &str, off: u32, namespace: bool, reexport: bool) {
        let line = self.line(off);
        self.import_refs.push(ImportRef {
            spec: spec.into(),
            line,
            namespace,
            reexport,
        });
    }

    pub(super) fn take_export_span(&mut self, decl: Span) -> Span {
        match self.export_span {
            Some(e) if e.start <= decl.start && e.end >= decl.end => {
                self.export_span = None;
                e
            }
            _ => decl,
        }
    }

    pub(super) fn is_test_callee(&self, e: &Expression) -> bool {
        let is = |n: &str| self.test_calls.iter().any(|t| t == n);
        match e.get_inner_expression() {
            Expression::Identifier(id) => is(id.name.as_str()),
            Expression::StaticMemberExpression(m) => {
                matches!(m.object.get_inner_expression(), Expression::Identifier(o) if is(o.name.as_str()))
            }
            _ => false,
        }
    }

    pub(super) fn class_node(&mut self, class: &Class<'a>, idx: usize) {
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
    pub(super) fn declarator(&mut self, d: &VariableDeclarator<'a>, anchor: Span) -> bool {
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
