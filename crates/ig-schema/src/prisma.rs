//! Prisma schemas: each `model` a table and each `view` a view, named by `@@map`/`@map`; a
//! relation field sets the foreign key on the scalar fields it names. Enums are left out.

use crate::Found;
use crate::text::{close, inside, split_top, unquote};
use ig_core::model::{Column, ColumnRef, Range, Table};
use regex::Regex;
use std::sync::LazyLock;

static BLOCK: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^\s*(model|view|enum|type|datasource|generator)\s+(\w+)\s*\{").unwrap()
});
static FIELD: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(\w+)\s+([\w.]+(?:\([^)]*\))?)(\[\])?(\?)?\s*(.*)$").unwrap());
static PROVIDER: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"provider\s*=\s*"([^"]+)""#).unwrap());

struct Block {
    kind: String,
    name: String,
    file: String,
    line: u32,
    end: u32,
    /// (line, text) of each body line, comments dropped.
    body: Vec<(u32, String)>,
}

struct Field<'a> {
    name: &'a str,
    ty: &'a str,
    list: bool,
    optional: bool,
    attrs: &'a str,
    line: u32,
}

fn blocks(path: &str, text: &str) -> Vec<Block> {
    let mut out: Vec<Block> = vec![];
    let mut open: Option<Block> = None;
    for (i, raw) in text.lines().enumerate() {
        let line = i as u32 + 1;
        let l = strip_comment(raw).trim();
        if let Some(b) = open.as_mut() {
            if l == "}" {
                b.end = line;
                out.extend(open.take());
            } else if !l.is_empty() {
                b.body.push((line, l.to_string()));
            }
        } else if let Some(c) = BLOCK.captures(l) {
            open = Some(Block {
                kind: c[1].into(),
                name: c[2].into(),
                file: path.into(),
                line,
                end: line,
                body: vec![],
            });
        }
    }
    out
}

fn strip_comment(l: &str) -> &str {
    let b = l.as_bytes();
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'"' => i = crate::text::skip_string(b, i),
            b'/' if b.get(i + 1) == Some(&b'/') => return &l[..i],
            _ => {}
        }
        i += 1;
    }
    l
}

fn fields(b: &Block) -> Vec<Field<'_>> {
    (b.body.iter())
        .filter(|(_, l)| !l.starts_with('@'))
        .filter_map(|(line, l)| {
            let c = FIELD.captures(l)?;
            Some(Field {
                name: c.get(1)?.as_str(),
                ty: c.get(2)?.as_str(),
                list: c.get(3).is_some(),
                optional: c.get(4).is_some(),
                attrs: c.get(5).map_or("", |m| m.as_str()),
                line: *line,
            })
        })
        .collect()
}

/// The argument text of attribute `@name(...)` in `attrs` (`Some("")` when it has none).
fn attr<'a>(attrs: &'a str, name: &str) -> Option<&'a str> {
    let re = Regex::new(&format!(r"(?:^|[^@\w])@{}\b", regex::escape(name))).ok()?;
    let m = re.find(attrs)?;
    let after = m.end();
    Some(match attrs[after..].starts_with('(') {
        true => inside(attrs, after),
        false => "",
    })
}

/// A block attribute `@@name(...)`'s argument text.
fn block_attr<'a>(b: &'a Block, name: &str) -> Option<&'a str> {
    let head = format!("@@{name}(");
    (b.body.iter())
        .find(|(_, l)| l.starts_with(&head))
        .map(|(_, l)| inside(l, head.len() - 1))
}

/// The names in a `[a, b]` list, bare or after `fields:`/`references:`.
fn list(args: &str, key: Option<&str>) -> Vec<String> {
    let at = match key {
        Some(k) => args
            .find(&format!("{k}:"))
            .and_then(|i| args[i..].find('[').map(|j| i + j)),
        None => args.find('['),
    };
    let Some(open) = at else { return vec![] };
    let end = close(args, open).unwrap_or(args.len());
    split_top(&args[open + 1..end], b',')
        .into_iter()
        .map(|(_, n)| n.split('(').next().unwrap_or(n).trim().to_string())
        .collect()
}

