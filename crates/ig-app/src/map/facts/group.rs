//! Past 12 containers the root is grouped: the packages by top folder (`apps/`, `packages/`),
//! then the outside systems into one box, then the smallest into `other`. A grouped container
//! becomes a component of its group; its own components are not shown.

use super::{BoxKind, FactBox, Level, MAX_COMPONENTS, cap, guess, slug};
use std::collections::{BTreeMap, HashSet};

/// A box at the root: one container, or a group of them (id base, title, kind).
struct Unit {
    group: Option<(String, String, BoxKind)>,
    members: Vec<FactBox>,
}

pub(super) fn group_containers(out: &mut Vec<FactBox>) {
    let is_top = |b: &FactBox| b.level == Level::Container;
    if out.iter().filter(|b| is_top(b)).count() <= MAX_COMPONENTS {
        return;
    }
    let (tops, mut rest): (Vec<FactBox>, Vec<FactBox>) = out.drain(..).partition(is_top);
    let mut units: Vec<Unit> = vec![];
    let mut outside = vec![];
    let mut by_top: BTreeMap<String, Vec<FactBox>> = BTreeMap::new();
    for c in tops {
        let top = c
            .made_of
            .first()
            .and_then(|g| g.split_once('/'))
            .map(|t| t.0);
        match (&c.package, top) {
            (Some(_), Some(t)) => by_top.entry(t.to_string()).or_default().push(c),
            (Some(_), None) => units.push(plain(c)),
            (None, _) => outside.push(c),
        }
    }
    for (top, members) in by_top {
        units.push(if members.len() == 1 {
            plain(members.into_iter().next().unwrap())
        } else {
            let title = format!("{top} ({} packages)", members.len());
            Unit {
                group: Some((slug(&top, "group"), title, BoxKind::Group)),
                members,
            }
        });
    }
    if units.len() + outside.len() > MAX_COMPONENTS && outside.len() > 1 {
        let title = format!("Outside ({} systems)", outside.len());
        units.push(Unit {
            group: Some(("outside".into(), title, BoxKind::External)),
            members: outside,
        });
    } else {
        units.extend(outside.into_iter().map(plain));
    }
    if units.len() > MAX_COMPONENTS {
        units.sort_by_key(|u| std::cmp::Reverse(files_of(&u.members)));
        let merged: Vec<FactBox> = units
            .split_off(MAX_COMPONENTS - 1)
            .into_iter()
            .flat_map(|u| u.members)
            .collect();
        let title = format!("Other ({} containers)", merged.len());
        units.push(Unit {
            group: Some(("other".into(), title, BoxKind::Group)),
            members: merged,
        });
    }
    let mut taken: HashSet<String> = units
        .iter()
        .filter(|u| u.group.is_none())
        .map(|u| u.members[0].id.clone())
        .chain(["system".to_string()])
        .collect();
    for u in units {
        let Some((base, title, kind)) = u.group else {
            out.extend(u.members);
            continue;
        };
        let mut id = base.clone();
        for n in 2.. {
            if taken.insert(id.clone()) {
                break;
            }
            id = format!("{base}-{n}");
        }
        let mut nodes: Vec<String> = u.members.iter().flat_map(|m| m.nodes.clone()).collect();
        nodes.sort();
        out.push(FactBox {
            id: id.clone(),
            parent: Some("system".into()),
            level: Level::Container,
            made_of: u.members.iter().flat_map(|m| m.made_of.clone()).collect(),
            package: None,
            files: files_of(&u.members),
            nodes,
            guess: guess(kind, title, None, None),
        });
        let gone: HashSet<&str> = u.members.iter().map(|m| m.id.as_str()).collect();
        rest.retain(|b| !b.parent.as_deref().is_some_and(|p| gone.contains(p)));
        let mut kids: Vec<FactBox> = u
            .members
            .iter()
            .map(|m| FactBox {
                id: format!("{id}/{}", m.id),
                parent: Some(id.clone()),
                level: Level::Component,
                guess: super::Guess {
                    pattern: None,
                    slot: None,
                    ..m.guess.clone()
                },
                ..m.clone()
            })
            .collect();
        cap(&mut kids, &id, None, None, "containers");
        out.extend(kids);
    }
    out.extend(rest);
}

fn files_of(ms: &[FactBox]) -> u32 {
    ms.iter().map(|m| m.files).sum()
}

fn plain(c: FactBox) -> Unit {
    Unit {
        group: None,
        members: vec![c],
    }
}
