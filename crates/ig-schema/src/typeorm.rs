//! TypeORM entities: an `@Entity` class a table (named by its argument, else the class in
//! snake_case), its `@Column`-like properties the columns, a `@ManyToOne` (or a `@OneToOne`
//! with `@JoinColumn`) the foreign key column. `@ViewEntity` classes are views; columns of a
//! base class that is no entity are inherited.

use crate::Found;
use crate::text::{
    Lines, blank_comments, close, entry, inside, skip_string, snake, split_top, truthy, unquote,
};
use ig_core::model::{Column, ColumnRef, Range, Table};
use regex::Regex;
use std::sync::LazyLock;

static CLASS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\bclass\s+(\w+)(?:<[^>{]*>)?(?:\s+extends\s+([\w.]+)(?:<[^{]*>)?)?(?:\s+implements\s+[^{]+)?\s*\{").unwrap()
});
static ENTITY: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"@(Entity|ViewEntity)\s*\(").unwrap());
static PROP: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?s)^(?:(?:public|private|protected|readonly|declare|static|override|abstract)\s+)*(\w+)\s*[?!]?\s*(?::\s*([^=;]+?))?\s*(?:=.*)?;?\s*$").unwrap()
});
static TARGET: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"=>\s*(\w+)|^\s*[\x22'](\w+)[\x22']").unwrap());
static SOURCE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\.(?:from|(?:inner|left)Join(?:AndSelect)?)\(\s*(\w+)").unwrap());

const COLUMNS: &[&str] = &[
    "Column",
    "PrimaryColumn",
    "PrimaryGeneratedColumn",
    "CreateDateColumn",
    "UpdateDateColumn",
    "DeleteDateColumn",
    "VersionColumn",
    "ObjectIdColumn",
    "ViewColumn",
];

/// A class as read: its entity decorator, base, columns and relations still by class name.
struct Cls {
    name: String,
    base: Option<String>,
    entity: Option<Entity>,
    file: String,
    range: Range,
    columns: Vec<Column>,
    /// (column, target class, nullable, referenced column).
    relations: Vec<(String, String, bool, Option<String>, u32)>,
}

/// An `@Entity` or `@ViewEntity`: the table it names and a view's query.
struct Entity {
    view: bool,
    table: String,
    expression: String,
}

/// An entity decorator: where its arguments open, its name, its arguments.
type Decorated<'a> = (usize, String, Vec<(usize, &'a str)>);

struct Deco<'a> {
    name: &'a str,
    args: Vec<(usize, &'a str)>,
}

/// A class body's members between `from` and `to`: each with its decorators and offset.
fn members_of<'a>(text: &'a str, from: usize, to: usize) -> Vec<(Vec<Deco<'a>>, &'a str, usize)> {
    let b = text.as_bytes();
    let mut out = vec![];
    let mut pending = vec![];
    let mut i = from;
    while i < to {
        let c = b[i];
        if c.is_ascii_whitespace() || c == b';' || c == b',' {
            i += 1;
            continue;
        }
        if c == b'@' {
            let name_end = (i + 1..to)
                .find(|&j| !(b[j].is_ascii_alphanumeric() || b[j] == b'_'))
                .unwrap_or(to);
            let name = &text[i + 1..name_end];
            let mut j = name_end;
            while j < to && b[j] == b' ' {
                j += 1;
            }
            let args = match b.get(j) {
                Some(b'(') => {
                    let end = close(text, j).unwrap_or(to);
                    let a = split_top(&text[j + 1..end], b',');
                    j = end + 1;
                    a
                }
                _ => vec![],
            };
            pending.push(Deco { name, args });
            i = j;
            continue;
        }
        let start = i;
        let mut seen_paren = false;
        while i < to {
            match b[i] {
                b'(' => {
                    i = close(text, i).unwrap_or(to);
                    seen_paren = true;
                }
                b'{' if seen_paren => {
                    i = close(text, i).unwrap_or(to) + 1;
                    break;
                }
                b'{' | b'[' => i = close(text, i).unwrap_or(to),
                b'"' | b'\'' | b'`' => i = skip_string(b, i),
                b';' => break,
                b'\n' => {
                    let so_far = text[start..i].trim_end();
                    let next = text[i..to].trim_start();
                    let open = so_far.ends_with(['|', '&', '=', ':', ',', '(', '.', '<'])
                        || next.starts_with(['|', '&', '.', '=']);
                    if !open {
                        break;
                    }
                }
                _ => {}
            }
            i += 1;
        }
        let member = text[start..i.min(to)].trim();
        out.push((std::mem::take(&mut pending), member, start));
    }
    out
}

