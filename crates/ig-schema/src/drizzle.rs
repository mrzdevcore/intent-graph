//! Drizzle tables: `const x = pgTable("name", { ... })` (and `mysqlTable`, `sqliteTable`, a
//! `pgTableCreator`'s function), each column's builder its type and its chain its constraints;
//! `pgView` and the like as views reading what their query's `.from`/`.join` names.

use crate::Found;
use crate::text::{Lines, blank_comments, close, entry, inside, split_top, unquote};
use ig_core::model::{Column, ColumnRef, Range, Table};
use regex::Regex;
use std::sync::LazyLock;

static DECL: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?:const|let|var)\s+(\w+)\s*(?::[^=]+)?=\s*(\w+)\s*(?:<[^>(]*>)?\(").unwrap()
});
static OBJECT: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?:const|let|var)\s+(\w+)\s*=\s*\{").unwrap());
static BUILDER: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(?:\w+\.)?(\w+)\s*(?:<[^>(]*>)?\(").unwrap());
static CHAIN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*\.\s*\$?(\w+)\s*(?:<[^>(]*>)?\(").unwrap());
static ARROW: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"=>\s*\(?\s*(\w+)\.(\w+)").unwrap());
static SOURCE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\.(?:from|innerJoin|leftJoin|rightJoin|fullJoin|crossJoin|join)\(\s*(\w+)")
        .unwrap()
});
static MEMBER: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(\w+)\.(\w+)").unwrap());
static KEYS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\b\w+\.(\w+)").unwrap());

const TABLES: &[(&str, &str)] = &[
    ("pgTable", "postgresql"),
    ("mysqlTable", "mysql"),
    ("sqliteTable", "sqlite"),
];
const VIEWS: &[(&str, &str)] = &[
    ("pgView", "postgresql"),
    ("pgMaterializedView", "postgresql"),
    ("mysqlView", "mysql"),
    ("sqliteView", "sqlite"),
];

/// A table function: its dialect, whether it makes views, and a creator's name template.
struct Maker {
    name: String,
    dialect: &'static str,
    view: bool,
    template: Option<(String, String)>,
}

/// A table as read, with its columns' JavaScript keys and pending `.references()`.
struct Decl {
    var: String,
    table: Table,
    keys: Vec<String>,
    refs: Vec<(usize, String, String)>,
    reads: Vec<String>,
}

fn makers(texts: &[String]) -> Vec<Maker> {
    let mut out: Vec<Maker> = (TABLES.iter().map(|t| (t, false)))
        .chain(VIEWS.iter().map(|v| (v, true)))
        .map(|((name, dialect), view)| Maker {
            name: name.to_string(),
            dialect,
            view,
            template: None,
        })
        .collect();
    for text in texts {
        for c in DECL.captures_iter(text) {
            let Some(dialect) = c[2].strip_suffix("TableCreator").and_then(|d| {
                TABLES
                    .iter()
                    .find(|t| t.0.strip_suffix("Table") == Some(d))
                    .map(|t| t.1)
            }) else {
                continue;
            };
            let args = inside(text, c.get(0).map_or(0, |m| m.end() - 1));
            let template = args.find('`').and_then(|i| {
                let body = &args[i + 1..];
                let body = &body[..body.find('`')?];
                let (pre, rest) = body.split_once("${")?;
                Some((pre.to_string(), rest.split_once('}')?.1.to_string()))
            });
            out.push(Maker {
                name: c[1].to_string(),
                dialect,
                view: false,
                template,
            });
        }
    }
    out
}

/// The Drizzle tables and views in `files`.
pub fn read(files: &[(&str, &str)]) -> Found {
    let marked = |t: &str| {
        ["Table(", "TableCreator(", "View("]
            .iter()
            .any(|m| t.contains(m))
            && t.contains("drizzle")
    };
    let texts: Vec<(&str, String)> = (files.iter())
        .filter(|(_, t)| marked(t))
        .map(|(p, t)| (*p, blank_comments(t, false)))
        .collect();
    let makers = makers(&texts.iter().map(|t| t.1.clone()).collect::<Vec<_>>());
    let objects: Vec<(String, String)> = (texts.iter())
        .flat_map(|(_, t)| {
            OBJECT.captures_iter(t).map(|c| {
                let open = c.get(0).map_or(0, |m| m.end() - 1);
                (c[1].to_string(), inside(t, open).to_string())
            })
        })
        .collect();
    let mut decls = vec![];
    let mut dialect = None;
    for (path, text) in &texts {
        let lines = Lines::new(text);
        for c in DECL.captures_iter(text) {
            let Some(maker) = makers.iter().find(|m| m.name == c[2]) else {
                continue;
            };
            let start = c.get(0).map_or(0, |m| m.start());
            let open = c.get(0).map_or(0, |m| m.end() - 1);
            let Some(d) = decl(&c[1], maker, text, open, &objects, path, &lines, start) else {
                continue;
            };
            dialect = dialect.or(Some(maker.dialect.to_string()));
            decls.push(d);
        }
    }
    let tables = resolve(decls);
    Found {
        tables,
        dialect,
        notices: vec![],
    }
}

#[allow(clippy::too_many_arguments)]
fn decl(
    var: &str,
    maker: &Maker,
    text: &str,
    open: usize,
    objects: &[(String, String)],
    path: &str,
    lines: &Lines,
    start: usize,
) -> Option<Decl> {
    let end = close(text, open).unwrap_or(text.len());
    let args = split_top(&text[open + 1..end], b',');
    let raw = unquote(args.first()?.1)?;
    let name = match &maker.template {
        Some((pre, post)) => format!("{pre}{raw}{post}"),
        None => raw.to_string(),
    };
    let mut d = Decl {
        var: var.into(),
        table: Table {
            name,
            kind: if maker.view { "view" } else { "table" }.into(),
            source: "drizzle".into(),
            file: path.into(),
            range: Range {
                start_line: lines.line(start),
                end_line: lines.line(end),
            },
            columns: vec![],
            reads: vec![],
        },
        keys: vec![],
        refs: vec![],
        reads: vec![],
    };
    if let Some((at, cols)) = args.get(1)
        && let Some(brace) = cols.find('{')
    {
        let base = open + 1 + at + brace;
        columns(&mut d, text, base, objects, &|at| lines.line(at), 0);
    }
    if let Some((_, extra)) = args.get(2) {
        constraints(&mut d, extra);
    }
    if maker.view {
        let rest = &text[end.min(text.len())..];
        let stop = rest
            .find("\nexport ")
            .or(rest.find("\nconst "))
            .unwrap_or(rest.len());
        let query = &rest[..stop];
        d.reads = SOURCE
            .captures_iter(query)
            .map(|c| c[1].to_string())
            .collect();
        if let Some(i) = query.find("sql`") {
            let body = &query[i + 4..];
            d.table.reads = crate::sql::view_reads(&body[..body.find('`').unwrap_or(body.len())]);
        }
        let last = rest[..stop].trim_end().len();
        d.table.range.end_line = lines.line(end + last.saturating_sub(1));
    }
    Some(d)
}

/// The columns of the object literal opening at `open` in `text`, spreads of known objects
/// included at the line of the spread.
fn columns(
    d: &mut Decl,
    text: &str,
    open: usize,
    objects: &[(String, String)],
    line: &dyn Fn(usize) -> u32,
    depth: usize,
) {
    for (at, e) in split_top(inside(text, open), b',') {
        let here = line(open + 1 + at);
        if let Some(spread) = e.strip_prefix("...") {
            if let Some((_, obj)) = objects.iter().find(|(n, _)| n == spread.trim())
                && depth < 4
            {
                columns(d, &format!("{{{obj}}}"), 0, objects, &|_| here, depth + 1);
            }
        } else if let Some((key, expr)) = entry(e) {
            column(d, key, expr, here);
        }
    }
}

fn column(d: &mut Decl, key: &str, expr: &str, line: u32) {
    let Some(b) = BUILDER.captures(expr) else {
        return;
    };
    let open = b.get(0).map_or(0, |m| m.end() - 1);
    let end = close(expr, open).unwrap_or(expr.len());
    let args = split_top(&expr[open + 1..end], b',');
    let name = args.first().and_then(|(_, a)| unquote(a)).unwrap_or(key);
    let mut c = Column {
        name: name.into(),
        ty: b[1].to_string(),
        line,
        pk: false,
        nullable: true,
        unique: false,
        default: None,
        references: None,
        check: None,
    };
    let mut rest = &expr[(end + 1).min(expr.len())..];
    while let Some(m) = CHAIN.captures(rest) {
        let open = m.get(0).map_or(0, |m| m.end() - 1);
        let close = close(rest, open).unwrap_or(rest.len());
        let inner = rest.get(open + 1..close).unwrap_or("");
        match &m[1] {
            "notNull" => c.nullable = false,
            "primaryKey" => (c.pk, c.nullable) = (true, false),
            "unique" => c.unique = true,
            "default" => c.default = Some(inner.trim().to_string()),
            "defaultNow" => c.default = Some("now()".into()),
            "defaultRandom" => c.default = Some("gen_random_uuid()".into()),
            "array" => c.ty.push_str("[]"),
            "references" => {
                if let Some(r) = ARROW.captures(inner) {
                    d.refs
                        .push((d.table.columns.len(), r[1].to_string(), r[2].to_string()));
                }
            }
            _ => {}
        }
        rest = rest.get(close + 1..).unwrap_or("");
    }
    d.keys.push(key.into());
    d.table.columns.push(c);
}

/// A table's third argument: `primaryKey(...)`, `unique().on(t.x)`, `foreignKey(...)`.
fn constraints(d: &mut Decl, extra: &str) {
    let col = |d: &Decl, key: &str| d.keys.iter().position(|k| k == key);
    let mut body = extra.split_once("=>").map_or(extra, |b| b.1).trim();
    while let Some(inner) = body
        .strip_prefix(['(', '[', '{'])
        .map(|_| inside(body, 0).trim())
    {
        body = inner;
    }
    for (_, part) in split_top(body, b',') {
        let part = part
            .split_once(':')
            .filter(|(k, _)| !k.contains('('))
            .map_or(part, |p| p.1)
            .trim();
        if part.starts_with("primaryKey(") {
            for k in KEYS.captures_iter(part) {
                if let Some(i) = col(d, &k[1]) {
                    (d.table.columns[i].pk, d.table.columns[i].nullable) = (true, false);
                }
            }
        } else if part.starts_with("unique(") || part.starts_with("uniqueIndex(") {
            let on = part.find(".on(").map(|i| inside(part, i + 3)).unwrap_or("");
            let keys: Vec<_> = KEYS
                .captures_iter(on)
                .filter_map(|k| col(d, &k[1]))
                .collect();
            if let [one] = keys.as_slice() {
                d.table.columns[*one].unique = true;
            }
        } else if part.starts_with("foreignKey(") {
            let list = |name: &str| -> Vec<(String, String)> {
                let at = part
                    .find(&format!("{name}:"))
                    .and_then(|i| part[i..].find('[').map(|j| i + j));
                at.map_or(vec![], |o| {
                    let inner = inside(part, o);
                    (MEMBER.captures_iter(inner))
                        .map(|c| (c[1].to_string(), c[2].to_string()))
                        .collect()
                })
            };
            for ((_, key), (var, target)) in list("columns").into_iter().zip(list("foreignColumns"))
            {
                if let Some(i) = col(d, &key) {
                    d.refs.push((i, var, target));
                }
            }
        }
    }
}

/// Every table with `.references(() => x.y)` resolved through the `const` declaring `x`.
fn resolve(decls: Vec<Decl>) -> Vec<Table> {
    let lookup = |var: &str, key: &str| -> ColumnRef {
        match decls.iter().find(|d| d.var == var) {
            Some(d) => ColumnRef {
                table: d.table.name.clone(),
                column: Some(
                    (d.keys.iter().position(|k| k == key))
                        .map_or(key.to_string(), |i| d.table.columns[i].name.clone()),
                ),
            },
            None => ColumnRef {
                table: var.into(),
                column: Some(key.into()),
            },
        }
    };
    let resolved: Vec<Table> = (decls.iter())
        .map(|d| {
            let mut t = d.table.clone();
            for (i, var, key) in &d.refs {
                if let Some(c) = t.columns.get_mut(*i) {
                    c.references = Some(lookup(var, key));
                }
            }
            for var in &d.reads {
                let name = decls
                    .iter()
                    .find(|x| &x.var == var)
                    .map_or(var.clone(), |x| x.table.name.clone());
                if !t.reads.contains(&name) {
                    t.reads.push(name);
                }
            }
            t
        })
        .collect();
    resolved
}
