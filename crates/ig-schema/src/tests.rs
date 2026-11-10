use super::*;
use ig_core::model::Column;

mod merge;
mod orm;
mod prisma;
mod sql;

/// The schema `files` (path, text) declare.
fn schema(files: &[(&str, &str)]) -> (Schema, Vec<String>) {
    let owned: Vec<(String, String)> = (files.iter())
        .map(|(p, t)| (p.to_string(), t.to_string()))
        .collect();
    read(&owned)
}

fn table<'a>(s: &'a Schema, name: &str) -> &'a Table {
    (s.tables.iter())
        .find(|t| t.name == name)
        .unwrap_or_else(|| panic!("no table {name} in {:?}", names(s)))
}

fn col<'a>(t: &'a Table, name: &str) -> &'a Column {
    (t.columns.iter())
        .find(|c| c.name == name)
        .unwrap_or_else(|| panic!("no column {name} in {}: {:?}", t.name, cols(t)))
}

fn names(s: &Schema) -> Vec<&str> {
    s.tables.iter().map(|t| t.name.as_str()).collect()
}

fn cols(t: &Table) -> Vec<&str> {
    t.columns.iter().map(|c| c.name.as_str()).collect()
}

/// A column's foreign key as (table, column).
fn fk(c: &Column) -> Option<(&str, Option<&str>)> {
    (c.references.as_ref()).map(|r| (r.table.as_str(), r.column.as_deref()))
}