/// An options object's entries, from a decorator's arguments.
fn option<'a>(args: &[(usize, &'a str)], key: &str) -> Option<&'a str> {
    let obj = args.iter().find(|(_, a)| a.starts_with('{'))?.1;
    let entries = split_top(inside(obj, 0), b',');
    entries
        .iter()
        .filter_map(|(_, e)| entry(e))
        .find(|(k, _)| *k == key)
        .map(|(_, v)| v)
}

fn string_arg<'a>(args: &[(usize, &'a str)]) -> Option<&'a str> {
    args.iter().find_map(|(_, a)| unquote(a))
}

fn classes(path: &str, text: &str) -> Vec<Cls> {
    let lines = Lines::new(text);
    let entities: Vec<Decorated> = (ENTITY.captures_iter(text))
        .filter_map(|c| {
            let open = c.get(0)?.end() - 1;
            Some((open, c[1].to_string(), split_top(inside(text, open), b',')))
        })
        .collect();
    let mut prev_end = 0;
    let mut out = vec![];
    for c in CLASS.captures_iter(text) {
        let Some(m) = c.get(0) else { continue };
        let open = m.end() - 1;
        let end = close(text, open).unwrap_or(text.len());
        let deco = entities
            .iter()
            .rfind(|(at, ..)| *at >= prev_end && *at < m.start());
        let start = deco.map_or(m.start(), |d| text[..d.0].rfind('@').unwrap_or(d.0));
        prev_end = end;
        let name = c[1].to_string();
        let entity = deco.map(|(_, kind, args)| {
            let table = (string_arg(args).map(str::to_string))
                .or_else(|| option(args, "name").and_then(unquote).map(str::to_string))
                .unwrap_or_else(|| snake(&name));
            Entity {
                view: kind == "ViewEntity",
                table,
                expression: option(args, "expression").map_or(String::new(), str::to_string),
            }
        });
        let mut cls = Cls {
            name,
            base: c.get(2).map(|b| b.as_str().to_string()),
            entity,
            file: path.into(),
            range: Range {
                start_line: lines.line(start),
                end_line: lines.line(end),
            },
            columns: vec![],
            relations: vec![],
        };
        for (decos, member, at) in members_of(text, open + 1, end) {
            members(&mut cls, &decos, member, lines.line(at));
        }
        out.push(cls);
    }
    out
}

fn members(cls: &mut Cls, decos: &[Deco], member: &str, line: u32) {
    let Some(p) = PROP.captures(member) else {
        return;
    };
    let prop = &p[1];
    let ts = p
        .get(2)
        .map_or("", |t| t.as_str())
        .split('|')
        .map(str::trim)
        .find(|t| !matches!(*t, "null" | "undefined"))
        .unwrap_or("");
    let join = decos.iter().find(|d| d.name == "JoinColumn");
    for d in decos {
        if COLUMNS.contains(&d.name) {
            cls.columns.push(column(d, prop, ts, line));
        } else if d.name == "ManyToOne" || d.name == "OneToOne" && join.is_some() {
            let Some(target) = d.args.first().and_then(|(_, a)| TARGET.captures(a)) else {
                continue;
            };
            let target = target
                .get(1)
                .or(target.get(2))
                .map_or("", |m| m.as_str())
                .to_string();
            let name = join
                .and_then(|j| option(&j.args, "name"))
                .and_then(unquote)
                .map_or(format!("{prop}Id"), str::to_string);
            let referenced = join
                .and_then(|j| option(&j.args, "referencedColumnName"))
                .and_then(unquote)
                .map(str::to_string);
            let nullable = truthy(option(&d.args, "nullable")).unwrap_or(true);
            cls.relations
                .push((name, target, nullable, referenced, line));
        }
    }
}

