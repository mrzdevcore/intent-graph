//! The tables level: a database's tables and views as boxes inside the store they are kept in,
//! grouped by domain (set aside: views, leftovers, translations), with an arrow for each foreign
//! key (`uses`, with its columns and ends), each join table (`via`), and each view's reads.

use super::arrows::{ArrowKind, End, Ends, MapArrow};
use super::externals;
use super::facts::{BoxKind, FactBox, Guess, Level};
use super::file::{MapBox, MapFile, fresh};
use ig_core::model::{Column, Schema, Table};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet, HashMap};

const BOOKKEEPING: [&str; 4] = ["deleted", "created_at", "updated_at", "deleted_at"];
const HUB: usize = 10;
const DOMAIN: usize = 3;
const JOIN_EXTRA: usize = 2;
// the readers that always say whether a column is nullable (crates/ig-schema)
const TELLS_NULLABLE: [&str; 6] = [
    "sql",
    "prisma",
    "django",
    "sqlalchemy",
    "drizzle",
    "typeorm",
];
const LOCALE: [&str; 3] = ["locale", "lang", "language"];

#[derive(Default)]
pub struct Tables {
    pub boxes: Vec<MapBox>,
    pub arrows: Vec<MapArrow>,
    pub tables: BTreeMap<String, TableView>,
    pub groups: BTreeMap<String, TableGroup>,
    pub folded: BTreeSet<String>,
    pub notices: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TableView {
    #[serde(flatten)]
    pub table: Table,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub translations: Vec<String>,
    pub nullability: bool,
}

impl TableView {
    // as the viewer's keys-only card draws it (viewer/src/map/db/card.ts)
    pub fn card_rows(&self) -> u32 {
        let cols = &self.table.columns;
        let keys = cols
            .iter()
            .filter(|c| c.pk || c.references.is_some())
            .count();
        (keys + (cols.len() > keys) as usize + !self.translations.is_empty() as usize) as u32
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum TableGroup {
    Domain,
    Other,
    Views,
    Leftovers,
    Translations,
}

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
    let by_name: HashMap<String, &Table> = (db.tables.iter())
        .map(|t| (t.name.to_lowercase(), t))
        .collect();
    let id = |t: &Table| format!("{}#{}", store.id, t.name);
    let of = |name: &str| by_name.get(&name.to_lowercase()).copied();
    let placed = place(db, &by_name);
    let group_id = |t: &Table| placed[&t.name.to_lowercase()].0.clone();
    let loose = !placed.values().any(|g| g.1 == TableGroup::Domain);
    let in_store = |kind: TableGroup| loose && kind == TableGroup::Other;
    let group_of = |t: &Table| -> String {
        match &placed[&t.name.to_lowercase()] {
            (_, kind, _) if in_store(*kind) => store.id.clone(),
            (g, _, _) => format!("{}#{g}", store.id),
        }
    };
    let parent = |a: &Table, b: &Table| {
        if group_id(a) == group_id(b) {
            group_of(a)
        } else {
            store.id.clone()
        }
    };

    let mut out = Tables::default();
    let mut members: BTreeMap<String, (TableGroup, String, Vec<&Table>)> = BTreeMap::new();
    for t in &db.tables {
        let (g, kind, title) = &placed[&t.name.to_lowercase()];
        (members.entry(g.clone()))
            .or_insert_with(|| (*kind, title.clone(), vec![]))
            .2
            .push(t);
    }
    for (g, (kind, title, ts)) in members.iter().filter(|(_, m)| !in_store(m.0)) {
        let gid = format!("{}#{g}", store.id);
        let files: BTreeSet<String> = ts.iter().map(|t| t.file.clone()).collect();
        out.boxes.push(made(FactBox {
            id: gid.clone(),
            parent: Some(store.id.clone()),
            level: Level::Component,
            files: files.len() as u32,
            made_of: files.into_iter().collect(),
            package: None,
            nodes: vec![],
            guess: guess(BoxKind::Group, title.clone()),
        }));
        if *kind == TableGroup::Translations {
            out.folded.insert(gid.clone());
        }
        out.groups.insert(gid, *kind);
    }

    let mut translations: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for t in &db.tables {
        let name = t.name.to_lowercase();
        let aside = placed[&name].1 == TableGroup::Translations;
        if let Some(of_parent) = translated(&name, &by_name).filter(|_| aside) {
            let cols = (t.columns.iter())
                .filter(|c| !c.pk && c.references.is_none() && !bookkeeping(c))
                .filter(|c| !LOCALE.contains(&c.name.to_lowercase().as_str()))
                .map(|c| c.name.clone());
            let all = translations.entry(of_parent).or_default();
            for c in cols {
                if !all.contains(&c) {
                    all.push(c);
                }
            }
        }
    }

    for t in &db.tables {
        let tid = id(t);
        out.boxes.push(made(FactBox {
            id: tid.clone(),
            parent: Some(group_of(t)),
            level: Level::Code,
            made_of: vec![t.file.clone()],
            package: None,
            files: 1,
            nodes: vec![],
            guess: guess(BoxKind::Table, t.name.clone()),
        }));
        let mut refs: BTreeMap<String, (&Table, Vec<&Column>)> = BTreeMap::new();
        for c in &t.columns {
            let Some(to) = c.references.as_ref().and_then(|r| of(&r.table)) else {
                continue;
            };
            if to.name.eq_ignore_ascii_case(&t.name) {
                continue;
            }
            refs.entry(id(to)).or_insert((to, vec![])).1.push(c);
        }
        for (to_id, (to, cols)) in refs {
            let names: Vec<&str> = cols.iter().map(|c| c.name.as_str()).collect();
            let mut a = arrow(
                &parent(t, to),
                &tid,
                &to_id,
                ArrowKind::Uses,
                Some(names.join(", ")),
            );
            a.from_column = Some(cols[0].name.clone());
            a.to_column = referenced(cols[0], to);
            a.ends = ends(t, &cols);
            out.arrows.push(a);
        }
        let translation = placed[&t.name.to_lowercase()].1 == TableGroup::Translations;
        if translation {
            out.folded.insert(tid.clone());
        }
        if let Some((a, ca, b, cb)) = join(t, &of).filter(|_| !translation) {
            out.folded.insert(tid.clone());
            let mut v = arrow(
                &parent(a, b),
                &id(a),
                &id(b),
                ArrowKind::Uses,
                Some(t.name.clone()),
            );
            v.id = format!("{} via {}", v.id, t.name);
            v.from_column = referenced(ca, a);
            v.to_column = referenced(cb, b);
            v.ends = Some(Ends {
                from: End::Many0,
                to: End::Many0,
            });
            v.via = Some(t.name.clone());
            out.arrows.push(v);
        }
        for to in t.reads.iter().filter_map(|r| of(r)) {
            out.arrows
                .push(arrow(&parent(t, to), &tid, &id(to), ArrowKind::Reads, None));
        }
        let translations = translations
            .remove(&t.name.to_lowercase())
            .unwrap_or_default();
        out.tables.insert(
            tid,
            TableView {
                table: t.clone(),
                translations,
                nullability: tells_nullable(t),
            },
        );
    }
    for gid in out.groups.keys() {
        let mut kids = out
            .boxes
            .iter()
            .filter(|b| b.parent.as_deref() == Some(gid));
        if kids.all(|b| out.folded.contains(&b.id)) {
            out.folded.insert(gid.clone());
        }
    }
    out.arrows.retain(|a| a.from != a.to);
    out.arrows.sort_by(|a, b| a.id.cmp(&b.id));
    out.arrows.dedup_by(|a, b| a.id == b.id);

    let mut pointing: HashMap<&str, BTreeSet<&str>> = HashMap::new();
    for a in out
        .arrows
        .iter()
        .filter(|a| a.kind == ArrowKind::Uses && a.via.is_none() && !out.folded.contains(&a.from))
    {
        pointing.entry(&a.to).or_default().insert(&a.from);
    }
    let hubs: BTreeSet<String> = (pointing.into_iter())
        .filter(|(_, from)| from.len() >= HUB)
        .map(|(to, _)| to.to_string())
        .collect();
    for a in out.arrows.iter_mut().filter(|a| a.kind == ArrowKind::Uses) {
        a.pinned = hubs.contains(&a.to);
    }
    out
}

pub(super) fn unfolded(
    folded: &BTreeSet<String>,
    boxes: &[MapBox],
    arrows: &[MapArrow],
) -> (Vec<MapBox>, Vec<MapArrow>) {
    let kept = |id: &String| !folded.contains(id);
    (
        (boxes.iter()).filter(|b| kept(&b.id)).cloned().collect(),
        (arrows.iter())
            .filter(|a| kept(&a.from) && kept(&a.to))
            .cloned()
            .collect(),
    )
}

fn place(
    db: &Schema,
    by_name: &HashMap<String, &Table>,
) -> HashMap<String, (String, TableGroup, String)> {
    let set_aside = |name: &str, t: &Table| -> Option<TableGroup> {
        if t.kind == "view" {
            Some(TableGroup::Views)
        } else if leftover(name, t, by_name) {
            Some(TableGroup::Leftovers)
        } else if translated(name, by_name).is_some() {
            Some(TableGroup::Translations)
        } else {
            None
        }
    };
    let prefix = |name: &str| name.split('_').next().unwrap_or_default().to_string();
    let mut count: HashMap<String, usize> = HashMap::new();
    for t in &db.tables {
        let name = t.name.to_lowercase();
        if set_aside(&name, t).is_none() {
            *count.entry(prefix(&name)).or_default() += 1;
        }
    }
    let aside = |g: TableGroup| {
        let word = format!("{g:?}").to_lowercase();
        (format!("({word})"), g, word)
    };
    (db.tables.iter())
        .map(|t| {
            let name = t.name.to_lowercase();
            let p = prefix(&name);
            let g = match set_aside(&name, t) {
                Some(g) => aside(g),
                None if !p.is_empty() && count.get(&p).copied().unwrap_or(0) >= DOMAIN => {
                    (format!("{p}_*"), TableGroup::Domain, p)
                }
                None => aside(TableGroup::Other),
            };
            (name, g)
        })
        .collect()
}

// `orders_backup`, `orders_backup_2023_01_01`, `orders_temp`, and `orders_backup_old` when it
// copies at least half of `orders`' columns; not `user_backup_codes` of `user`
fn leftover(name: &str, t: &Table, by_name: &HashMap<String, &Table>) -> bool {
    if name.ends_with("_temp") {
        return true;
    }
    name.match_indices("_backup").any(|(i, m)| {
        let Some(rest) = name[i + m.len()..].strip_prefix('_') else {
            return i + m.len() == name.len();
        };
        let dated = rest
            .chars()
            .all(|c| c.is_ascii_digit() || c == '_' || c == '-');
        !rest.is_empty() && (dated || by_name.get(&name[..i]).is_some_and(|of| copies(t, of)))
    })
}

fn copies(t: &Table, of: &Table) -> bool {
    let mine: BTreeSet<String> = t.columns.iter().map(|c| c.name.to_lowercase()).collect();
    let same = (of.columns.iter())
        .filter(|c| mine.contains(&c.name.to_lowercase()))
        .count();
    !of.columns.is_empty() && 2 * same >= of.columns.len()
}

// `products_translations` of `products`, `product_type_translations` of `product_types`
fn translated(name: &str, by_name: &HashMap<String, &Table>) -> Option<String> {
    let p = name
        .strip_suffix("_translations")
        .filter(|p| !p.is_empty())?;
    [p.to_string(), format!("{p}s")]
        .into_iter()
        .find(|p| by_name.contains_key(p))
}

fn tells_nullable(t: &Table) -> bool {
    TELLS_NULLABLE.contains(&t.source.as_str())
}

fn bookkeeping(c: &Column) -> bool {
    BOOKKEEPING.contains(&c.name.to_lowercase().as_str())
}

fn referenced(c: &Column, to: &Table) -> Option<String> {
    if let Some(col) = c.references.as_ref().and_then(|r| r.column.clone()) {
        return Some(col);
    }
    let mut pk = to.columns.iter().filter(|c| c.pk);
    match (pk.next(), pk.next()) {
        (Some(c), None) => Some(c.name.clone()),
        _ => None,
    }
}

// none rather than a guess: a reader that does not tell nullability, or columns that disagree
fn ends(t: &Table, cols: &[&Column]) -> Option<Ends> {
    if !tells_nullable(t) {
        return None;
    }
    let pk: Vec<&str> = t
        .columns
        .iter()
        .filter(|c| c.pk)
        .map(|c| c.name.as_str())
        .collect();
    let one = |c: &Column| Ends {
        from: if c.unique || pk == [c.name.as_str()] {
            End::ZeroOne
        } else {
            End::Many0
        },
        to: if c.nullable { End::ZeroOne } else { End::One },
    };
    let first = one(cols[0]);
    cols.iter().all(|c| one(c) == first).then_some(first)
}

fn join<'t>(
    t: &'t Table,
    of: &impl Fn(&str) -> Option<&'t Table>,
) -> Option<(&'t Table, &'t Column, &'t Table, &'t Column)> {
    if t.kind == "view" {
        return None;
    }
    let pk: Vec<&Column> = t.columns.iter().filter(|c| c.pk).collect();
    let &[a, b] = pk.as_slice() else {
        return None;
    };
    let ta = of(&a.references.as_ref()?.table)?;
    let tb = of(&b.references.as_ref()?.table)?;
    let me = |x: &Table| x.name.eq_ignore_ascii_case(&t.name);
    if me(ta) || me(tb) || ta.name.eq_ignore_ascii_case(&tb.name) {
        return None;
    }
    let extra = (t.columns.iter())
        .filter(|c| !c.pk && !bookkeeping(c))
        .count();
    (extra <= JOIN_EXTRA).then_some((ta, a, tb, b))
}

fn guess(kind: BoxKind, title: String) -> Guess {
    Guess {
        kind,
        title,
        pattern: None,
        slot: None,
        logo: None,
    }
}

fn made(f: FactBox) -> MapBox {
    MapBox {
        needs_reading: false,
        ..fresh(&f)
    }
}

fn arrow(parent: &str, from: &str, to: &str, kind: ArrowKind, label: Option<String>) -> MapArrow {
    MapArrow {
        id: format!("{from}->{to}"),
        parent: parent.into(),
        from: from.into(),
        to: to.into(),
        kind,
        label,
        count: 1,
        kinds: vec![kind],
        ..Default::default()
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
