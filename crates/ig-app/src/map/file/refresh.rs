//! `refresh`: the new facts with the old file's reading kept.

use super::{Field, MapBox, MapFile, Source, globs, slots};
use crate::map::{BoxKind, FactBox, Level, MapArrow};
use std::collections::{BTreeMap, BTreeSet, HashSet};

/// The new facts with the old file's reading kept: annotated and pinned fields survive, new
/// boxes need reading, gone boxes are dropped unless pinned (then kept as stale). A container
/// whose old components were drawn by an agent or a person keeps those components; files none
/// of them match go into `{container}/unread`.
pub fn refresh(old: Option<&MapFile>, facts: &[FactBox], arrows: Vec<MapArrow>) -> MapFile {
    let olds: BTreeMap<&str, &MapBox> = old
        .map(|m| m.boxes.iter().map(|b| (b.id.as_str(), b)).collect())
        .unwrap_or_default();
    let is_comp =
        |b: &MapBox, c: &str| b.level == Level::Component && b.parent.as_deref() == Some(c);
    // containers in the facts whose old components someone other than ig drew
    let drawn: BTreeSet<&str> = facts
        .iter()
        .filter(|f| f.level == Level::Container)
        .map(|f| f.id.as_str())
        .filter(|c| {
            olds.values()
                .any(|o| is_comp(o, c) && o.made_of_source != Source::Ig)
        })
        .collect();
    let in_drawn = |p: &Option<String>| p.as_deref().is_some_and(|p| drawn.contains(p));
    let mut boxes: Vec<MapBox> = vec![];
    let mut files: BTreeMap<&str, Vec<String>> = BTreeMap::new();
    for f in facts {
        if f.level == Level::Component && in_drawn(&f.parent) {
            let c = f.parent.as_deref().unwrap();
            files.entry(c).or_default().extend(f.nodes.iter().cloned());
            continue;
        }
        let new = fresh(f);
        boxes.push(match olds.get(f.id.as_str()) {
            Some(o) => keep(o, new),
            None => new,
        });
    }
    for c in &drawn {
        let all = files.remove(c).unwrap_or_default();
        boxes.extend(drawn_components(c, &olds, &all, facts));
    }
    let ids: HashSet<&str> = facts.iter().map(|f| f.id.as_str()).collect();
    boxes.extend(
        olds.values()
            .filter(|o| !ids.contains(o.id.as_str()) && !in_drawn(&o.parent))
            .filter(|o| !o.pinned.is_empty())
            .map(|o| MapBox {
                stale: true,
                ..(*o).clone()
            }),
    );
    fit_slots(&mut boxes, facts);
    boxes.sort_by(|a, b| a.id.cmp(&b.id));
    let mut arrows = arrows;
    arrows.sort_by(|a, b| a.id.cmp(&b.id));
    MapFile {
        version: 1,
        boxes,
        arrows,
        marks: vec![],
    }
}

/// A slot (pinned or not) the parent's pattern no longer has goes back to ig's guess, or to
/// none when the pattern has no room for that either; a pin stands whenever it is still valid.
fn fit_slots(boxes: &mut [MapBox], facts: &[FactBox]) {
    let patterns: BTreeMap<String, Option<crate::map::Pattern>> = boxes
        .iter()
        .map(|b| (b.id.clone(), b.pattern.as_ref().map(|p| p.value.clone())))
        .collect();
    for b in boxes.iter_mut().filter(|b| !b.stale) {
        let pattern = b
            .parent
            .as_ref()
            .and_then(|p| patterns.get(p))
            .cloned()
            .flatten();
        let ok = slots(pattern.as_ref());
        if b.slot
            .as_ref()
            .is_some_and(|s| !ok.contains(&s.value.as_str()))
        {
            let guess = facts
                .iter()
                .find(|f| f.id == b.id)
                .and_then(|f| f.guess.slot.clone());
            b.slot = guess.filter(|g| ok.contains(&g.as_str())).map(Field::ig);
            b.pinned.retain(|p| p != "slot");
        }
    }
}

/// ig's own box for a fact.
fn fresh(f: &FactBox) -> MapBox {
    MapBox {
        id: f.id.clone(),
        parent: f.parent.clone(),
        level: f.level,
        made_of: f.made_of.clone(),
        made_of_source: Source::Ig,
        package: f.package.clone(),
        files: f.files,
        kind: Field::ig(f.guess.kind),
        title: Field::ig(f.guess.title.clone()),
        tech: None,
        description: None,
        logo: None,
        badges: None,
        pattern: f.guess.pattern.clone().map(Field::ig),
        slot: f.guess.slot.clone().map(Field::ig),
        service_name: None,
        pinned: vec![],
        needs_reading: true,
        stale: false,
        nodes: f.nodes.clone(),
    }
}

