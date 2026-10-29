//! React function components: a PascalCase function whose body draws JSX, its props (the
//! first parameter's pattern or type), hooks, inner functions and the components it draws.

use super::*;

impl Reader<'_> {
    /// The React components a top-level statement declares, into `out`.
    pub(super) fn react_statement(&self, path: &str, s: &Statement, out: &mut Vec<Anatomy>) {
        match s {
            Statement::FunctionDeclaration(f) => self.react_function(path, f, None, out),
            Statement::VariableDeclaration(v) => self.react_variables(path, v, out),
            Statement::ExportNamedDeclaration(e) => match &e.declaration {
                Some(Declaration::FunctionDeclaration(f)) => {
                    self.react_function(path, f, None, out)
                }
                Some(Declaration::VariableDeclaration(v)) => self.react_variables(path, v, out),
                _ => {}
            },
            Statement::ExportDefaultDeclaration(d) => match &d.declaration {
                ExportDefaultDeclarationKind::FunctionDeclaration(f) => {
                    self.react_function(path, f, Some(stem(path)), out)
                }
                ExportDefaultDeclarationKind::ArrowFunctionExpression(f) => {
                    let name = stem(path).to_string();
                    self.react_body(path, &name, d.span, &f.params, &f.body.statements, out);
                }
                _ => {}
            },
            _ => {}
        }
    }

    fn react_function(&self, path: &str, f: &Function, or: Option<&str>, out: &mut Vec<Anatomy>) {
        let Some(name) = f.id.as_ref().map(|i| i.name.as_str()).or(or) else {
            return;
        };
        if let Some(b) = &f.body {
            self.react_body(path, name, f.span, &f.params, &b.statements, out);
        }
    }

    /// `const Name = (props) => …`, also wrapped in `memo(…)` or `forwardRef(…)`.
    fn react_variables(&self, path: &str, v: &VariableDeclaration, out: &mut Vec<Anatomy>) {
        for d in &v.declarations {
            let (BindingPattern::BindingIdentifier(id), Some(mut init)) = (&d.id, d.init.as_ref())
            else {
                continue;
            };
            while let Expression::CallExpression(c) = init.get_inner_expression()
                && callee(&c.callee)
                    .is_some_and(|f| ["memo", "forwardRef", "observer"].contains(&f))
                && let Some(x) = c.arguments.first().and_then(|x| x.as_expression())
            {
                init = x;
            }
            match init.get_inner_expression() {
                Expression::ArrowFunctionExpression(f) => {
                    self.react_body(path, &id.name, d.span, &f.params, &f.body.statements, out)
                }
                Expression::FunctionExpression(f) => {
                    if let Some(b) = &f.body {
                        self.react_body(path, &id.name, d.span, &f.params, &b.statements, out);
                    }
                }
                _ => {}
            }
        }
    }

    /// A component when `name` is PascalCase and its body draws JSX.
    fn react_body(
        &self,
        path: &str,
        name: &str,
        span: Span,
        params: &FormalParameters,
        body: &[Statement],
        out: &mut Vec<Anatomy>,
    ) {
        if !is_pascal(name) {
            return;
        }
        let mut jsx = Jsx {
            code: self.code,
            tags: vec![],
            seen: HashSet::new(),
            any: false,
        };
        for s in body {
            jsx.visit_statement(s);
        }
        if !jsx.any {
            return;
        }
        let mut a = Anatomy {
            framework: "react".into(),
            name: name.into(),
            file: path.into(),
            range: Range {
                start_line: self.line(span),
                end_line: self.lines.line(span.end.saturating_sub(1) as usize),
            },
            inputs: vec![],
            state: vec![],
            uses: vec![],
            functions: vec![],
            renders: (jsx.tags.iter())
                .map(|(t, at)| part(t, "tag", self.lines.line(*at as usize), None))
                .collect(),
            template: None,
        };
        if let Some(p) = params.items.first() {
            let m = match &p.pattern {
                BindingPattern::ObjectPattern(o) => o
                    .properties
                    .iter()
                    .filter_map(|x| Some((x.key.static_name()?.to_string(), None, x.span)))
                    .collect(),
                _ => p
                    .type_annotation
                    .as_ref()
                    .and_then(|t| self.type_members(&t.type_annotation))
                    .unwrap_or_default(),
            };
            self.push(&mut a.inputs, "prop", m);
        }
        for s in body {
            self.hook_statement(s, &mut a);
        }
        out.push(a);
    }

    /// A statement of a React component's body: hooks and inner functions.
    fn hook_statement(&self, s: &Statement, a: &mut Anatomy) {
        let mut one = |e: &Expression, names: Vec<String>, at: Span| {
            let line = self.line(at);
            match e.get_inner_expression() {
                Expression::ArrowFunctionExpression(_) | Expression::FunctionExpression(_) => {
                    for n in &names {
                        a.functions.push(part(n, "function", line, None));
                    }
                }
                Expression::CallExpression(c) => {
                    let Some(f) = callee(&c.callee).filter(|f| is_hook(f)) else {
                        return;
                    };
                    // `const [count, setCount] = useState(0)`: the state is the first name
                    let name = names.first().cloned().unwrap_or_else(|| f.to_string());
                    let first = c.arguments.first().map(|x| self.text(x.span()));
                    if REACT_STATE.contains(&f) {
                        a.state.push(part(&name, f, line, first));
                    } else if REACT_EFFECT.contains(&f) {
                        a.state.push(part(f, "effect", line, None));
                    } else if f == "useCallback" {
                        a.functions.push(part(&name, f, line, None));
                    } else {
                        let detail = (!names.is_empty()).then(|| names.join(", "));
                        a.uses.push(part(f, "hook", line, detail));
                    }
                }
                _ => {}
            }
        };
        match s {
            Statement::VariableDeclaration(v) => {
                for d in &v.declarations {
                    if let Some(init) = &d.init {
                        one(init, bound(&d.id), d.span);
                    }
                }
            }
            Statement::ExpressionStatement(e) => one(&e.expression, vec![], e.span),
            Statement::FunctionDeclaration(f) => {
                if let Some(id) = &f.id {
                    a.functions
                        .push(part(&id.name, "function", self.line(f.span), None));
                }
            }
            _ => {}
        }
    }
}

/// The JSX in a function body: whether there is any, and the components it draws (by tag,
/// first use, with its offset).
struct Jsx<'s> {
    code: &'s str,
    tags: Vec<(String, u32)>,
    seen: HashSet<String>,
    any: bool,
}

impl<'a> Visit<'a> for Jsx<'_> {
    fn visit_jsx_opening_element(&mut self, it: &JSXOpeningElement<'a>) {
        self.any = true;
        let s = it.name.span();
        let tag = self
            .code
            .get(s.start as usize..s.end as usize)
            .unwrap_or("");
        if (is_pascal(tag) || tag.contains('.')) && self.seen.insert(tag.to_string()) {
            self.tags.push((tag.to_string(), s.start));
        }
        walk::walk_jsx_opening_element(self, it);
    }

    fn visit_jsx_fragment(&mut self, it: &JSXFragment<'a>) {
        self.any = true;
        walk::walk_jsx_fragment(self, it);
    }
}
