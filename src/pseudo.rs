//! Pseudo-code: a short, readable rendering of a function body.
//!
//! `if (!x) throw new E(msg)` becomes `require x else E "msg"`; `await`, `this.`, logging, types and
//! destructuring plumbing are dropped; object arguments shrink to their keys and callbacks to `fn(…)`.

use oxc_ast::ast::*;
use oxc_span::GetSpan;
use oxc_syntax::operator::{BinaryOperator, LogicalOperator, UnaryOperator};

const MAX_LINES: usize = 40;
const MAX_EXPR: usize = 110;

pub struct Printer<'s> {
    src: &'s str,
    out: Vec<String>,
}

fn is_logging(e: &Expression) -> bool {
    let Expression::CallExpression(c) = e.get_inner_expression() else {
        return false;
    };
    let Expression::StaticMemberExpression(m) = c.callee.get_inner_expression() else {
        return false;
    };
    let obj = match m.object.get_inner_expression() {
        Expression::Identifier(i) => i.name.as_str().to_string(),
        Expression::StaticMemberExpression(o) => o.property.name.as_str().to_string(),
        _ => return false,
    };
    let o = obj.to_ascii_lowercase();
    (o == "console" || o.contains("logger") || o == "log")
        && matches!(
            m.property.name.as_str(),
            "log" | "info" | "warn" | "error" | "debug" | "trace"
        )
}

