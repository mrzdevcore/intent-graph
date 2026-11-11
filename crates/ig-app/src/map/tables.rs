//! The tables level: a database's tables and views as boxes inside the store they are kept in,
//! with an arrow for each foreign key (`uses`, labelled with its columns) and from each view to
//! what it reads (`reads`).

use super::arrows::{ArrowKind, MapArrow};
use super::externals;
use super::facts::{BoxKind, FactBox, Guess, Level};
use super::file::{MapBox, MapFile, fresh};
use ig_core::model::{Schema, Table};
use std::collections::{BTreeMap, HashMap};

/// The tables level of a map.
#[derive(Default)]
pub struct Tables {
    pub boxes: Vec<MapBox>,
    pub arrows: Vec<MapArrow>,
    /// Each table box's table, by box id.
    pub tables: BTreeMap<String, Table>,
    pub notices: Vec<String>,
}

/// `db`'s tables as boxes in the store of `m` they are kept in (`store`); none, with a notice,
/// when `m` has no store for them.
pub fn tables(m: &MapFile, db: &Schema) -> Tables {
    if db.tables.is_empty() {
        return Tables::default();
    }
    let Some(store) = store(m, db) else {
        return Tables {
            notices: vec![format!(
                "{} tables read, but no store on the map to draw them in",
                db.tables.len()
            )],
            ..Tables::default()
        };
    };
    let id_of: HashMap<String, String> = (db.tables.iter())
        .map(|t| (t.name.to_lowercase(), format!("{}#{}", store.id, t.name)))
        .collect();
    let of = |name: &str| id_of.get(&name.to_lowercase());
    let mut out = Tables::default();
    for t in &db.tables {
        let id = id_of[&t.name.to_lowercase()].clone();
        out.boxes.push(MapBox {
            needs_reading: false,
            ..fresh(&FactBox {
                id: id.clone(),
                parent: Some(store.id.clone()),
                level: Level::Code,
                made_of: vec![t.file.clone()],
                package: None,
                files: 1,
                nodes: vec![],
                guess: Guess {
                    kind: BoxKind::Table,
                    title: t.name.clone(),
                    pattern: None,
                    slot: None,
                    logo: None,
                },
            })
        });
        // one arrow per table referenced, labelled with the columns that reference it
        let mut refs: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
        for c in &t.columns {
            if let Some(to) = c.references.as_ref().and_then(|r| of(&r.table)) {
                refs.entry(to).or_default().push(&c.name);
            }
        }
        for (to, cols) in refs {
            out.arrows.push(arrow(
                &store.id,
                &id,
                to,
                ArrowKind::Uses,
                Some(cols.join(", ")),
            ));
        }
        for to in t.reads.iter().filter_map(|r| of(r)) {
            out.arrows
                .push(arrow(&store.id, &id, to, ArrowKind::Reads, None));
        }
        out.tables.insert(id, t.clone());
    }
    out.arrows.retain(|a| a.from != a.to);
    out.arrows.sort_by(|a, b| a.id.cmp(&b.id));
    out.arrows.dedup_by(|a, b| a.id == b.id);
    out
}

fn arrow(parent: &str, from: &str, to: &str, kind: ArrowKind, label: Option<String>) -> MapArrow {
    MapArrow {
        note: None,
        context: None,
        shape: None,
        written: Default::default(),
        seen: vec![],
        id: format!("{from}->{to}"),
        parent: parent.into(),
        from: from.into(),
        to: to.into(),
        kind,
        label,
        count: 1,
        kinds: vec![kind],
        in_loop: false,
        origin: Default::default(),
        parts: vec![],
    }
}

/// The store `db`'s tables are kept in: the database of its dialect, else another SQL
/// database, else a store the person drew; never a cache, a bucket or a search index.
fn store<'m>(m: &'m MapFile, db: &Schema) -> Option<&'m MapBox> {
    let want = externals::for_dialect(db.dialect.as_deref()).id;
    let rank = |b: &MapBox| -> Option<u8> {
        let known = (b.nodes.iter())
            .filter_map(|n| externals::by_title(n.strip_prefix("system:")?))
            .next()
            .or_else(|| externals::by_title(&b.title.value));
        match known {
            Some(k) if k.id == want => Some(0),
            Some(k) if externals::relational(k.id) && k.id != "mongodb" => Some(1),
            Some(_) => None,
            None => Some(2),
        }
    };
    (m.boxes.iter())
        .filter(|b| b.kind.value == BoxKind::Store && !b.stale)
        .filter_map(|b| Some((rank(b)?, b.id.as_str(), b)))
        .min_by_key(|(r, id, _)| (*r, *id))
        .map(|(_, _, b)| b)
}

#[cfg(test)]
mod tests;
