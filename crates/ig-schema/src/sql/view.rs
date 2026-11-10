//! A query's reads (`FROM`, `JOIN`) and the columns its select list names plainly.

use super::lex::{Cur, K, Tok, commas, statements, type_text};
use super::{Hints, col_at, find, same};
use ig_core::model::{Column, Table};

/// Words that end a `FROM` item, so are no alias.
pub(super) const CLAUSE: &[&str] = &[
    "WHERE",
    "JOIN",
    "INNER",
    "LEFT",
    "RIGHT",
    "FULL",
    "CROSS",
    "OUTER",
    "NATURAL",
    "ON",
    "USING",
    "GROUP",
    "ORDER",
    "HAVING",
    "LIMIT",
    "OFFSET",
    "UNION",
    "EXCEPT",
    "INTERSECT",
    "WINDOW",
    "FETCH",
    "FOR",
    "LATERAL",
    "AS",
    "SELECT",
    "FROM",
    "WITH",
    "RETURNING",
    "QUALIFY",
    "END",
    "NULL",
    "TRUE",
    "FALSE",
    "AND",
    "OR",
    "NOT",
    "IS",
    "IN",
    "THEN",
    "ELSE",
    "WHEN",
    "DESC",
    "ASC",
];

pub(super) fn clause(t: &Tok) -> bool {
    CLAUSE.iter().any(|k| t.is(k))
}

/// The tables a query reads (its `FROM` and `JOIN` items, CTE names left out) and the alias of
/// each, as (alias, table).
pub(super) fn reads(toks: &[Tok]) -> (Vec<String>, Vec<(String, String)>) {
    let mut ctes = vec![];
    for w in toks.windows(3) {
        if w[0].ident() && w[1].is("AS") && w[2].p("(") {
            ctes.push(w[0].text.to_ascii_lowercase());
        }
    }
    let (mut out, mut aliases): (Vec<String>, Vec<(String, String)>) = (vec![], vec![]);
    let mut selects = vec![false];
    let mut c = Cur::new(toks);
    while let Some(t) = c.peek() {
        c.i += 1;
        if t.p("(") {
            selects.push(false);
        } else if t.p(")") {
            selects.pop();
            if selects.is_empty() {
                selects.push(false);
            }
        } else if t.is("SELECT") {
            if let Some(s) = selects.last_mut() {
                *s = true;
            }
        } else if t.is("JOIN") || t.is("FROM") && selects.last() == Some(&true) {
            loop {
                c.eat("ONLY");
                c.eat("LATERAL");
                let Some(name) = c.name() else { break };
                if c.at_p("(") {
                    break;
                }
                let alias = (c.eat("AS") || c.peek().is_some_and(|t| t.ident() && !clause(t)))
                    .then(|| c.ident())
                    .flatten();
                let cte = ctes.contains(&name.to_ascii_lowercase());
                if !cte && !out.iter().any(|o| same(o, &name)) {
                    out.push(name.clone());
                }
                aliases.push((alias.unwrap_or_else(|| name.clone()), name));
                if !(t.is("FROM") && c.eat_p(",")) {
                    break;
                }
            }
        }
    }
    (out, aliases)
}

/// The tables a query in SQL text reads, for a view an ORM declares with SQL.
pub fn view_reads(sql: &str) -> Vec<String> {
    reads(&statements(sql, &mut Hints::default()).concat()).0
}

/// A query's columns where the select list says plainly (`t.col`, `x AS alias`, `t.*`), typed
/// from `tables` when they name a known column, and the tables it reads.
pub(super) fn select_columns(body: &[Tok], tables: &[Table]) -> (Vec<Column>, Vec<String>) {
    let (read, aliases) = reads(body);
    let mut depth = 0;
    let mut start = None;
    let mut end = body.len();
    for (i, t) in body.iter().enumerate() {
        if t.p("(") {
            depth += 1;
        } else if t.p(")") {
            depth -= 1;
        } else if depth == 0 && t.is("SELECT") && start.is_none() {
            start = Some(i + 1);
        } else if depth == 0
            && start.is_some()
            && (t.is("FROM") || t.is("UNION") || t.is("EXCEPT") || t.is("INTERSECT"))
        {
            end = i;
            break;
        }
    }
    let Some(start) = start else {
        return (vec![], read);
    };
    let mut c = Cur::new(&body[start..end]);
    let _ = c.eat("DISTINCT") && c.eat("ON") && c.group().is_some();
    c.eat("ALL");
    let source_of = |q: &str| {
        let name = (aliases.iter())
            .find(|(a, _)| same(a, q))
            .map_or(q, |(_, t)| t.as_str());
        find(tables, name).map(|i| &tables[i])
    };
    let from: Vec<&Table> = aliases
        .iter()
        .filter_map(|(_, t)| find(tables, t).map(|i| &tables[i]))
        .collect();
    let mut out: Vec<Column> = vec![];
    for item in commas(c.rest()) {
        let line = item[0].line;
        let n = item.len();
        if item[n - 1].p("*") {
            let star = match item {
                [q, dot, _] if dot.p(".") => source_of(&q.text).into_iter().collect(),
                [_] => from.clone(),
                _ => vec![],
            };
            for t in star {
                out.extend(t.columns.iter().map(|c| plain(&c.name, &c.ty, line)));
            }
            continue;
        }
        let last = &item[n - 1];
        let (name, expr) = if n >= 2 && item[n - 2].is("AS") && last.ident() {
            (last.text.clone(), &item[..n - 2])
        } else if last.ident() && (n == 1 || n == 3 && item[1].p(".")) {
            (last.text.clone(), item)
        } else if n >= 2
            && last.ident()
            && !clause(last)
            && !item[n - 2].p(".")
            && (item[n - 2].p(")")
                || matches!(item[n - 2].k, K::Word | K::Quoted | K::Str | K::Num))
        {
            (last.text.clone(), &item[..n - 1])
        } else {
            continue;
        };
        let ty = match expr {
            [col] if col.ident() => from
                .iter()
                .find_map(|t| col_at(t, &col.text).map(|i| t.columns[i].ty.clone())),
            [q, dot, col] if dot.p(".") && col.ident() => source_of(&q.text)
                .and_then(|t| col_at(t, &col.text).map(|i| t.columns[i].ty.clone())),
            _ => expr
                .iter()
                .rposition(|t| t.p("::"))
                .map(|i| type_text(&expr[i + 1..])),
        };
        out.push(plain(&name, &ty.unwrap_or_default(), line));
    }
    (out, read)
}

pub(super) fn plain(name: &str, ty: &str, line: u32) -> Column {
    Column {
        name: name.into(),
        ty: ty.into(),
        line,
        pk: false,
        nullable: true,
        unique: false,
        default: None,
        references: None,
        check: None,
    }
}
