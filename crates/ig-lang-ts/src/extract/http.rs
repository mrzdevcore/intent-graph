//! HTTP client calls and URL-ish strings, for blueprint http links and `ig setup`.

use super::*;

impl<'s, 'a> Extractor<'s, 'a> {
    /// A URL-ish string from a literal, template or concatenation; dynamic parts become `:`.
    pub(super) fn url_of(&self, e: &Expression) -> Option<String> {
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
            // ponytail: a plain-function call whose first argument is a URL (`withQuery('/x', id)`)
            // is taken to decorate that URL; a helper that maps it elsewhere would mislink.
            // A later string argument means segments are appended (`join('/api', 'users', id)`):
            // no URL.
            Expression::CallExpression(c)
                if matches!(c.callee, Expression::Identifier(_))
                    && !c.arguments.iter().skip(1).any(|a| {
                        matches!(
                            a.as_expression().map(|e| e.get_inner_expression()),
                            Some(Expression::StringLiteral(_) | Expression::TemplateLiteral(_))
                        )
                    }) =>
            {
                c.arguments
                    .first()
                    .and_then(|a| a.as_expression())
                    .and_then(|a| self.url_of(a))
                    .filter(|u| u.starts_with('/') || u.contains("://"))
            }
            _ => None,
        }
    }

    /// Counts callees whose first argument looks like a URL, for `ig setup` (http.calls candidates).
    pub(super) fn count_url_callee(&mut self, it: &CallExpression<'a>) {
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

    pub(super) fn http_call(&mut self, it: &CallExpression<'a>) {
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
}
