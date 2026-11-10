//! The tables of two sides compared: tables and views added, removed, and changed column by
//! column (added, removed, retyped, a key or nullability changed).

use ig_core::model::{Column, Schema, Table};
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TableChanges {
    pub added: Vec<TableRef>,
    pub removed: Vec<TableRef>,
    pub changed: Vec<ChangedTable>,
}

impl TableChanges {
    pub fn is_empty(&self) -> bool {
        self.added.is_empty() && self.removed.is_empty() && self.changed.is_empty()
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TableRef {
    pub name: String,
    /// "table" or "view".
    pub kind: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangedTable {
    pub name: String,
    pub kind: String,
    pub added: Vec<Column>,
    /// The base's columns, gone from the head.
    pub removed: Vec<Column>,
    pub changed: Vec<ChangedColumn>,
}

/// A column both sides have: what it was, and in `what` the words for what changed ("type",
/// "nullable", "primary key", "foreign key", "unique", "default", "check").
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangedColumn {
    pub name: String,
    pub base: Column,
    pub what: Vec<&'static str>,
}

/// `head`'s tables against `base`'s, matched by name whatever the case.
pub fn tables(base: &Schema, head: &Schema) -> TableChanges {
    let (b, h) = (by_name(base), by_name(head));
    let r = |t: &Table| TableRef {
        name: t.name.clone(),
        kind: t.kind.clone(),
    };
    let mut out = TableChanges {
        added: (h.iter())
            .filter(|(k, _)| !b.contains_key(*k))
            .map(|(_, t)| r(t))
            .collect(),
        removed: (b.iter())
            .filter(|(k, _)| !h.contains_key(*k))
            .map(|(_, t)| r(t))
            .collect(),
        changed: vec![],
    };
    for (k, ht) in &h {
        if let Some(bt) = b.get(k)
            && let Some(c) = changed(bt, ht)
        {
            out.changed.push(c);
        }
    }
    out
}

fn by_name(s: &Schema) -> BTreeMap<String, &Table> {
    (s.tables.iter())
        .map(|t| (t.name.to_lowercase(), t))
        .collect()
}

fn changed(base: &Table, head: &Table) -> Option<ChangedTable> {
    let find = |t: &Table, name: &str| -> Option<Column> {
        (t.columns.iter())
            .find(|c| c.name.eq_ignore_ascii_case(name))
            .cloned()
    };
    let added: Vec<Column> = (head.columns.iter())
        .filter(|c| find(base, &c.name).is_none())
        .cloned()
        .collect();
    let removed: Vec<Column> = (base.columns.iter())
        .filter(|c| find(head, &c.name).is_none())
        .cloned()
        .collect();
    let changed: Vec<ChangedColumn> = (head.columns.iter())
        .filter_map(|c| {
            let was = find(base, &c.name)?;
            let what = differs(&was, c);
            (!what.is_empty()).then(|| ChangedColumn {
                name: c.name.clone(),
                base: was,
                what,
            })
        })
        .collect();
    if added.is_empty() && removed.is_empty() && changed.is_empty() && base.kind == head.kind {
        return None;
    }
    Some(ChangedTable {
        name: head.name.clone(),
        kind: head.kind.clone(),
        added,
        removed,
        changed,
    })
}

fn differs(a: &Column, b: &Column) -> Vec<&'static str> {
    let fk = |c: &Column| {
        c.references.as_ref().map(|r| {
            (
                r.table.to_lowercase(),
                r.column.as_ref().map(|c| c.to_lowercase()),
            )
        })
    };
    [
        (!a.ty.eq_ignore_ascii_case(&b.ty), "type"),
        (a.nullable != b.nullable, "nullable"),
        (a.pk != b.pk, "primary key"),
        (fk(a) != fk(b), "foreign key"),
        (a.unique != b.unique, "unique"),
        (a.default != b.default, "default"),
        (a.check != b.check, "check"),
    ]
    .into_iter()
    .filter_map(|(d, w)| d.then_some(w))
    .collect()
}

/// One line each, as `ig diff` prints them: `table +  name`, `table ~  name: + col type, …`.
pub fn lines(t: &TableChanges) -> Vec<String> {
    let kind = |k: &str| if k == "view" { "view " } else { "table" };
    let mut out: Vec<String> = vec![];
    for a in &t.added {
        out.push(format!("{} +  {}", kind(&a.kind), a.name));
    }
    for r in &t.removed {
        out.push(format!("{} −  {}", kind(&r.kind), r.name));
    }
    for c in &t.changed {
        let cols = (c.added.iter())
            .map(|a| format!("+ {} {}", a.name, a.ty).trim_end().to_string())
            .chain(
                c.changed
                    .iter()
                    .map(|x| format!("~ {} ({})", x.name, x.what.join(", "))),
            )
            .chain(c.removed.iter().map(|r| format!("− {}", r.name)));
        out.push(format!(
            "{} ~  {}: {}",
            kind(&c.kind),
            c.name,
            cols.collect::<Vec<_>>().join(", ")
        ));
    }
    out
}

#[cfg(test)]
mod tests;
