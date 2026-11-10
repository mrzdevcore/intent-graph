//! Text helpers the model readers share: line numbers, brackets and string literals, comments.

/// Where each line of a text starts, to turn a byte offset into a 1-based line.
pub struct Lines(Vec<usize>);

impl Lines {
    pub fn new(text: &str) -> Self {
        let starts = std::iter::once(0).chain(text.match_indices('\n').map(|(i, _)| i + 1));
        Lines(starts.collect())
    }

    pub fn line(&self, at: usize) -> u32 {
        self.0.partition_point(|&s| s <= at) as u32
    }
}

/// The index of the quote closing the string literal opening at `i` (a triple quote's last), or
/// the text's length when it never closes.
pub fn skip_string(b: &[u8], i: usize) -> usize {
    let q = b[i];
    if b.get(i + 1) == Some(&q) && b.get(i + 2) == Some(&q) {
        let mut j = i + 3;
        while j + 2 < b.len() && !(b[j] == q && b[j + 1] == q && b[j + 2] == q) {
            j += if b[j] == b'\\' { 2 } else { 1 };
        }
        return (j + 2).min(b.len());
    }
    let mut j = i + 1;
    while j < b.len() && b[j] != q {
        j += if b[j] == b'\\' { 2 } else { 1 };
    }
    j.min(b.len())
}

/// The index of the bracket closing the one at `open`, string literals skipped.
pub fn close(s: &str, open: usize) -> Option<usize> {
    let b = s.as_bytes();
    let mut depth = 0usize;
    let mut i = open;
    while i < b.len() {
        match b[i] {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(i);
                }
            }
            b'"' | b'\'' | b'`' => i = skip_string(b, i),
            _ => {}
        }
        i += 1;
    }
    None
}

/// The text between the bracket at `open` and its closer (to the end when it never closes).
pub fn inside(s: &str, open: usize) -> &str {
    let end = close(s, open).unwrap_or(s.len());
    s.get(open + 1..end).unwrap_or("")
}

/// `s` split on `sep` outside brackets and strings: each piece trimmed, with its offset.
pub fn split_top(s: &str, sep: u8) -> Vec<(usize, &str)> {
    let b = s.as_bytes();
    let mut out = vec![];
    let mut push = |from: usize, to: usize| {
        let piece = s.get(from..to.min(s.len())).unwrap_or("");
        let lead = piece.len() - piece.trim_start().len();
        if !piece.trim().is_empty() {
            out.push((from + lead, piece.trim()));
        }
    };
    let (mut depth, mut start, mut i) = (0i32, 0, 0);
    while i < b.len() {
        match b[i] {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth -= 1,
            b'"' | b'\'' | b'`' => i = skip_string(b, i),
            c if c == sep && depth <= 0 => {
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

/// A string literal's contents (`"x"`, `'x'`, `` `x` ``).
pub fn unquote(s: &str) -> Option<&str> {
    let s = s.trim();
    let q = s.chars().next()?;
    (matches!(q, '"' | '\'' | '`') && s.len() >= 2 && s.ends_with(q)).then(|| &s[1..s.len() - 1])
}

/// A `key=value` keyword argument (Python) split, `==` not counted.
pub fn kwarg(piece: &str) -> Option<(&str, &str)> {
    let (k, v) = piece.split_once('=')?;
    let k = k.trim();
    (!v.starts_with('=') && !k.is_empty() && k.chars().all(|c| c.is_alphanumeric() || c == '_'))
        .then(|| (k, v.trim()))
}

/// A `key: value` object entry (JavaScript) split, a quoted key unquoted.
pub fn entry(piece: &str) -> Option<(&str, &str)> {
    let (k, v) = piece.split_once(':')?;
    let k = unquote(k).unwrap_or(k.trim());
    (!k.is_empty()
        && k.chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '$'))
    .then(|| (k, v.trim()))
}

/// The value of `key` among a call's comma-separated arguments, as `kwarg` or `entry` reads them.
pub fn arg<'a>(
    args: &[(usize, &'a str)],
    key: &str,
    split: fn(&'a str) -> Option<(&'a str, &'a str)>,
) -> Option<&'a str> {
    args.iter()
        .filter_map(|(_, a)| split(a))
        .find(|(k, _)| *k == key)
        .map(|(_, v)| v)
}

/// `CamelCase` as `camel_case`.
pub fn snake(s: &str) -> String {
    let c: Vec<char> = s.chars().collect();
    let mut out = String::new();
    for (i, &ch) in c.iter().enumerate() {
        if ch.is_uppercase() && i > 0 {
            let prev = c[i - 1];
            let next_lower = c.get(i + 1).is_some_and(|n| n.is_lowercase());
            if prev.is_lowercase() || prev.is_ascii_digit() || (prev.is_uppercase() && next_lower) {
                out.push('_');
            }
        }
        out.extend(ch.to_lowercase());
    }
    out
}

/// `s` with `//` and `/* */` (JavaScript) or `#` (Python) comments blanked, offsets kept.
pub fn blank_comments(s: &str, python: bool) -> String {
    let mut b = s.as_bytes().to_vec();
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'"' | b'\'' | b'`' => i = skip_string(&b, i),
            b'#' if python => {
                while i < b.len() && b[i] != b'\n' {
                    b[i] = b' ';
                    i += 1;
                }
            }
            b'/' if !python && b.get(i + 1) == Some(&b'/') => {
                while i < b.len() && b[i] != b'\n' {
                    b[i] = b' ';
                    i += 1;
                }
            }
            b'/' if !python && b.get(i + 1) == Some(&b'*') => {
                let end = s[i + 2..].find("*/").map_or(b.len(), |e| i + 2 + e + 2);
                for c in &mut b[i..end] {
                    if *c != b'\n' {
                        *c = b' ';
                    }
                }
                i = end;
                continue;
            }
            _ => {}
        }
        i += 1;
    }
    String::from_utf8_lossy(&b).into_owned()
}

/// `s` with runs of whitespace collapsed to one space.
pub fn squash(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// A Python or TypeScript truth value as written.
pub fn truthy(v: Option<&str>) -> Option<bool> {
    match v?.trim() {
        "True" | "true" => Some(true),
        "False" | "false" => Some(false),
        _ => None,
    }
}
