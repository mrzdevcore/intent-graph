//! SQLAlchemy: declarative classes with `__tablename__` (`Column(...)` and 2.0's
//! `Mapped[T] = mapped_column(...)`, mixin columns included) and `Table("name", metadata, ...)`.

use crate::Found;
use crate::py::{self, Class, Line, assign, call};
use crate::text::{arg, kwarg, split_top, truthy, unquote};
use ig_core::model::{Column, ColumnRef, Range, Table};
use regex::Regex;
use std::sync::LazyLock;

static TABLE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?:^|[^\w.])(?:\w+\.)?Table\s*\(").unwrap());
static MODULE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^(?:[a-z_]\w*\.)+").unwrap());

/// A class with its columns as read, before mixins and `Model.attr` keys resolve.
struct Cls {
    name: String,
    bases: Vec<String>,
    table: Option<String>,
    file: String,
    line: u32,
    end: u32,
    columns: Vec<Column>,
}

/// A foreign key written `ForeignKey(Model.attr)`, kept as `@Model` until classes are known.
const CLASS_REF: char = '@';

fn classes(path: &str, lines: &[Line], all: &[Class]) -> Vec<Cls> {
    (all.iter())
        .map(|c| {
            let mut table = None;
            let mut columns = vec![];
            for &i in &c.body {
                let (target, value) = assign(&lines[i].text);
                let (name, ann) = match target.split_once(':') {
                    Some((n, a)) => (n.trim(), Some(a.trim())),
                    None => (target, None),
                };
                if name == "__tablename__" {
                    table = value.and_then(unquote).map(str::to_string);
                } else if name.chars().all(|ch| ch.is_alphanumeric() || ch == '_') {
                    columns.extend(member(name, ann, value, lines[i].line));
                }
            }
            Cls {
                name: c.name.clone(),
                bases: c
                    .bases
                    .iter()
                    .map(|b| b.rsplit('.').next().unwrap_or(b).to_string())
                    .collect(),
                table,
                file: path.into(),
                line: c.line,
                end: c.end,
                columns,
            }
        })
        .collect()
}

/// `Mapped[T]`'s type and whether it is optional.
fn mapped(ann: &str) -> Option<(String, bool)> {
    let open = ann.find("Mapped[")? + "Mapped".len();
    let inner = crate::text::inside(ann, open).trim();
    let inner = unquote(inner).unwrap_or(inner);
    let parts: Vec<&str> = inner.split('|').map(str::trim).collect();
    if parts.len() > 1 && parts.contains(&"None") {
        let t = parts.iter().find(|p| **p != "None").unwrap_or(&"");
        return Some((t.to_string(), true));
    }
    for wrap in ["Optional[", "Union["] {
        if let Some(rest) = inner.strip_prefix(wrap) {
            let rest = rest.strip_suffix(']').unwrap_or(rest);
            let t = split_top(rest, b',')
                .into_iter()
                .map(|(_, t)| t)
                .find(|t| *t != "None");
            return Some((
                unquote(t.unwrap_or(""))
                    .unwrap_or(t.unwrap_or(""))
                    .to_string(),
                true,
            ));
        }
    }
    Some((inner.to_string(), false))
}

fn member(name: &str, ann: Option<&str>, value: Option<&str>, line: u32) -> Option<Column> {
    let typed = ann.and_then(mapped);
    let called = value.and_then(call);
    match called {
        Some((f, args)) if f == "Column" || f == "mapped_column" => {
            column(Some(name), args, typed, line)
        }
        None if typed.is_some() && value.is_none() => column(Some(name), "", typed, line),
        _ => None,
    }
}

/// A `Column(...)`/`mapped_column(...)` call's column.
fn column(
    attr: Option<&str>,
    args: &str,
    typed: Option<(String, bool)>,
    line: u32,
) -> Option<Column> {
    let args = split_top(args, b',');
    let get = |k: &str| arg(&args, k, kwarg);
    let positional: Vec<&str> = args
        .iter()
        .map(|(_, a)| *a)
        .filter(|a| kwarg(a).is_none())
        .collect();
    let mut name = get("name").and_then(unquote).map(str::to_string);
    let (mut ty, mut references) = (None, None);
    for p in &positional {
        if let Some(s) = unquote(p) {
            name = name.or(Some(s.to_string()));
        } else if let Some(("ForeignKey", fk)) = call(p) {
            references = foreign_key(fk);
        } else if !matches!(
            call(p).map(|c| c.0),
            Some("Sequence" | "Identity" | "Computed" | "CheckConstraint")
        ) {
            ty = ty.or(Some(MODULE.replace(p, "").into_owned()));
        }
    }
    let ty = get("type_")
        .map(|t| MODULE.replace(t, "").into_owned())
        .or(ty);
    let pk = truthy(get("primary_key")) == Some(true);
    let nullable = truthy(get("nullable")).unwrap_or(match &typed {
        Some((_, optional)) => *optional,
        None => !pk,
    });
    Some(Column {
        name: name.or(attr.map(str::to_string))?,
        ty: ty.or(typed.map(|t| t.0)).unwrap_or_default(),
        line,
        pk,
        nullable: nullable && !pk,
        unique: truthy(get("unique")) == Some(true),
        default: get("default").or(get("server_default")).map(str::to_string),
        references,
        check: None,
    })
}

/// `ForeignKey("table.col")` or `ForeignKey(Model.attr)`.
fn foreign_key(args: &str) -> Option<ColumnRef> {
    let first = split_top(args, b',').into_iter().next()?.1;
    let (target, quoted) = match unquote(first) {
        Some(s) => (s, true),
        None => (first, false),
    };
    let (table, col) = target.rsplit_once('.')?;
    let table = table.strip_prefix("public.").unwrap_or(table);
    Some(ColumnRef {
        table: if quoted {
            table.into()
        } else {
            format!("{CLASS_REF}{table}")
        },
        column: Some(col.into()),
    })
}

/// `Table("name", metadata, Column(...), ...)` declarations in `lines`.
fn tables(path: &str, lines: &[Line]) -> Vec<Table> {
    let mut out = vec![];
    for l in lines {
        let Some(m) = TABLE.find(&l.text) else {
            continue;
        };
        let args = split_top(crate::text::inside(&l.text, m.end() - 1), b',');
        let Some(name) = args.first().and_then(|(_, a)| unquote(a)) else {
            continue;
        };
        let columns = (args.iter())
            .filter_map(|(_, a)| match call(a) {
                Some(("Column", c)) => column(None, c, None, l.line),
                _ => None,
            })
            .collect();
        out.push(table(name, path, l.line, l.end, columns));
    }
    out
}

fn table(name: &str, file: &str, line: u32, end: u32, columns: Vec<Column>) -> Table {
    Table {
        name: name.into(),
        kind: "table".into(),
        source: "sqlalchemy".into(),
        file: file.into(),
        range: Range {
            start_line: line,
            end_line: end,
        },
        columns,
        reads: vec![],
    }
}

/// Columns of `c`'s unmapped bases (mixins), then its own.
fn with_mixins(c: &Cls, all: &[Cls], depth: usize) -> Vec<Column> {
    let mut out: Vec<Column> = vec![];
    for b in &c.bases {
        if let Some(base) = all.iter().find(|x| &x.name == b && x.table.is_none())
            && depth < 8
        {
            out.extend(with_mixins(base, all, depth + 1));
        }
    }
    for col in &c.columns {
        out.retain(|x| x.name != col.name);
        out.push(col.clone());
    }
    out
}

/// The SQLAlchemy tables in `files`.
pub fn read(files: &[(&str, &str)]) -> Found {
    let mut out = vec![];
    let mut all: Vec<Cls> = vec![];
    for (path, text) in files {
        if !text.contains("__tablename__") && !text.contains("Table(") {
            continue;
        }
        let lines = py::lines(text);
        all.extend(classes(path, &lines, &py::classes(&lines)));
        out.extend(tables(path, &lines));
    }
    for c in all.iter().filter(|c| c.table.is_some()) {
        let name = c.table.as_deref().unwrap_or_default();
        out.push(table(name, &c.file, c.line, c.end, with_mixins(c, &all, 0)));
    }
    for r in out
        .iter_mut()
        .flat_map(|t| t.columns.iter_mut())
        .filter_map(|c| c.references.as_mut())
    {
        if let Some(class) = r.table.strip_prefix(CLASS_REF) {
            let hit = all.iter().find(|c| c.name == class);
            r.table = hit
                .and_then(|c| c.table.clone())
                .unwrap_or_else(|| class.to_string());
            let col = hit.and_then(|c| {
                c.columns
                    .iter()
                    .find(|x| r.column.as_deref() == Some(&x.name))
            });
            r.column = col.map(|x| x.name.clone()).or(r.column.take());
        }
    }
    Found {
        tables: out,
        dialect: None,
        notices: vec![],
    }
}
