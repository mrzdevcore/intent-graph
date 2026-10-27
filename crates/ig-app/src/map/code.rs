//! The code level: the functions and classes inside each component, at most 12 per component.
//! Shown only; never stored in the map file.

use super::file::{MapBox, MapFile, fresh};
use super::{BoxKind, FactBox, Guess, Level, MapArrow, arrows};
use ig_core::model::{Graph, Node};
use std::collections::{BTreeMap, HashSet};

const MAX_CODE: usize = 12;

/// A map box as the facts `arrows` lifts edges over.
pub(crate) fn as_fact(b: &MapBox) -> FactBox {
    FactBox {
        id: b.id.clone(),
        parent: b.parent.clone(),
        level: b.level,
        made_of: b.made_of.clone(),
        package: b.package.clone(),
        files: b.files,
        nodes: b.nodes.clone(),
        guess: Guess {
            kind: b.kind.value,
            title: b.title.value.clone(),
            pattern: b.pattern.as_ref().map(|p| p.value.clone()),
            slot: b.slot.as_ref().map(|s| s.value.clone()),
        },
    }
}

/// The function and class nodes of each component, each a box whose id is its graph node id.
/// Past 12, they are grouped by file (the box id is the module's), else by name prefix, the
/// smallest groups merged into `{component}/other`. Arrows between them are lifted as in
/// `arrows`, keeping those inside one component.
pub fn code_boxes(m: &MapFile, g: &Graph) -> (Vec<MapBox>, Vec<MapArrow>) {
    let mut members: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for n in &g.nodes {
        if let Some(p) = &n.parent {
            members.entry(p).or_default().push(&n.id);
        }
    }
    let mut code: Vec<FactBox> = vec![];
    for c in m
        .boxes
        .iter()
        .filter(|b| b.level == Level::Component && !b.stale)
    {
        let modules: HashSet<&str> = c.nodes.iter().map(String::as_str).collect();
        let tops: Vec<&Node> = g
            .nodes
            .iter()
            .filter(|n| matches!(n.kind.as_str(), "function" | "class"))
            .filter(|n| n.parent.as_deref().is_some_and(|p| modules.contains(p)))
            .collect();
        for (id, title, kind, group) in groups(&c.id, &tops) {
            let mut nodes = vec![];
            let mut files: Vec<String> = vec![];
            for n in &group {
                nodes.push(n.id.clone());
                nodes.extend(
                    members
                        .get(n.id.as_str())
                        .into_iter()
                        .flatten()
                        .map(|s| s.to_string()),
                );
                files.push(n.file.clone());
            }
            files.sort();
            files.dedup();
            code.push(FactBox {
                id,
                parent: Some(c.id.clone()),
                level: Level::Code,
                files: files.len() as u32,
                made_of: files,
                package: c.package.clone(),
                nodes,
                guess: Guess {
                    kind,
                    title,
                    pattern: None,
                    slot: None,
                },
            });
        }
    }
    let comps: HashSet<&str> = m
        .boxes
        .iter()
        .filter(|b| b.level == Level::Component)
        .map(|b| b.id.as_str())
        .collect();
    let all: Vec<FactBox> = m
        .boxes
        .iter()
        .map(as_fact)
        .chain(code.iter().cloned())
        .collect();
    let lifted = arrows(&all, g)
        .into_iter()
        .filter(|a| comps.contains(a.parent.as_str()))
        .collect();
    let boxes = code
        .iter()
        .map(|f| MapBox {
            needs_reading: false,
            ..fresh(f)
        })
        .collect();
    (boxes, lifted)
}

