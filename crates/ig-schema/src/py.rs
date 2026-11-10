//! Python read as logical lines and the classes they nest into, enough for model declarations.

use crate::text::{Lines, blank_comments, skip_string, split_top};
use regex::Regex;
use std::sync::LazyLock;

/// One logical line: continuation lines joined, comments dropped.
pub struct Line {
    pub line: u32,
    pub end: u32,
    pub indent: usize,
    pub text: String,
}

/// `text`'s logical lines, blank ones dropped.
pub fn lines(text: &str) -> Vec<Line> {
    let s = blank_comments(text, true);
    let b = s.as_bytes();
    let at = Lines::new(&s);
    let mut out = vec![];
    let (mut start, mut depth, mut i) = (0, 0usize, 0);
    let mut push = |from: usize, to: usize| {
        let raw = s.get(from..to.min(s.len())).unwrap_or("");
        let first = raw.lines().find(|l| !l.trim().is_empty()).unwrap_or("");
        let text = raw.split_whitespace().collect::<Vec<_>>().join(" ");
        let text = text.replace("\\ ", " ");
        if !text.is_empty() {
            let lead = raw.len() - raw.trim_start().len();
            out.push(Line {
                line: at.line(from + lead),
                end: at.line(to.saturating_sub(1).max(from)),
                indent: first.len() - first.trim_start().len(),
                text,
            });
        }
    };
    while i < b.len() {
        match b[i] {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth = depth.saturating_sub(1),
            b'"' | b'\'' => i = skip_string(b, i),
            b'\n' if depth == 0 && (i == 0 || b[i - 1] != b'\\') => {
                push(start, i);
                start = i + 1;
            }
            _ => {}
        }
        i += 1;
    }
    push(start, b.len());
    out
}

/// A class: its name, bases (as written) and the indices of its body's direct statements.
pub struct Class {
    pub name: String,
    pub bases: Vec<String>,
    pub indent: usize,
    pub line: u32,
    pub end: u32,
    pub body: Vec<usize>,
}

static CLASS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^class\s+(\w+)\s*(?:\((.*)\))?\s*:").unwrap());

/// Every class in `lines`, nested ones too.
pub fn classes(lines: &[Line]) -> Vec<Class> {
    let mut out = vec![];
    for (i, l) in lines.iter().enumerate() {
        let Some(c) = CLASS.captures(&l.text) else {
            continue;
        };
        let bases = c.get(2).map_or(vec![], |b| {
            split_top(b.as_str(), b',')
                .into_iter()
                .filter(|(_, b)| !b.contains('='))
                .map(|(_, b)| b.to_string())
                .collect()
        });
        let inner: Vec<usize> = (i + 1..lines.len())
            .take_while(|&j| lines[j].indent > l.indent)
            .collect();
        let body_indent = inner.first().map(|&j| lines[j].indent);
        out.push(Class {
            name: c[1].to_string(),
            bases,
            indent: l.indent,
            line: l.line,
            end: inner.last().map_or(l.end, |&j| lines[j].end),
            body: inner
                .into_iter()
                .filter(|&j| Some(lines[j].indent) == body_indent)
                .collect(),
        });
    }
    out
}

/// The class `name` nested directly in `outer`.
pub fn nested<'a>(all: &'a [Class], outer: &Class, name: &str) -> Option<&'a Class> {
    all.iter().find(|c| {
        c.name == name && c.indent > outer.indent && c.line > outer.line && c.line <= outer.end
    })
}

/// A statement split at its assignment: target (with any annotation) and value.
pub fn assign(text: &str) -> (&str, Option<&str>) {
    let b = text.as_bytes();
    let mut depth = 0i32;
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth -= 1,
            b'"' | b'\'' => i = skip_string(b, i),
            b'=' if depth == 0 => {
                let prev = if i > 0 { b[i - 1] } else { b' ' };
                if b.get(i + 1) != Some(&b'=') && !matches!(prev, b'=' | b'!' | b'<' | b'>' | b':')
                {
                    return (text[..i].trim(), Some(text[i + 1..].trim()));
                }
                i += 1;
            }
            _ => {}
        }
        i += 1;
    }
    (text.trim(), None)
}

/// A call's callee (its last dotted segment) and argument text: `models.CharField(x)`.
pub fn call(expr: &str) -> Option<(&str, &str)> {
    let open = expr.find('(')?;
    let callee = expr[..open].trim();
    if callee.is_empty()
        || !callee
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '.')
    {
        return None;
    }
    let name = callee.rsplit('.').next().unwrap_or(callee);
    Some((name, crate::text::inside(expr, open)))
}
