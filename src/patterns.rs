//! Pattern nodes from `ig.yaml` `patterns` (ast-grep).

use crate::config;
use crate::extract::{FileFacts, intents_in, normalized_hash};
use crate::model::{Metrics, Node, Range};
use ast_grep_core::tree_sitter::LanguageExt;
use ast_grep_core::{Language, Pattern};
use ast_grep_language::{JavaScript, Tsx, TypeScript};
use std::collections::HashMap;

pub struct Compiled {
    /// (config, [TypeScript, Tsx, JavaScript])
    pats: Vec<(config::Pattern, [Pattern; 3])>,
    pub errors: Vec<(usize, String)>,
}

pub fn compile(patterns: &[config::Pattern]) -> Compiled {
    let mut c = Compiled { pats: vec![], errors: vec![] };
    for (i, p) in patterns.iter().enumerate() {
        let r = (|| -> Result<[Pattern; 3], String> {
            let e = |e: ast_grep_core::PatternError| e.to_string();
            let ts = Pattern::try_new(&p.matcher, TypeScript).map_err(e)?;
            if ts.has_error() {
                return Err(format!("pattern does not parse: `{}`", p.matcher));
            }
            Ok([
                ts,
                Pattern::try_new(&p.matcher, Tsx).map_err(e)?,
                Pattern::try_new(&p.matcher, JavaScript).map_err(e)?,
            ])
        })();
        match r {
            Ok(ps) => c.pats.push((p.clone(), ps)),
            Err(m) => c.errors.push((i, m)),
        }
    }
    c
}

fn strip_quotes(s: &str) -> &str {
    let t = s.trim();
    for q in ['"', '\'', '`'] {
        if t.len() >= 2 && t.starts_with(q) && t.ends_with(q) {
            return &t[1..t.len() - 1];
        }
    }
    t
}

struct Hit {
    start_line: u32,
    end_line: u32,
    id: String,
    kind: String,
    text: String,
}

fn find<L: Language + LanguageExt>(lang: L, code: &str, cfg: &config::Pattern, pat: &Pattern, out: &mut Vec<Hit>) {
    let grep = lang.ast_grep(code);
    for m in grep.root().find_all(pat) {
        let env = m.get_env();
        let text_of = |var: &str| env.get_match(var).map(|n| n.text().to_string());
        let ok = cfg.where_.iter().all(|(var, allowed)| {
            text_of(var).is_some_and(|t| allowed.iter().any(|a| a == &t || a == strip_quotes(&t)))
        });
        if !ok {
            continue;
        }
        // Interpolate `{VAR}`; a missing variable skips the match.
        let mut id = String::new();
        let mut rest = cfg.id.as_str();
        let mut missing = false;
        while let Some(open) = rest.find('{') {
            id.push_str(&rest[..open]);
            let Some(close) = rest[open..].find('}') else { break };
            let var = &rest[open + 1..open + close];
            match text_of(var) {
                Some(t) if cfg.where_.iter().any(|(v, _)| v == var) => id.push_str(&strip_quotes(&t).to_uppercase()),
                Some(t) => id.push_str(strip_quotes(&t)),
                None => missing = true,
            }
            rest = &rest[open + close + 1..];
        }
        id.push_str(rest);
        if missing {
            continue;
        }
        out.push(Hit {
            start_line: m.start_pos().line() as u32 + 1,
            end_line: m.end_pos().line() as u32 + 1,
            id,
            kind: cfg.kind.clone(),
            text: m.text().to_string(),
        });
    }
}

fn comment_lines_above(code: &str, line: u32) -> String {
    let lines: Vec<&str> = code.lines().collect();
    let mut out = vec![];
    let mut i = line as usize - 1;
    while i > 0 {
        let t = lines.get(i - 1).map(|l| l.trim()).unwrap_or("");
        if t.starts_with("//") || t.starts_with("/*") || t.starts_with('*') || t.ends_with("*/") {
            out.push(t);
            i -= 1;
        } else {
            break;
        }
    }
    out.reverse();
    out.join("\n")
}

fn contains(outer: &Range, inner: &Range) -> bool {
    outer.start_line <= inner.start_line && inner.end_line <= outer.end_line
}

fn span(r: &Range) -> u32 {
    r.end_line - r.start_line
}

/// Adds pattern nodes to `facts`, reparenting nodes and re-attributing references inside them.
pub fn apply(c: &Compiled, file: &str, code: &str, facts: &mut FileFacts) {
    if c.pats.is_empty() {
        return;
    }
    let lower = file.to_ascii_lowercase();
    let lang = if lower.ends_with(".tsx") || lower.ends_with(".jsx") {
        1
    } else if lower.ends_with(".js") || lower.ends_with(".mjs") || lower.ends_with(".cjs") {
        2
    } else {
        0
    };
    let mut hits = vec![];
    for (cfg, ps) in &c.pats {
        match lang {
            1 => find(Tsx, code, cfg, &ps[1], &mut hits),
            2 => find(JavaScript, code, cfg, &ps[2], &mut hits),
            _ => find(TypeScript, code, cfg, &ps[0], &mut hits),
        }
    }
    hits.sort_by_key(|h| h.start_line);
    let mut counts: HashMap<String, u32> = HashMap::new();
    for h in hits {
        let base = format!("{file}#{}", h.id);
        let n = counts.entry(base.clone()).or_insert(0);
        *n += 1;
        let id = if *n == 1 { base } else { format!("{base}~{n}") };
        if facts.nodes.iter().any(|x| x.id == id) {
            continue;
        }
        let range = Range { start_line: h.start_line, end_line: h.end_line };
        let parent = facts
            .nodes
            .iter()
            .filter(|n| contains(&n.range, &range))
            .min_by_key(|n| span(&n.range))
            .map(|n| n.id.clone())
            .unwrap_or_else(|| file.to_string());
        for n in facts.nodes.iter_mut() {
            if n.parent.as_deref() == Some(&parent) && contains(&range, &n.range) {
                n.parent = Some(id.clone());
            }
        }
        for r in facts.refs.iter_mut() {
            if r.from == parent && range.start_line <= r.line && r.line <= range.end_line {
                r.from = id.clone();
            }
        }
        let mut intents = vec![];
        intents_in(&comment_lines_above(code, h.start_line), &mut intents);
        let package = facts.nodes[0].package.clone();
        facts.nodes.push(Node {
            id,
            kind: h.kind,
            name: h.id,
            file: file.into(),
            range,
            parent: Some(parent),
            package,
            exported: false,
            hash: normalized_hash(&h.text),
            metrics: Metrics { loc: h.end_line - h.start_line + 1, complexity: 1 },
            intents,
        });
    }
}
