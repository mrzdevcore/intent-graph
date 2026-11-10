//! SQL as tokens split into statements, and a cursor to read one statement with.

use super::Hints;

#[derive(Clone, Copy, PartialEq, Debug)]
pub(super) enum K {
    Word,
    Quoted,
    Str,
    Num,
    Punct,
}

#[derive(Clone, Debug)]
pub(super) struct Tok {
    pub(super) k: K,
    pub(super) s: usize,
    pub(super) e: usize,
    pub(super) line: u32,
    pub(super) text: String,
}

impl Tok {
    pub(super) fn is(&self, kw: &str) -> bool {
        self.k == K::Word && self.text.eq_ignore_ascii_case(kw)
    }

    pub(super) fn p(&self, c: &str) -> bool {
        self.k == K::Punct && self.text == c
    }

    pub(super) fn ident(&self) -> bool {
        matches!(self.k, K::Word | K::Quoted)
    }
}

/// `src` as statements of tokens, comments and the bodies of dollar-quoted strings skipped.
pub(super) fn statements(src: &str, hints: &mut Hints) -> Vec<Vec<Tok>> {
    let b = src.as_bytes();
    let backslash = src.contains('`') || src.to_ascii_uppercase().contains("AUTO_INCREMENT");
    let (mut out, mut cur, mut i, mut line) = (vec![], vec![], 0, 1u32);
    let word = |c: u8| c.is_ascii_alphanumeric() || c == b'_' || c == b'$' || c >= 0x80;
    while i < b.len() {
        let (c, start, at) = (b[i], i, line);
        let mut k = K::Punct;
        match c {
            b'\n' => {
                line += 1;
                i += 1;
                continue;
            }
            c if c.is_ascii_whitespace() => {
                i += 1;
                continue;
            }
            b'-' if b.get(i + 1) == Some(&b'-') => {
                while i < b.len() && b[i] != b'\n' {
                    i += 1;
                }
                continue;
            }
            b'/' if b.get(i + 1) == Some(&b'*') => {
                let end = src[i + 2..].find("*/").map_or(b.len(), |e| i + 2 + e + 2);
                line += src[i..end].matches('\n').count() as u32;
                i = end;
                continue;
            }
            b';' => {
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
                i += 1;
                continue;
            }
            b'\'' => {
                let esc = backslash || cur.last().is_some_and(|t: &Tok| t.is("E") && t.e == i);
                i += 1;
                while i < b.len() {
                    let doubled = b[i] == b'\'' && b.get(i + 1) == Some(&b'\'');
                    if esc && b[i] == b'\\' || doubled {
                        i += 2;
                    } else if b[i] == b'\'' {
                        break;
                    } else {
                        i += 1;
                    }
                }
                i = (i + 1).min(b.len());
                k = K::Str;
            }
            b'"' | b'`' => {
                if c == b'`' {
                    hints.mysql += 1;
                }
                i += 1;
                while i < b.len() && (b[i] != c || b.get(i + 1) == Some(&c)) {
                    i += if b[i] == c { 2 } else { 1 };
                }
                i = (i + 1).min(b.len());
                k = K::Quoted;
            }
            b'$' if src.get(i..).and_then(dollar_tag).is_some() => {
                let tag = src.get(i..).and_then(dollar_tag).unwrap_or("$$");
                hints.pg += 1;
                let body = i + tag.len();
                i = src[body..]
                    .find(tag)
                    .map_or(b.len(), |e| body + e + tag.len());
                k = K::Str;
            }
            b':' if b.get(i + 1) == Some(&b':') => {
                hints.pg += 1;
                i += 2;
            }
            c if c.is_ascii_digit() => {
                while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'.') {
                    i += 1;
                }
                k = K::Num;
            }
            c if word(c) => {
                while i < b.len() && word(b[i]) {
                    i += 1;
                }
                k = K::Word;
            }
            _ => {
                i += src
                    .get(i..)
                    .and_then(|r| r.chars().next())
                    .map_or(1, char::len_utf8)
            }
        }
        let raw = src.get(start..i.min(src.len())).unwrap_or("");
        line += raw.matches('\n').count() as u32;
        let text = match k {
            K::Quoted => {
                let q = &raw[..1];
                let inner = raw.get(1..raw.len().saturating_sub(1)).unwrap_or("");
                inner.replace(&format!("{q}{q}"), q)
            }
            _ => raw.to_string(),
        };
        cur.push(Tok {
            k,
            s: start,
            e: i.min(src.len()),
            line: at,
            text,
        });
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// The `$tag$` opening a dollar-quoted string at the start of `s`.
pub(super) fn dollar_tag(s: &str) -> Option<&str> {
    let end = s[1..].find('$')? + 2;
    let tag = &s[1..end - 1];
    let ok = tag.is_empty()
        || (tag.chars().all(|c| c.is_alphanumeric() || c == '_')
            && !tag.starts_with(|c: char| c.is_ascii_digit()));
    ok.then(|| &s[..end])
}

/// A cursor over one statement's tokens.
pub(super) struct Cur<'a> {
    pub(super) t: &'a [Tok],
    pub(super) i: usize,
}