impl<'s> Printer<'s> {
    pub fn new(src: &'s str) -> Printer<'s> {
        Printer { src, out: vec![] }
    }

    fn text(&self, span: oxc_span::Span) -> String {
        let t = &self.src[span.start as usize..span.end as usize];
        t.split_whitespace().collect::<Vec<_>>().join(" ")
    }

    fn short(s: String) -> String {
        if s.chars().count() <= MAX_EXPR {
            s
        } else {
            s.chars().take(MAX_EXPR - 1).collect::<String>() + "…"
        }
    }

    fn args(&self, args: &[Argument]) -> String {
        args.iter()
            .map(|a| match a.as_expression() {
                Some(e) => self.expr(e),
                None => self.text(a.span()),
            })
            .collect::<Vec<_>>()
            .join(", ")
    }

    pub fn expr(&self, e: &Expression) -> String {
        match e {
            Expression::AwaitExpression(a) => self.expr(&a.argument),
            Expression::ParenthesizedExpression(p) => self.expr(&p.expression),
            Expression::TSAsExpression(x) => self.expr(&x.expression),
            Expression::TSSatisfiesExpression(x) => self.expr(&x.expression),
            Expression::TSNonNullExpression(x) => self.expr(&x.expression),
            Expression::TSTypeAssertion(x) => self.expr(&x.expression),
            Expression::TSInstantiationExpression(x) => self.expr(&x.expression),
            Expression::ThisExpression(_) => "this".into(),
            Expression::StaticMemberExpression(m) => match m.object.get_inner_expression() {
                Expression::ThisExpression(_) => m.property.name.as_str().to_string(),
                o => format!("{}.{}", self.expr(o), m.property.name.as_str()),
            },
            Expression::CallExpression(c) => {
                format!("{}({})", self.expr(&c.callee), self.args(&c.arguments))
            }
            Expression::NewExpression(n) => {
                format!("new {}({})", self.expr(&n.callee), self.args(&n.arguments))
            }
            Expression::ObjectExpression(o) => {
                let keys: Vec<String> = o
                    .properties
                    .iter()
                    .map(|p| match p {
                        ObjectPropertyKind::ObjectProperty(p) => p
                            .key
                            .static_name()
                            .map(|k| k.to_string())
                            .unwrap_or_else(|| "…".into()),
                        ObjectPropertyKind::SpreadProperty(s) => {
                            format!("...{}", self.expr(&s.argument))
                        }
                    })
                    .collect();
                format!("{{{}}}", keys.join(", "))
            }
            Expression::ArrowFunctionExpression(f) => format!("fn({})", self.params(&f.params)),
            Expression::FunctionExpression(f) => format!("fn({})", self.params(&f.params)),
            Expression::UnaryExpression(u) if u.operator == UnaryOperator::LogicalNot => {
                format!("not {}", self.atom(&u.argument))
            }
            Expression::LogicalExpression(l) => {
                let op = match l.operator {
                    LogicalOperator::And => "and",
                    LogicalOperator::Or => "or",
                    LogicalOperator::Coalesce => "??",
                };
                format!("{} {op} {}", self.expr(&l.left), self.expr(&l.right))
            }
            Expression::BinaryExpression(b) => {
                let op = match b.operator {
                    BinaryOperator::StrictEquality | BinaryOperator::Equality => "==",
                    BinaryOperator::StrictInequality | BinaryOperator::Inequality => "!=",
                    o => o.as_str(),
                };
                format!("{} {op} {}", self.expr(&b.left), self.expr(&b.right))
            }
            Expression::ConditionalExpression(c) => {
                format!(
                    "{} ? {} : {}",
                    self.expr(&c.test),
                    self.expr(&c.consequent),
                    self.expr(&c.alternate)
                )
            }
            Expression::AssignmentExpression(a) => {
                format!("{} = {}", self.text(a.left.span()), self.expr(&a.right))
            }
            Expression::ChainExpression(c) => {
                self.text(c.span).replace("this.", "").replace("await ", "")
            }
            _ => self
                .text(e.span())
                .replace("this.", "")
                .replace("await ", ""),
        }
    }

    fn atom(&self, e: &Expression) -> String {
        let s = self.expr(e);
        if matches!(
            e.get_inner_expression(),
            Expression::LogicalExpression(_)
                | Expression::BinaryExpression(_)
                | Expression::ConditionalExpression(_)
        ) {
            format!("({s})")
        } else {
            s
        }
    }

    /// `not x` → `x`, otherwise `not (x)`.
    fn negate(&self, e: &Expression) -> String {
        match e.get_inner_expression() {
            Expression::UnaryExpression(u) if u.operator == UnaryOperator::LogicalNot => {
                self.expr(&u.argument)
            }
            Expression::LogicalExpression(l) if l.operator == LogicalOperator::Or => {
                format!("{} and {}", self.negate(&l.left), self.negate(&l.right))
            }
            Expression::BinaryExpression(b) => {
                let op = match b.operator {
                    BinaryOperator::StrictEquality | BinaryOperator::Equality => Some("!="),
                    BinaryOperator::StrictInequality | BinaryOperator::Inequality => Some("=="),
                    BinaryOperator::LessThan => Some(">="),
                    BinaryOperator::LessEqualThan => Some(">"),
                    BinaryOperator::GreaterThan => Some("<="),
                    BinaryOperator::GreaterEqualThan => Some("<"),
                    _ => None,
                };
                match op {
                    Some(op) => format!("{} {op} {}", self.expr(&b.left), self.expr(&b.right)),
                    None => format!("not ({})", self.expr(e)),
                }
            }
            x => format!("not {}", self.atom(x)),
        }
    }

    fn params(&self, p: &FormalParameters) -> String {
        p.items
            .iter()
            .map(|i| match &i.pattern {
                BindingPattern::BindingIdentifier(b) => b.name.as_str().to_string(),
                BindingPattern::ObjectPattern(o) => {
                    let ks: Vec<String> = o
                        .properties
                        .iter()
                        .filter_map(|p| p.key.static_name().map(|k| k.to_string()))
                        .collect();
                    ks.join(", ")
                }
                other => self.text(other.span()),
            })
            .collect::<Vec<_>>()
            .join(", ")
    }

    fn throw_text(&self, e: &Expression) -> String {
        match e.get_inner_expression() {
            Expression::NewExpression(n) => {
                let name = self.expr(&n.callee);
                match n.arguments.first().and_then(|a| a.as_expression()) {
                    Some(Expression::StringLiteral(s)) => {
                        format!("{name} \"{}\"", s.value.as_str())
                    }
                    Some(Expression::TemplateLiteral(t)) => format!("{name} {}", self.text(t.span)),
                    _ => name,
                }
            }
            x => self.expr(x),
        }
    }

    fn line(&mut self, depth: usize, s: String) {
        if self.out.len() < MAX_LINES {
            self.out
                .push(format!("{}{}", "  ".repeat(depth), Self::short(s)));
        } else if self.out.len() == MAX_LINES {
            self.out.push(format!("{}…", "  ".repeat(depth)));
        }
    }

    /// The single statement of `s`, ignoring logging (so `if (!x) { log; throw }` is still a guard).
    fn single<'b>(s: &'b Statement<'b>) -> Option<&'b Statement<'b>> {
        match s {
            Statement::BlockStatement(b) => {
                let mut real = b.body.iter().filter(|st| !matches!(st, Statement::ExpressionStatement(e) if is_logging(&e.expression)));
                let first = real.next()?;
                real.next().is_none().then_some(first)
            }
            s => Some(s),
        }
    }

