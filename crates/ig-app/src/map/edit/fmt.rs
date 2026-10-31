//! `ig fmt`: the spec files with every block indented four spaces a level, at most one blank
//! line between nodes and none at the ends or after the header, and the old names of kinds
//! and arrows written as the new ones. Only those names, the whitespace before a line and blank
//! lines change: comments, order and the rest of every line stay.

use super::super::spec::{self, DIR, HEADER, OLD_HEADER, read::new_name};
use ig_core::ports::Files;
use kdl::KdlDocument;

/// Formats every `ig/*.kdl` (the team kinds in `ig/kinds/` are read, not formatted) and writes the ones that change (`write` false: writes nothing);
/// returns their paths. A file that does not read, or a format that would change the map or a
/// comment, is an error and nothing is written.
pub fn fmt(files: &dyn Files, write: bool) -> Result<Vec<String>, Vec<String>> {
    let (kinds, old) = spec::read::texts(files)?;
    let before = spec::parse([old.clone(), kinds.clone()].concat())?;
    let mut new = vec![];
    for (p, t) in &old {
        let t = t.as_deref().unwrap_or_default();
        let text = format(&renamed(t));
        if let Some(line) = changed_comment(t, &text) {
            return Err(vec![format!(
                "fmt would change a comment at {p}:{line}; nothing written"
            )]);
        }
        new.push((p.clone(), Some(text)));
    }
    if spec::parse([new.clone(), kinds].concat())? != before {
        return Err(vec![format!(
            "{DIR}: formatting would change the spec; nothing written"
        )]);
    }
    let mut changed: Vec<&(String, Option<String>)> = new
        .iter()
        .zip(&old)
        .filter(|(n, o)| n.1 != o.1)
        .map(|(n, _)| n)
        .collect();
    // system.kdl last, as every write to ig/
    changed.sort_by_key(|(p, _)| p.ends_with("/system.kdl"));
    if write {
        for (p, t) in &changed {
            let t = t.as_deref().unwrap_or_default();
            files
                .write_atomic(p, t.as_bytes())
                .map_err(|e| vec![format!("{p}: {e}")])?;
        }
    }
    Ok(changed.into_iter().map(|(p, _)| p.clone()).collect())
}

/// `t` with every old kind and arrow name written as the new one; as it is when it does not read.
fn renamed(t: &str) -> String {
    fn walk(d: &mut KdlDocument) -> bool {
        let mut changed = false;
        for n in d.nodes_mut() {
            if let Some(new) = new_name(n.name().value()) {
                n.set_name(new);
                changed = true;
            }
            if let Some(c) = n.children_mut() {
                changed |= walk(c);
            }
        }
        changed
    }
    match t.parse::<KdlDocument>() {
        Ok(mut d) => match walk(&mut d) {
            true => d.to_string(),
            false => t.into(),
        },
        Err(_) => t.into(),
    }
}

/// `t` read for its layout: for each line, whether it starts in code (not inside a string or a
/// comment) and the block depth there; and every comment with the line it starts on.
struct Lexed {
    lines: Vec<(bool, usize)>,
    comments: Vec<(usize, String)>,
}

enum At {
    Code,
    /// A quoted string; `true` for a `"""` one.
    Str(bool),
    /// A raw string closed by `"` and this many `#`.
    Raw(usize),
    /// A `/* */` comment: its nesting, start offset and line.
    Block(usize, usize, usize),
    /// A `//` comment: its start offset and line.
    Line(usize, usize),
}

fn lex(t: &str) -> Lexed {
    let b = t.as_bytes();
    let (mut i, mut depth, mut line) = (0, 0usize, 1);
    let mut lines = vec![(true, 0)];
    let mut comments = vec![];
    let mut s = At::Code;
    while i < b.len() {
        let c = b[i];
        let at = |p: &str| t.as_bytes()[i..].starts_with(p.as_bytes());
        match s {
            At::Code => match c {
                b'{' => depth += 1,
                b'}' => depth = depth.saturating_sub(1),
                b'"' if at("\"\"\"") => {
                    s = At::Str(true);
                    i += 2;
                }
                b'"' => s = At::Str(false),
                b'#' => {
                    let n = b[i..].iter().take_while(|&&x| x == b'#').count();
                    if b.get(i + n) == Some(&b'"') {
                        s = At::Raw(n);
                        i += n;
                    }
                }
                b'/' if at("//") => {
                    s = At::Line(i, line);
                    i += 1;
                }
                b'/' if at("/*") => {
                    s = At::Block(1, i, line);
                    i += 1;
                }
                _ => {}
            },
            At::Str(multi) => match c {
                b'\\' if b.get(i + 1) != Some(&b'\n') => i += 1,
                b'"' if multi && at("\"\"\"") => {
                    s = At::Code;
                    i += 2;
                }
                b'"' if !multi => s = At::Code,
                _ => {}
            },
            At::Raw(n) => {
                if c == b'"' && b[i + 1..].iter().take(n).filter(|&&x| x == b'#').count() == n {
                    s = At::Code;
                    i += n;
                }
            }
            At::Block(nest, start, l) => {
                if at("/*") {
                    s = At::Block(nest + 1, start, l);
                    i += 1;
                } else if at("*/") {
                    i += 1;
                    s = match nest {
                        1 => {
                            comments.push((l, t[start..=i].to_string()));
                            At::Code
                        }
                        _ => At::Block(nest - 1, start, l),
                    };
                }
            }
            At::Line(start, l) if c == b'\n' => {
                comments.push((l, t[start..i].to_string()));
                s = At::Code;
            }
            At::Line(..) => {}
        }
        if c == b'\n' {
            line += 1;
            lines.push((matches!(s, At::Code), depth));
        }
        i += 1;
    }
    if let At::Line(start, l) = s {
        comments.push((l, t[start..].to_string()));
    }
    Lexed { lines, comments }
}

/// `t` formatted: a line that starts in code gets four spaces a level of the block it is in
/// (a `}` line its block's own), and runs of blank lines become one.
fn format(t: &str) -> String {
    let lx = lex(t);
    let mut out = String::new();
    let (mut gap, mut opens) = (false, false);
    for (k, l) in t.split_inclusive('\n').enumerate() {
        let (code, depth) = lx.lines[k];
        if !code {
            out.push_str(l);
            continue;
        }
        let body = l.trim_start_matches([' ', '\t']);
        if body.trim().is_empty() {
            gap = true;
            continue;
        }
        let closes = body.starts_with('}');
        if gap && !opens && !closes && !out.is_empty() && out != HEADER && out != OLD_HEADER {
            out.push('\n');
        }
        gap = false;
        let d = if closes {
            depth.saturating_sub(1)
        } else {
            depth
        };
        out.push_str(&"    ".repeat(d));
        out.push_str(body);
        opens = body.trim_end().ends_with('{');
    }
    if !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }
    out
}

/// The line in `old` of a comment that `new` does not keep as written, or of one it adds.
pub(super) fn changed_comment(old: &str, new: &str) -> Option<usize> {
    let (a, b) = (lex(old).comments, lex(new).comments);
    let mut left: Vec<&str> = b.iter().map(|(_, c)| c.as_str()).collect();
    for (l, c) in &a {
        match left.iter().position(|x| x == c) {
            Some(i) => {
                left.swap_remove(i);
            }
            None => return Some(*l),
        }
    }
    let extra = left.first()?;
    b.iter().find(|(_, c)| c == extra).map(|(l, _)| *l)
}
