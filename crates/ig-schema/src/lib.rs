//! A database's schema read from the files that declare it: SQL DDL and migrations (applied in
//! natural path order, down migrations skipped), a Prisma schema, Django and SQLAlchemy models,
//! Drizzle tables and TypeORM entities. Each reader passes over a file without its marker. The
//! merge keeps one declaration per table, an ORM's over SQL's, and points a foreign key left to
//! a table's primary key at that column.

mod django;
mod drizzle;
mod prisma;
mod py;
mod sql;
mod sqlalchemy;
mod text;
mod typeorm;

use ig_core::model::{Schema, Table};

/// The schema reader `ig` runs: `wants` and `read` behind the port.
pub struct SchemaFiles;

impl ig_core::ports::SchemaReader for SchemaFiles {
    fn wants(&self, path: &str) -> bool {
        wants(path)
    }

    fn read(&self, files: &[(String, String)]) -> (Schema, Vec<String>) {
        read(files)
    }
}

/// What one reader found.
#[derive(Default)]
struct Found {
    tables: Vec<Table>,
    dialect: Option<String>,
    notices: Vec<String>,
}

/// Whether `path` may declare tables, so is worth passing to `read`.
pub fn wants(path: &str) -> bool {
    let p = path.to_ascii_lowercase();
    !p.ends_with(".d.ts")
        && [
            ".sql", ".prisma", ".py", ".ts", ".mts", ".cts", ".js", ".mjs", ".cjs",
        ]
        .iter()
        .any(|e| p.ends_with(e))
}

/// The schema `files` (path, text) declare, and notices on what could not be read.
pub fn read(files: &[(String, String)]) -> (Schema, Vec<String>) {
    let with = |exts: &[&str]| -> Vec<(&str, &str)> {
        (files.iter())
            .filter(|(p, _)| wants(p) && exts.iter().any(|e| p.to_ascii_lowercase().ends_with(e)))
            .map(|(p, t)| (p.as_str(), t.as_str()))
            .collect()
    };
    let prisma = prisma::read(&with(&[".prisma"]));
    let schemas: Vec<&str> = prisma.tables.iter().map(|t| dir(&t.file)).collect();
    let sql_files: Vec<(&str, &str)> = (with(&[".sql"]).into_iter())
        .filter(|(p, _)| !sql::is_down(p) && !generated_by_prisma(p, &schemas))
        .collect();
    let py: Vec<(&str, &str)> = (with(&[".py"]).into_iter())
        .filter(|(p, _)| !in_folder(p, "migrations"))
        .collect();
    let js = with(&[".ts", ".mts", ".cts", ".js", ".mjs", ".cjs"]);
    merge(vec![
        prisma,
        drizzle::read(&js),
        typeorm::read(&js),
        sqlalchemy::read(&py),
        django::read(&py),
        sql::read(&sql_files),
    ])
}

fn dir(path: &str) -> &str {
    path.rsplit_once('/').map_or("", |(d, _)| d)
}

fn in_folder(path: &str, folder: &str) -> bool {
    dir(path).split('/').any(|s| s == folder)
}

/// A SQL file under a `migrations/` folder that sits next to a Prisma schema (`prisma/migrations/`).
fn generated_by_prisma(path: &str, schemas: &[&str]) -> bool {
    let parts: Vec<&str> = dir(path).split('/').collect();
    (0..parts.len())
        .filter(|&i| parts[i] == "migrations")
        .any(|i| {
            let parent = parts[..i].join("/");
            let within = |d: &&str| {
                *d == parent || !parent.is_empty() && d.starts_with(&format!("{parent}/"))
            };
            !schemas.is_empty()
                && (parts[..i].last() == Some(&"prisma") || schemas.iter().any(within))
        })
}

/// The readers' tables, in priority order, merged: the first declaration of a name wins.
fn merge(found: Vec<Found>) -> (Schema, Vec<String>) {
    let mut tables: Vec<Table> = vec![];
    let mut notices = vec![];
    let mut dialect = None;
    for f in found {
        notices.extend(f.notices);
        let orm = f.tables.first().is_some_and(|t| t.source != "sql");
        dialect = dialect.or(f.dialect);
        for t in f.tables {
            match tables.iter().find(|x| x.name.eq_ignore_ascii_case(&t.name)) {
                None => tables.push(t),
                Some(x) if orm && x.source != t.source && x.source != "sql" && differ(x, &t) => {
                    notices.push(format!(
                        "{}:{}: table {} is declared differently by {} in {}; kept that one",
                        t.file, t.range.start_line, t.name, x.source, x.file
                    ));
                }
                Some(_) => {}
            }
        }
    }
    resolve(&mut tables);
    tables.sort_by(|a, b| (a.name.to_lowercase(), &a.name).cmp(&(b.name.to_lowercase(), &b.name)));
    (Schema { dialect, tables }, notices)
}

/// Whether two declarations of a table name different columns.
fn differ(a: &Table, b: &Table) -> bool {
    let names = |t: &Table| {
        let mut n: Vec<String> = t.columns.iter().map(|c| c.name.to_lowercase()).collect();
        n.sort();
        n
    };
    names(a) != names(b)
}

/// Index of the table `name`, case-insensitive; a bare name also finds a schema-qualified one.
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

/// Foreign keys pointed at their table's declared spelling, one left to the primary key at
/// that key's column when there is just one, and an untyped key typed as its target.
fn resolve(tables: &mut [Table]) {
    let mut fixes = vec![];
    for (ti, t) in tables.iter().enumerate() {
        for (ci, c) in t.columns.iter().enumerate() {
            let Some(r) = &c.references else { continue };
            let Some(target) = find(tables, &r.table).map(|i| &tables[i]) else {
                continue;
            };
            let column = r.column.clone().or_else(|| {
                match target
                    .columns
                    .iter()
                    .filter(|c| c.pk)
                    .collect::<Vec<_>>()
                    .as_slice()
                {
                    [one] => Some(one.name.clone()),
                    _ => None,
                }
            });
            let ty = (target.columns.iter())
                .find(|tc| {
                    column
                        .as_deref()
                        .is_some_and(|n| tc.name.eq_ignore_ascii_case(n))
                })
                .map(|tc| tc.ty.clone());
            fixes.push((ti, ci, target.name.clone(), column, ty));
        }
    }
    for (ti, ci, table, column, ty) in fixes {
        let c = &mut tables[ti].columns[ci];
        if c.ty.is_empty() {
            c.ty = ty.unwrap_or_default();
        }
        c.references = Some(ig_core::model::ColumnRef { table, column });
    }
}

#[cfg(test)]
mod tests;