    fn stmts(&mut self, body: &[Statement], depth: usize) {
        for s in body {
            self.stmt(s, depth);
        }
    }

    fn block(&mut self, s: &Statement, depth: usize) {
        match s {
            Statement::BlockStatement(b) => self.stmts(&b.body, depth),
            s => self.stmt(s, depth),
        }
    }

    pub fn stmt(&mut self, s: &Statement, depth: usize) {
        match s {
            Statement::ExpressionStatement(e) => {
                if !is_logging(&e.expression) {
                    let t = self.expr(&e.expression);
                    self.line(depth, t);
                }
            }
            Statement::VariableDeclaration(v) => {
                for d in &v.declarations {
                    let Some(init) = &d.init else { continue };
                    let lhs = match &d.id {
                        BindingPattern::BindingIdentifier(b) => b.name.as_str().to_string(),
                        BindingPattern::ObjectPattern(o) => {
                            // Unpacking parameters is plumbing; unpacking a call result is not.
                            if matches!(init.get_inner_expression(), Expression::Identifier(_)) {
                                continue;
                            }
                            o.properties
                                .iter()
                                .filter_map(|p| p.key.static_name().map(|k| k.to_string()))
                                .collect::<Vec<_>>()
                                .join(", ")
                        }
                        other => self.text(other.span()),
                    };
                    let rhs = self.expr(init);
                    self.line(depth, format!("{lhs} = {rhs}"));
                }
            }
            Statement::IfStatement(i) => {
                let guard = Self::single(&i.consequent);
                match (guard, &i.alternate) {
                    (Some(Statement::ThrowStatement(t)), None) => {
                        let s = format!(
                            "require {} else {}",
                            self.negate(&i.test),
                            self.throw_text(&t.argument)
                        );
                        self.line(depth, s);
                    }
                    (Some(Statement::BreakStatement(_)), None)
                    | (Some(Statement::ContinueStatement(_)), None) => {
                        let what = if matches!(guard, Some(Statement::BreakStatement(_))) {
                            "break"
                        } else {
                            "continue"
                        };
                        let s = format!("if {} → {what}", self.expr(&i.test));
                        self.line(depth, s);
                    }
                    (Some(Statement::ReturnStatement(r)), None) => {
                        let v = r
                            .argument
                            .as_ref()
                            .map(|a| format!(" {}", self.expr(a)))
                            .unwrap_or_default();
                        let s = format!("if {} → return{v}", self.expr(&i.test));
                        self.line(depth, s);
                    }
                    _ => {
                        let s = format!("if {}:", self.expr(&i.test));
                        self.line(depth, s);
                        self.block(&i.consequent, depth + 1);
                        let mut alt = i.alternate.as_ref();
                        while let Some(a) = alt {
                            if let Statement::IfStatement(e) = a {
                                let s = format!("else if {}:", self.expr(&e.test));
                                self.line(depth, s);
                                self.block(&e.consequent, depth + 1);
                                alt = e.alternate.as_ref();
                            } else {
                                self.line(depth, "else:".into());
                                self.block(a, depth + 1);
                                alt = None;
                            }
                        }
                    }
                }
            }
            Statement::ReturnStatement(r) => {
                let s = match &r.argument {
                    Some(a) => format!("return {}", self.expr(a)),
                    None => "return".into(),
                };
                self.line(depth, s);
            }
            Statement::ThrowStatement(t) => {
                let s = format!("throw {}", self.throw_text(&t.argument));
                self.line(depth, s);
            }
            Statement::ForOfStatement(f) => {
                let s = format!(
                    "for each {} in {}:",
                    self.text(f.left.span())
                        .trim_start_matches("const ")
                        .trim_start_matches("let "),
                    self.expr(&f.right)
                );
                self.line(depth, s);
                self.block(&f.body, depth + 1);
            }
            Statement::ForInStatement(f) => {
                let s = format!(
                    "for each key {} in {}:",
                    self.text(f.left.span())
                        .trim_start_matches("const ")
                        .trim_start_matches("let "),
                    self.expr(&f.right)
                );
                self.line(depth, s);
                self.block(&f.body, depth + 1);
            }
            Statement::ForStatement(f) => {
                let s = format!(
                    "loop {}:",
                    self.text(Span::new(f.span.start + 3, f.body.span().start))
                        .trim()
                );
                self.line(depth, s);
                self.block(&f.body, depth + 1);
            }
            Statement::WhileStatement(w) => {
                let s = format!("while {}:", self.expr(&w.test));
                self.line(depth, s);
                self.block(&w.body, depth + 1);
            }
            Statement::DoWhileStatement(w) => {
                self.line(depth, "repeat:".into());
                self.block(&w.body, depth + 1);
                let s = format!("while {}", self.expr(&w.test));
                self.line(depth, s);
            }
            Statement::TryStatement(t) => {
                self.line(depth, "try:".into());
                self.stmts(&t.block.body, depth + 1);
                if let Some(h) = &t.handler {
                    self.line(depth, "on error:".into());
                    self.stmts(&h.body.body, depth + 1);
                }
                if let Some(f) = &t.finalizer {
                    self.line(depth, "finally:".into());
                    self.stmts(&f.body, depth + 1);
                }
            }
            Statement::SwitchStatement(sw) => {
                let s = format!("switch {}:", self.expr(&sw.discriminant));
                self.line(depth, s);
                for c in &sw.cases {
                    let s = match &c.test {
                        Some(t) => format!("case {}:", self.expr(t)),
                        None => "default:".into(),
                    };
                    self.line(depth + 1, s);
                    for st in c
                        .consequent
                        .iter()
                        .filter(|st| !matches!(st, Statement::BreakStatement(_)))
                    {
                        self.stmt(st, depth + 2);
                    }
                }
            }
            Statement::BlockStatement(b) => self.stmts(&b.body, depth),
            Statement::FunctionDeclaration(f) => {
                let s = format!(
                    "fn {}({})",
                    f.id.as_ref().map(|i| i.name.as_str()).unwrap_or(""),
                    self.params(&f.params)
                );
                self.line(depth, s);
            }
            Statement::EmptyStatement(_)
            | Statement::BreakStatement(_)
            | Statement::ContinueStatement(_) => {}
            s if s.is_declaration() => {}
            s => {
                let t = self.text(s.span());
                self.line(depth, t);
            }
        }
    }