/// (id, title, kind, nodes) per code box of component `c`.
fn groups<'g>(c: &str, tops: &[&'g Node]) -> Vec<(String, String, BoxKind, Vec<&'g Node>)> {
    if tops.len() <= MAX_CODE {
        return tops
            .iter()
            .map(|n| {
                let kind = if n.kind == "class" {
                    BoxKind::Class
                } else {
                    BoxKind::Function
                };
                (n.id.clone(), n.name.clone(), kind, vec![*n])
            })
            .collect();
    }
    let mut by_file: BTreeMap<&str, Vec<&Node>> = BTreeMap::new();
    for n in tops {
        by_file
            .entry(n.parent.as_deref().unwrap())
            .or_default()
            .push(n);
    }
    let mut out: Vec<(String, String, BoxKind, Vec<&Node>)> =
        if (2..=MAX_CODE).contains(&by_file.len()) {
            by_file
                .into_iter()
                .map(|(module, ns)| {
                    let title = module.rsplit('/').next().unwrap_or(module).to_string();
                    (module.to_string(), title, BoxKind::Group, ns)
                })
                .collect()
        } else {
            let mut by_prefix: BTreeMap<&str, Vec<&Node>> = BTreeMap::new();
            for n in tops {
                by_prefix.entry(prefix(&n.name)).or_default().push(n);
            }
            by_prefix
                .into_iter()
                .map(|(p, ns)| (format!("{c}/{p}"), format!("{p}…"), BoxKind::Group, ns))
                .collect()
        };
    if out.len() > MAX_CODE {
        // the largest 11 stay; the rest share one box
        out.sort_by(|a, b| b.3.len().cmp(&a.3.len()).then(a.0.cmp(&b.0)));
        let rest: Vec<&Node> = out.drain(MAX_CODE - 1..).flat_map(|g| g.3).collect();
        out.push((format!("{c}/other"), "Other".into(), BoxKind::Group, rest));
    }
    out
}

/// A name's first word: `getUser` → `get`, `load_all` → `load`.
fn prefix(name: &str) -> &str {
    name.char_indices()
        .skip(1)
        .find(|(_, ch)| ch.is_uppercase() || *ch == '_' || ch.is_ascii_digit())
        .map_or(name, |(i, _)| &name[..i])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::facts;
    use crate::map::file::refresh;

    #[test]
    fn groups_past_twelve_code() {
        let (ws, mut g, cfg) = crate::map::shop::load();
        let b = facts(&ws, &g, &cfg);
        let m = refresh(None, &b, arrows(&b, &g), &[]).0;
        let (code, _) = code_boxes(&m, &g);
        // few functions: one box per node, id = the node id
        let order: Vec<&str> = code
            .iter()
            .filter(|b| b.parent.as_deref() == Some("api/domain-order"))
            .map(|b| b.id.as_str())
            .collect();
        assert_eq!(
            order,
            [
                "apps/api/src/domain/order/order.ts#Order",
                "apps/api/src/domain/order/order.ts#loadOrder"
            ]
        );
        let file = "apps/api/src/domain/payment/payment.ts";
        let total = g
            .nodes
            .iter()
            .find(|n| n.id == format!("{file}#total"))
            .unwrap()
            .clone();
        let words = [
            "get", "set", "load", "save", "make", "find", "list", "sync", "read", "send", "drop",
            "wrap", "fix", "run",
        ];
        for i in 0..30 {
            let mut n = total.clone();
            n.name = format!("{}Thing{i}", words[i % words.len()]);
            n.id = format!("{file}#{}", n.name);
            g.nodes.push(n);
        }
        let (code, arrows) = code_boxes(&m, &g);
        let kids: Vec<&MapBox> = code
            .iter()
            .filter(|b| b.parent.as_deref() == Some("api/domain-payment"))
            .collect();
        assert!(kids.len() <= 12, "{} children", kids.len());
        assert_eq!(kids.iter().map(|b| b.files).sum::<u32>(), kids.len() as u32);
        assert!(kids.iter().any(|b| b.id == "api/domain-payment/other"));
        let ids: HashSet<&str> = code.iter().map(|b| b.id.as_str()).collect();
        assert_eq!(ids.len(), code.len(), "ids are unique");
        assert!(
            arrows
                .iter()
                .all(|a| ids.contains(a.from.as_str()) && ids.contains(a.to.as_str()))
        );
    }
}
