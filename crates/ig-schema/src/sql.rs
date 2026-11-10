//! SQL DDL and migrations: `CREATE`/`ALTER`/`DROP` of tables and views, applied file by file in
//! natural path order so the last migration's state wins. Postgres first; MySQL and SQLite
//! syntax tolerated. Every other statement is skipped.

mod alter;
mod column;
mod lex;
mod order;
mod view;

pub use order::{is_down, natural};
pub use view::view_reads;

use crate::Found;
use column::{Con, Named, and, column, constraint, names};
use ig_core::model::{Column, ColumnRef, Range, Table};
use lex::{Cur, Tok, commas, statements};
use regex::Regex;
use std::sync::LazyLock;
use view::{plain, select_columns};

#[derive(Default)]
struct Hints {
    pg: u32,
    mysql: u32,
    sqlite: u32,
}

impl Hints {
    fn dialect(&self) -> Option<String> {
        let best = self.pg.max(self.mysql).max(self.sqlite);
        let name = if best == 0 {
            return None;
        } else if self.pg == best {
            "postgresql"
        } else if self.mysql == best {
            "mysql"
        } else {
            "sqlite"
        };
        Some(name.into())
    }

    fn of_type(&mut self, ty: &str) {
        let pg = [
            "uuid",
            "timestamptz",
            "jsonb",
            "serial",
            "bigserial",
            "smallserial",
            "bytea",
        ];
        if ty.ends_with("[]") || pg.contains(&ty) {
            self.pg += 1;
        }
    }
}

/// The tables, the constraints by name, dialect votes and notices as statements apply.
struct Db<'a> {
    tables: Vec<Table>,
    named: Vec<Named>,
    hints: Hints,
    notices: Vec<String>,
    path: &'a str,
    src: &'a str,
}

/// Index of the table `name`, case-insensitive; a bare name also finds a qualified one.
fn find(tables: &[Table], name: &str) -> Option<usize> {
    let exact = tables
        .iter()
        .position(|t| t.name.eq_ignore_ascii_case(name));
    exact.or_else(|| {
        let tail = |n: &str| n.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
        let hits: Vec<usize> = (0..tables.len())
            .filter(|&i| tail(&tables[i].name) == tail(name))
            .collect();
        (hits.len() == 1).then(|| hits[0])
    })
}

fn col_at(t: &Table, name: &str) -> Option<usize> {
    t.columns
        .iter()
        .position(|c| c.name.eq_ignore_ascii_case(name))
}

fn same(a: &str, b: &str) -> bool {
    a.eq_ignore_ascii_case(b)
}