impl<'a> Cur<'a> {
    pub(super) fn new(t: &'a [Tok]) -> Self {
        Cur { t, i: 0 }
    }

    pub(super) fn peek(&self) -> Option<&'a Tok> {
        self.t.get(self.i)
    }

    pub(super) fn at(&self, kw: &str) -> bool {
        self.peek().is_some_and(|t| t.is(kw))
    }

    pub(super) fn at_p(&self, c: &str) -> bool {
        self.peek().is_some_and(|t| t.p(c))
    }

    pub(super) fn eat(&mut self, kw: &str) -> bool {
        let ok = self.at(kw);
        self.i += ok as usize;
        ok
    }

    pub(super) fn eat_p(&mut self, c: &str) -> bool {
        let ok = self.at_p(c);
        self.i += ok as usize;
        ok
    }

    pub(super) fn eat_all(&mut self, kws: &[&str]) -> bool {
        let ok = kws
            .iter()
            .enumerate()
            .all(|(n, k)| self.t.get(self.i + n).is_some_and(|t| t.is(k)));
        if ok {
            self.i += kws.len();
        }
        ok
    }

    pub(super) fn ident(&mut self) -> Option<String> {
        let t = self.peek().filter(|t| t.ident())?;
        self.i += 1;
        Some(t.text.clone())
    }

    /// A possibly schema-qualified name, a default schema (`public`, `dbo`, `main`) dropped.
    pub(super) fn name(&mut self) -> Option<String> {
        let mut parts = vec![self.ident()?];
        while self.at_p(".") && self.t.get(self.i + 1).is_some_and(Tok::ident) {
            self.i += 1;
            parts.extend(self.ident());
        }
        Some(qualified(parts))
    }

    /// The tokens inside the parenthesis at the cursor, which moves past its closer.
    pub(super) fn group(&mut self) -> Option<&'a [Tok]> {
        if !self.at_p("(") {
            return None;
        }
        let open = self.i;
        let mut depth = 0;
        while let Some(t) = self.peek() {
            self.i += 1;
            if t.p("(") {
                depth += 1;
            } else if t.p(")") {
                depth -= 1;
                if depth == 0 {
                    return Some(&self.t[open + 1..self.i - 1]);
                }
            }
        }
        Some(&self.t[open + 1..])
    }

    /// Moves past one token, or one parenthesised group.
    pub(super) fn skip(&mut self) {
        if self.group().is_none() {
            self.i += 1;
        }
    }

    pub(super) fn rest(&self) -> &'a [Tok] {
        self.t.get(self.i..).unwrap_or(&[])
    }

    pub(super) fn done(&self) -> bool {
        self.i >= self.t.len()
    }
}

pub(super) fn qualified(mut parts: Vec<String>) -> String {
    let default = ["public", "dbo", "main"];
    if parts.len() > 1 && default.iter().any(|d| parts[0].eq_ignore_ascii_case(d)) {
        parts.remove(0);
    }
    parts.join(".")
}

/// `toks` split on commas outside parentheses and brackets.
pub(super) fn commas(toks: &[Tok]) -> Vec<&[Tok]> {
    let (mut out, mut depth, mut start) = (vec![], 0i32, 0);
    for (i, t) in toks.iter().enumerate() {
        if t.p("(") || t.p("[") {
            depth += 1;
        } else if t.p(")") || t.p("]") {
            depth -= 1;
        } else if t.p(",") && depth <= 0 {
            out.push(&toks[start..i]);
            start = i + 1;
        }
    }
    out.push(&toks[start..]);
    out.retain(|p| !p.is_empty());
    out
}

/// The source text `toks` span, whitespace collapsed.
pub(super) fn source(src: &str, toks: &[Tok]) -> String {
    match (toks.first(), toks.last()) {
        (Some(a), Some(z)) => crate::text::squash(src.get(a.s..z.e).unwrap_or("")),
        _ => String::new(),
    }
}

/// A type as written, lower case: `varchar(40)`, `numeric(10,2)`, `timestamp with time zone`.
pub(super) fn type_text(toks: &[Tok]) -> String {
    let mut out = String::new();
    let mut prev_word = false;
    for t in toks {
        let word = matches!(t.k, K::Word | K::Quoted | K::Num);
        if word && prev_word {
            out.push(' ');
        }
        out.push_str(&t.text.to_lowercase());
        prev_word = word;
    }
    out
}
