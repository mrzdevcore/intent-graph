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