impl Db<'_> {
    fn notice(&mut self, line: u32, msg: String) {
        self.notices.push(format!("{}:{line}: {msg}", self.path));
    }

    fn apply(&mut self, toks: &[Tok]) {
        let mut c = Cur::new(toks);
        if c.eat("CREATE") {
            self.create(c);
        } else if c.eat("ALTER") && c.eat("TABLE") {
            self.alter(c);
        } else if c.eat("DROP")
            && (c.eat("TABLE") || c.eat("VIEW") || c.eat_all(&["MATERIALIZED", "VIEW"]))
        {
            self.drop(c);
        }
    }

    fn create(&mut self, mut c: Cur) {
        let range = Range {
            start_line: c.t[0].line,
            end_line: c.t.last().map_or(1, |t| t.line),
        };
        let mut temp = false;
        let kind = loop {
            let Some(t) = c.peek() else { return };
            c.i += 1;
            if t.is("TABLE") {
                break "table";
            } else if t.is("VIEW") {
                break "view";
            } else if t.is("TEMP") || t.is("TEMPORARY") {
                temp = true;
            } else if t.p("=") {
                c.i += 1;
                if c.at_p("@") {
                    c.i += 2;
                }
            } else if !MODIFIERS.iter().any(|m| t.is(m)) {
                return;
            }
        };
        let if_not = c.eat_all(&["IF", "NOT", "EXISTS"]);
        let Some(name) = c.name() else { return };
        if temp {
            return;
        }
        let at = find(&self.tables, &name);
        if if_not && at.is_some() {
            return;
        }
        let table = if kind == "view" {
            self.view(name, &mut c, range)
        } else {
            self.table(name, &mut c, range)
        };
        let Some(table) = table else { return };
        match at {
            Some(i) => self.tables[i] = table,
            None => self.tables.push(table),
        }
    }

    fn table(&mut self, name: String, c: &mut Cur, range: Range) -> Option<Table> {
        let mut t = Table {
            name,
            kind: "table".into(),
            source: "sql".into(),
            file: self.path.into(),
            range,
            columns: vec![],
            reads: vec![],
        };
        if c.at("AS") || c.at("PARTITION") || c.at("LIKE") {
            if c.eat("AS") {
                t.columns = select_columns(c.rest(), &self.tables).0;
                return Some(t);
            }
            return None;
        }
        let body = c.group()?;
        let mut cons = vec![];
        for el in commas(body) {
            match constraint(el, self.src) {
                Some(Some(con)) => cons.push((el[0].line, con)),
                Some(None) => {}
                None => t.columns.extend(column(el, self.src, &mut self.hints)),
            }
        }
        for tok in c.rest() {
            if tok.is("ENGINE") {
                self.hints.mysql += 1;
            } else if tok.is("ROWID") {
                self.hints.sqlite += 1;
            }
        }
        self.named.retain(|n| !same(&n.table, &t.name));
        for c in &t.columns {
            self.column_names(&t.name, c);
        }
        for (_, (name, con)) in cons {
            self.constrain(&mut t, name, con);
        }
        Some(t)
    }

    /// Registers the names Postgres gives a column's inline constraints.
    fn column_names(&mut self, table: &str, c: &Column) {
        let short = table.rsplit('.').next().unwrap_or(table);
        let cols = vec![c.name.clone()];
        let mut found = vec![];
        if c.pk {
            found.push(("pkey".to_string(), Con::Pk(cols.clone())));
        }
        if let Some(r) = &c.references {
            let refs = r.column.iter().cloned().collect();
            found.push(("fkey".into(), Con::Fk(cols.clone(), r.table.clone(), refs)));
        }
        if c.unique {
            found.push(("key".into(), Con::Unique(cols.clone())));
        }
        if let Some(check) = &c.check {
            found.push(("check".into(), Con::Check(check.clone())));
        }
        for (suffix, con) in found {
            let name = match suffix.as_str() {
                "pkey" => format!("{short}_pkey"),
                _ => format!("{short}_{}_{suffix}", c.name),
            };
            self.named.push(Named {
                table: table.into(),
                name,
                con,
            });
        }
    }

    /// Applies a table constraint to `t`'s columns and remembers it by name.
    fn constrain(&mut self, t: &mut Table, name: Option<String>, con: Con) {
        let short = t.name.rsplit('.').next().unwrap_or(&t.name).to_string();
        let default = match &con {
            Con::Pk(_) => format!("{short}_pkey"),
            Con::Fk(cols, ..) => format!("{short}_{}_fkey", cols.join("_")),
            Con::Unique(cols) => format!("{short}_{}_key", cols.join("_")),
            Con::Check(_) => format!("{short}_check"),
        };
        match &con {
            Con::Pk(cols) => {
                for c in t
                    .columns
                    .iter_mut()
                    .filter(|c| cols.iter().any(|n| same(n, &c.name)))
                {
                    c.pk = true;
                    c.nullable = false;
                }
            }
            Con::Fk(cols, table, refs) => {
                for (i, n) in cols.iter().enumerate() {
                    if let Some(c) = t.columns.iter_mut().find(|c| same(&c.name, n)) {
                        c.references = Some(ColumnRef {
                            table: table.clone(),
                            column: refs.get(i).cloned(),
                        });
                    }
                }
            }
            Con::Unique(cols) => {
                if let [one] = cols.as_slice()
                    && let Some(c) = t.columns.iter_mut().find(|c| same(&c.name, one))
                {
                    c.unique = true;
                }
            }
            Con::Check(expr) => {
                let toks = statements(expr, &mut Hints::default()).concat();
                let mut hit: Vec<usize> = toks
                    .iter()
                    .filter(|t| t.ident())
                    .filter_map(|w| col_at(t, &w.text))
                    .collect();
                hit.dedup();
                if let [one] = hit.as_slice() {
                    let c = &mut t.columns[*one];
                    c.check = Some(and(c.check.take(), expr.clone()));
                }
            }
        }
        self.named.push(Named {
            table: t.name.clone(),
            name: name.unwrap_or(default),
            con,
        });
    }

    fn view(&mut self, name: String, c: &mut Cur, range: Range) -> Option<Table> {
        let header = c.group().map(names);
        if c.eat("WITH") {
            c.group();
        }
        if !c.eat("AS") {
            return None;
        }
        let body = c.rest();
        let (mut columns, reads) = select_columns(body, &self.tables);
        if let Some(header) = header {
            if columns.len() != header.len() {
                columns = header
                    .iter()
                    .map(|n| plain(n, "", range.start_line))
                    .collect();
            }
            for (c, n) in columns.iter_mut().zip(header) {
                c.name = n;
            }
        }
        Some(Table {
            name,
            kind: "view".into(),
            source: "sql".into(),
            file: self.path.into(),
            range,
            columns,
            reads,
        })
    }
}

/// Words allowed between `CREATE` and `TABLE` or `VIEW`.
const MODIFIERS: &[&str] = &[
    "OR",
    "REPLACE",
    "UNLOGGED",
    "GLOBAL",
    "LOCAL",
    "MATERIALIZED",
    "RECURSIVE",
    "FOREIGN",
    "ALGORITHM",
    "DEFINER",
    "SQL",
    "SECURITY",
    "INVOKER",
    "UNDEFINED",
    "MERGE",
    "TEMPTABLE",
];

/// The tables and views `files` leave once every statement applies in path order.
pub fn read(files: &[(&str, &str)]) -> Found {
    let mut files: Vec<&(&str, &str)> = files.iter().collect();
    files.sort_by(|a, b| natural(a.0, b.0));
    let mut db = Db {
        tables: vec![],
        named: vec![],
        hints: Hints::default(),
        notices: vec![],
        path: "",
        src: "",
    };
    for (path, src) in files {
        if !MARKER.is_match(src) {
            continue;
        }
        db.path = path;
        db.src = src;
        for stmt in statements(src, &mut db.hints) {
            db.apply(&stmt);
        }
    }
    Found {
        dialect: db.hints.dialect(),
        tables: db.tables,
        notices: db.notices,
    }
}

static MARKER: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\b(?:create|alter|drop)\b").unwrap());