fn mapped(attrs: &str) -> Option<String> {
    attr(attrs, "map").and_then(|a| {
        let first = split_top(a, b',').into_iter().next()?.1;
        let v = first.strip_prefix("name:").unwrap_or(first);
        unquote(v).map(str::to_string)
    })
}

fn table_name(b: &Block) -> String {
    block_attr(b, "map")
        .and_then(|a| unquote(split_top(a, b',').first()?.1).map(str::to_string))
        .unwrap_or_else(|| b.name.clone())
}

fn column_name(models: &[Block], model: &str, field: &str) -> String {
    (models.iter().find(|m| m.name == model))
        .and_then(|m| {
            fields(m)
                .into_iter()
                .find(|f| f.name == field)
                .and_then(|f| mapped(f.attrs))
        })
        .unwrap_or_else(|| field.to_string())
}

/// The tables and views of every `.prisma` file in `files`.
pub fn read(files: &[(&str, &str)]) -> Found {
    let all: Vec<Block> = files.iter().flat_map(|(p, t)| blocks(p, t)).collect();
    let dialect = (all.iter()).find(|b| b.kind == "datasource").and_then(|b| {
        b.body
            .iter()
            .find_map(|(_, l)| PROVIDER.captures(l))
            .map(|c| c[1].to_string())
    });
    let models: Vec<Block> = all
        .into_iter()
        .filter(|b| b.kind == "model" || b.kind == "view")
        .collect();
    let tables = models.iter().map(|m| table(m, &models)).collect();
    Found {
        tables,
        dialect,
        notices: vec![],
    }
}

fn table(m: &Block, models: &[Block]) -> Table {
    let fs = fields(m);
    let is_model = |ty: &str| models.iter().any(|x| x.name == ty);
    let mut columns: Vec<Column> = (fs.iter())
        .filter(|f| !is_model(f.ty))
        .map(|f| Column {
            name: mapped(f.attrs).unwrap_or_else(|| f.name.into()),
            ty: format!("{}{}", f.ty, if f.list { "[]" } else { "" }),
            line: f.line,
            pk: attr(f.attrs, "id").is_some(),
            nullable: f.optional,
            unique: attr(f.attrs, "unique").is_some(),
            default: attr(f.attrs, "default").map(str::to_string),
            references: None,
            check: None,
        })
        .collect();
    let col = |field: &str| mapped_of(&fs, field);
    if let Some(a) = block_attr(m, "id") {
        let key = a.contains("fields:").then_some("fields");
        for f in list(a, key) {
            set(&mut columns, &col(&f), |c| c.pk = true);
        }
    }
    for (_, l) in m.body.iter().filter(|(_, l)| l.starts_with("@@unique(")) {
        let a = inside(l, "@@unique".len());
        if let [one] = list(a, a.contains("fields:").then_some("fields")).as_slice() {
            set(&mut columns, &col(one), |c| c.unique = true);
        }
    }
    for f in fs.iter().filter(|f| is_model(f.ty) && !f.list) {
        let Some(rel) = attr(f.attrs, "relation") else {
            continue;
        };
        let target = models.iter().find(|x| x.name == f.ty);
        let refs = list(rel, Some("references"));
        for (i, local) in list(rel, Some("fields")).iter().enumerate() {
            let r = ColumnRef {
                table: target.map_or_else(|| f.ty.to_string(), table_name),
                column: refs.get(i).map(|r| column_name(models, f.ty, r)),
            };
            set(&mut columns, &col(local), |c| {
                c.references = Some(r.clone())
            });
        }
    }
    Table {
        name: table_name(m),
        kind: if m.kind == "view" { "view" } else { "table" }.into(),
        source: "prisma".into(),
        file: m.file.clone(),
        range: Range {
            start_line: m.line,
            end_line: m.end,
        },
        columns,
        reads: vec![],
    }
}

fn mapped_of(fs: &[Field], field: &str) -> String {
    (fs.iter().find(|f| f.name == field))
        .and_then(|f| mapped(f.attrs))
        .unwrap_or_else(|| field.to_string())
}

fn set(columns: &mut [Column], name: &str, f: impl FnOnce(&mut Column)) {
    if let Some(c) = columns.iter_mut().find(|c| c.name == name) {
        f(c);
    }
}