    /// The pseudo-code of a function: a signature line, then its body.
    pub fn function(
        mut self,
        name: &str,
        params: &FormalParameters,
        body: Option<&FunctionBody>,
        expr_body: bool,
    ) -> String {
        let sig = format!("{name}({})", self.params(params));
        self.out.push(sig);
        if let Some(b) = body {
            if expr_body {
                if let Some(Statement::ExpressionStatement(e)) = b.statements.first() {
                    let s = format!("return {}", self.expr(&e.expression));
                    self.line(1, s);
                }
            } else {
                self.stmts(&b.statements, 1);
            }
        }
        self.out.join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxc_allocator::Allocator;
    use oxc_span::SourceType;

    fn pseudo(src: &str) -> String {
        let alloc = Allocator::default();
        let p = oxc_parser::Parser::new(&alloc, src, SourceType::ts())
            .parse()
            .program;
        let Statement::ClassDeclaration(c) = &p.body[0] else {
            panic!()
        };
        let ClassElement::MethodDefinition(m) = &c.body.body[0] else {
            panic!()
        };
        Printer::new(src).function("execute", &m.value.params, m.value.body.as_deref(), false)
    }

    #[test]
    fn guards_become_require() {
        let src = "class U { async execute({ id, ctx }: P): Promise<void> {
            const { a, b } = p;
            const ok = await this.repo.hasAccess(id, ctx.userId)
            if (!ok) { throw new ForbiddenError() }
            const role = await this.access.resolveLineRole(ctx.userId, id)
            if (!role || role.level >= ROLE.STAFF) throw new ForbiddenError('Insufficient role')
            if (!member) { this.logger.warn(`nope`); throw new ForbiddenError() }
            this.logger.warn(`nope`)
            if (x === 1) return
            await this.store.save({ prdLine: role.line, value, by: ctx.userId })
            for (const item of items) { if (item.ok) { done(item) } else { skip(item) } }
        } }";
        let p = pseudo(src);
        assert_eq!(
            p,
            "execute(id, ctx)
  ok = repo.hasAccess(id, ctx.userId)
  require ok else ForbiddenError
  role = access.resolveLineRole(ctx.userId, id)
  require role and role.level < ROLE.STAFF else ForbiddenError \"Insufficient role\"
  require member else ForbiddenError
  if x == 1 → return
  store.save({prdLine, value, by})
  for each item in items:
    if item.ok:
      done(item)
    else:
      skip(item)"
        );
    }
}