/// Container `c`'s old components, each with the files (of `all`) its globs match, first box
/// wins; a box matching nothing is dropped unless pinned (then stale). The rest of `all` goes
/// into `{c}/unread`.
fn drawn_components(
    c: &str,
    olds: &BTreeMap<&str, &MapBox>,
    all: &[String],
    facts: &[FactBox],
) -> Vec<MapBox> {
    let unread = format!("{c}/unread");
    let mut kept: Vec<MapBox> = olds
        .values()
        .filter(|o| o.level == Level::Component && o.parent.as_deref() == Some(c))
        .filter(|o| o.id != unread || !o.pinned.is_empty())
        .map(|o| MapBox {
            nodes: vec![],
            stale: false,
            ..(*o).clone()
        })
        .collect();
    let sets: Vec<_> = kept.iter().map(|b| globs(&b.made_of).ok()).collect();
    let mut rest = vec![];
    for f in all {
        match sets
            .iter()
            .position(|s| s.as_ref().is_some_and(|s| s.is_match(f)))
        {
            Some(k) => kept[k].nodes.push(f.clone()),
            None => rest.push(f.clone()),
        }
    }
    let mut out: Vec<MapBox> = kept
        .into_iter()
        .filter_map(|mut b| {
            b.files = b.nodes.len() as u32;
            b.stale = b.nodes.is_empty();
            (!b.stale || !b.pinned.is_empty()).then_some(b)
        })
        .collect();
    if let Some(u) = out.iter_mut().find(|b| b.id == unread)
        && !rest.is_empty()
    {
        // a person kept the unread box: it takes the new leftovers too
        u.nodes.extend(rest);
        u.made_of = folder_globs(&u.nodes, all);
        u.files = u.nodes.len() as u32;
        u.stale = false;
        u.needs_reading = true;
    } else if !rest.is_empty() {
        let container = facts.iter().find(|f| f.id == c);
        out.push(MapBox {
            id: unread,
            parent: Some(c.into()),
            level: Level::Component,
            made_of: folder_globs(&rest, all),
            made_of_source: Source::Ig,
            package: container.and_then(|f| f.package.clone()),
            files: rest.len() as u32,
            kind: Field::ig(BoxKind::Group),
            title: Field::ig("Unread".into()),
            tech: None,
            description: None,
            logo: None,
            badges: None,
            pattern: None,
            slot: None,
            service_name: None,
            pinned: vec![],
            needs_reading: true,
            stale: false,
            nodes: rest,
        });
    }
    out
}

/// Globs for exactly `rest`: `dir/*` for a folder all of whose files (in `all`) are in `rest`,
/// the file itself otherwise.
fn folder_globs(rest: &[String], all: &[String]) -> Vec<String> {
    let dir = |f: &str| {
        f.rsplit_once('/')
            .map_or(String::new(), |(d, _)| d.to_string())
    };
    let rest_set: HashSet<&String> = rest.iter().collect();
    let mut out = BTreeSet::new();
    for f in rest {
        let d = dir(f);
        let whole = all
            .iter()
            .filter(|g| dir(g) == d)
            .all(|g| rest_set.contains(g));
        out.insert(if whole && !d.is_empty() {
            format!("{}/*", globset::escape(&d))
        } else {
            globset::escape(f)
        });
    }
    out.into_iter().collect()
}

/// `new` (ig's facts and guess) with `old`'s non-ig and pinned fields.
fn keep(old: &MapBox, new: MapBox) -> MapBox {
    // the old field when pinned or not ig's own guess, else the new one
    fn pick<T: Clone>(
        pinned: bool,
        old: &Option<Field<T>>,
        new: Option<Field<T>>,
    ) -> Option<Field<T>> {
        match old {
            Some(o) if pinned || o.source != Source::Ig => Some(o.clone()),
            None if pinned => None,
            _ => new,
        }
    }
    let p = |f: &str| old.pins(f);
    let own = p("madeOf") || old.made_of_source != Source::Ig;
    MapBox {
        made_of: if own {
            old.made_of.clone()
        } else {
            new.made_of
        },
        made_of_source: if own {
            old.made_of_source
        } else {
            new.made_of_source
        },
        kind: pick(p("kind"), &Some(old.kind.clone()), Some(new.kind)).unwrap(),
        title: pick(p("title"), &Some(old.title.clone()), Some(new.title)).unwrap(),
        tech: pick(p("tech"), &old.tech, new.tech),
        description: pick(p("description"), &old.description, new.description),
        logo: pick(p("logo"), &old.logo, new.logo),
        badges: pick(p("badges"), &old.badges, new.badges),
        pattern: pick(p("pattern"), &old.pattern, new.pattern),
        slot: pick(p("slot"), &old.slot, new.slot),
        service_name: old.service_name.clone(),
        pinned: old.pinned.clone(),
        needs_reading: old.needs_reading,
        ..new
    }
}