fn column(d: &Deco, prop: &str, ts: &str, line: u32) -> Column {
    let get = |k: &str| option(&d.args, k);
    let generated = d.name == "PrimaryGeneratedColumn";
    let arg = string_arg(&d.args).filter(|a| !generated || *a == "uuid");
    let mut ty = (arg.or_else(|| get("type").and_then(unquote)))
        .map_or_else(|| ts.trim().to_string(), str::to_string);
    if let Some(len) = get("length").filter(|_| arg.is_some() || get("type").is_some()) {
        ty = format!("{ty}({})", unquote(len).unwrap_or(len));
    }
    let pk = d.name.starts_with("Primary") || truthy(get("primary")) == Some(true);
    Column {
        name: get("name").and_then(unquote).unwrap_or(prop).to_string(),
        ty,
        line,
        pk,
        nullable: !pk && (truthy(get("nullable")) == Some(true) || d.name == "DeleteDateColumn"),
        unique: truthy(get("unique")) == Some(true),
        default: get("default").map(str::to_string),
        references: None,
        check: None,
    }
}

/// The TypeORM entities in `files`.
pub fn read(files: &[(&str, &str)]) -> Found {
    let all: Vec<Cls> = (files.iter())
        .filter(|(_, t)| {
            t.contains("@Entity") || t.contains("@ViewEntity") || t.contains("typeorm")
        })
        .flat_map(|(p, t)| classes(p, &blank_comments(t, false)))
        .collect();
    let table_of = |class: &str| {
        (all.iter().find(|c| c.name == class))
            .and_then(|c| c.entity.as_ref())
            .map_or(snake(class), |e| e.table.clone())
    };
    let tables = (all.iter())
        .filter_map(|c| {
            let entity = c.entity.as_ref()?;
            let mut columns = inherited(c, &all, 0);
            for (col, target, nullable, referenced, line) in &c.relations {
                let r = ColumnRef {
                    table: table_of(target),
                    column: referenced.clone(),
                };
                match columns.iter_mut().find(|x| &x.name == col) {
                    Some(x) => x.references = Some(r),
                    None => columns.push(Column {
                        name: col.clone(),
                        ty: String::new(),
                        line: *line,
                        pk: false,
                        nullable: *nullable,
                        unique: false,
                        default: None,
                        references: Some(r),
                        check: None,
                    }),
                }
            }
            let reads = match unquote(&entity.expression) {
                Some(sql) => crate::sql::view_reads(sql),
                None => (SOURCE.captures_iter(&entity.expression))
                    .map(|m| table_of(&m[1]))
                    .collect(),
            };
            Some(Table {
                name: entity.table.clone(),
                kind: if entity.view { "view" } else { "table" }.into(),
                source: "typeorm".into(),
                file: c.file.clone(),
                range: c.range,
                columns,
                reads,
            })
        })
        .collect();
    Found {
        tables,
        dialect: None,
        notices: vec![],
    }
}

/// `c`'s columns after those of base classes that are no entity.
fn inherited(c: &Cls, all: &[Cls], depth: usize) -> Vec<Column> {
    let mut out = match (c.base.as_deref(), depth < 8) {
        (Some(b), true) => (all.iter())
            .find(|x| x.name == b && x.entity.is_none())
            .map_or(vec![], |x| inherited(x, all, depth + 1)),
        _ => vec![],
    };
    for col in &c.columns {
        out.retain(|x| x.name != col.name);
        out.push(col.clone());
    }
    out
}
