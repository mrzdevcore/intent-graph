//! Arrows between boxes: each graph edge lifted to the children of its ends' lowest common parent.

use super::facts::{BoxKind, FactBox};
use ig_core::model::{Edge, Graph};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ArrowKind {
    #[serde(alias = "network")]
    Calls,
    #[serde(alias = "event")]
    Sends,
    #[serde(alias = "data")]
    Reads,
    Writes,
    #[serde(alias = "depends")]
    Uses,
    Implements,
    Triggers,
    Bundle,
}

/// Every kind written (all but `bundle`), the most telling first: what a bundle is written as
/// in the spec is its most telling kind.
pub const TELLING: [ArrowKind; 7] = [
    ArrowKind::Calls,
    ArrowKind::Sends,
    ArrowKind::Reads,
    ArrowKind::Writes,
    ArrowKind::Triggers,
    ArrowKind::Implements,
    ArrowKind::Uses,
];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MapArrow {
    /// `"{from}->{to}"`.
    pub id: String,
    /// The box both ends sit in.
    pub parent: String,
    pub from: String,
    pub to: String,
    pub kind: ArrowKind,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub label: Option<String>,
    /// Graph edges lifted into this arrow.
    pub count: u32,
    pub kinds: Vec<ArrowKind>,
    #[serde(skip_serializing_if = "std::ops::Not::not", default)]
    pub in_loop: bool,
    /// Built, to build or drift: the spec against the code.
    #[serde(skip_serializing_if = "super::origin::is_built", default)]
    pub origin: super::origin::Origin,
    /// The arrows as written, when this one is drawn between boxes above their ends.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub parts: Vec<MapArrow>,
}

impl MapArrow {
    /// The kind the spec says: a bundle's most telling kind, any other arrow's own.
    pub fn spec_kind(&self) -> ArrowKind {
        if self.kind != ArrowKind::Bundle {
            return self.kind;
        }
        let first = TELLING.iter().find(|k| self.kinds.contains(k));
        first.copied().unwrap_or(ArrowKind::Uses)
    }
}

/// The graph edges lifted into one arrow, with their arrow kinds.
type Lifted<'g> = Vec<(&'g Edge, ArrowKind)>;

/// Bundle label order and nouns.
const NOUNS: &[(ArrowKind, &str)] = &[
    (ArrowKind::Calls, "call"),
    (ArrowKind::Sends, "message"),
    (ArrowKind::Reads, "read"),
    (ArrowKind::Writes, "write"),
    (ArrowKind::Triggers, "trigger"),
    (ArrowKind::Implements, "implementation"),
    (ArrowKind::Uses, "use"),
];

/// The arrow kind of a graph edge of kind `kind` into a box of kind `to`; none for edges not drawn.
pub(super) fn edge_kind(kind: &str, to: BoxKind) -> Option<ArrowKind> {
    Some(match kind {
        "imports" => ArrowKind::Uses,
        "calls" | "renders" | "dispatch" => ArrowKind::Calls,
        "implements" | "extends" => ArrowKind::Implements,
        "http" => ArrowKind::Calls,
        "uses" => match to {
            BoxKind::Store | BoxKind::Queue => ArrowKind::Reads,
            BoxKind::External => ArrowKind::Calls,
            _ => ArrowKind::Uses,
        },
        _ => return None,
    })
}

/// Every graph edge between two boxes, lifted to the children of their lowest common parent and
/// merged per pair; sorted by id. Edges inside one box, or from a box's loose files into its own
/// children, are not drawn.
pub fn arrows(boxes: &[FactBox], g: &Graph) -> Vec<MapArrow> {
    let by_id: HashMap<&str, &FactBox> = boxes.iter().map(|b| (b.id.as_str(), b)).collect();
    // Node → its deepest box (deeper boxes overwrite; code boxes nest at any depth).
    let depth = |b: &FactBox| {
        let mut d = 0;
        let mut p = b.parent.as_deref();
        while let Some(x) = p.filter(|_| d <= boxes.len()) {
            d += 1;
            p = by_id.get(x).and_then(|b| b.parent.as_deref());
        }
        d
    };
    let mut sorted: Vec<(usize, &FactBox)> = boxes.iter().map(|b| (depth(b), b)).collect();
    sorted.sort_by_key(|b| b.0);
    let sorted = sorted.into_iter().map(|b| b.1);
    let mut owner: HashMap<&str, &str> = HashMap::new();
    for b in sorted {
        for n in &b.nodes {
            owner.insert(n, &b.id);
        }
    }
    let box_of = |id: &str| -> Option<&str> {
        let module = id.split('#').next().unwrap_or(id);
        owner.get(id).or_else(|| owner.get(module)).copied()
    };
    /// `id` and its ancestors, innermost first.
    fn chain<'b>(by_id: &HashMap<&str, &'b FactBox>, mut id: &'b str) -> Vec<&'b str> {
        let mut out = vec![id];
        while let Some(p) = by_id.get(id).and_then(|b| b.parent.as_deref()) {
            out.push(p);
            id = p;
        }
        out
    }
    let names: HashMap<&str, &str> = g
        .nodes
        .iter()
        .map(|n| (n.id.as_str(), n.name.as_str()))
        .collect();
    let modules: HashSet<&str> = (g.nodes.iter().filter(|n| n.is_module()))
        .map(|n| n.id.as_str())
        .collect();
    let mut pairs: BTreeMap<(&str, &str, &str), Lifted> = BTreeMap::new();
    for e in &g.edges {
        let (Some(a), Some(b)) = (box_of(&e.from), box_of(&e.to)) else {
            continue;
        };
        let (ca, cb) = (chain(&by_id, a), chain(&by_id, b));
        let Some(i) = ca.iter().position(|x| cb.contains(x)) else {
            continue;
        };
        let j = cb.iter().position(|x| *x == ca[i]).unwrap();
        if i == 0 || j == 0 {
            continue;
        }
        let Some(kind) = edge_kind(&e.kind, by_id[b].guess.kind) else {
            continue;
        };
        pairs
            .entry((ca[i], ca[i - 1], cb[j - 1]))
            .or_default()
            .push((e, kind));
    }
    let mut out: Vec<MapArrow> = pairs
        .into_iter()
        .map(|((parent, from, to), edges)| {
            let mut kinds: Vec<ArrowKind> = edges.iter().map(|(_, k)| *k).collect();
            kinds.sort();
            kinds.dedup();
            let kind = if kinds.len() == 1 {
                kinds[0]
            } else {
                ArrowKind::Bundle
            };
            // One distinct target name among the edges of `kind` (`only` restricts edge kinds).
            let single = |only: Option<&str>| -> Option<String> {
                let mut t: Vec<&str> = edges
                    .iter()
                    .filter(|(e, _)| only.is_none_or(|o| e.kind == o))
                    .map(|(e, _)| {
                        names
                            .get(e.to.as_str())
                            .copied()
                            .unwrap_or_else(|| e.to.rsplit('#').next().unwrap_or(&e.to))
                    })
                    .collect();
                t.sort();
                t.dedup();
                // a file's name says nothing about the link
                let to_file = edges.iter().any(|(e, _)| modules.contains(e.to.as_str()));
                (t.len() == 1 && !to_file).then(|| t[0].to_string())
            };
            let label = match kind {
                ArrowKind::Bundle => Some(
                    NOUNS
                        .iter()
                        .filter_map(|(k, noun)| {
                            let n = edges.iter().filter(|(_, x)| x == k).count();
                            (n > 0).then(|| format!("{n} {noun}{}", if n > 1 { "s" } else { "" }))
                        })
                        .collect::<Vec<_>>()
                        .join(" · "),
                ),
                ArrowKind::Calls if edges.iter().all(|(e, _)| e.kind == "http") => {
                    single(Some("http"))
                }
                ArrowKind::Calls => single(None),
                _ => None,
            };
            MapArrow {
                id: format!("{from}->{to}"),
                parent: parent.into(),
                from: from.into(),
                to: to.into(),
                kind,
                label,
                count: edges.len() as u32,
                kinds,
                in_loop: edges.iter().any(|(e, _)| e.in_loop),
                origin: Default::default(),
                parts: vec![],
            }
        })
        .collect();
    out.sort_by(|a, b| a.id.cmp(&b.id));
    out
}

/// `arrows` as drawn: each end lifted to the child of the arrow's parent that holds it, so an
/// arrow between boxes at different depths (a spec's `integrations/gmail -> gmail`) starts and
/// ends at boxes of the level it is drawn in. Lifted arrows of one origin that land on the same
/// pair merge (a bundle when their kinds differ) under the first one's id, labelled by their
/// count; each stays as written in `parts`. An arrow already between boxes of its level is left
/// as it is.
pub fn lift(parents: &HashMap<&str, &str>, arrows: Vec<MapArrow>) -> Vec<MapArrow> {
    let up = |id: &str, parent: &str| -> Option<String> {
        let mut id = id.to_string();
        for _ in 0..=parents.len() {
            let p = *parents.get(id.as_str())?;
            if p == parent {
                return Some(id);
            }
            id = p.to_string();
        }
        None
    };
    let mut out = vec![];
    let mut pairs: BTreeMap<(String, String, String, String), Vec<MapArrow>> = BTreeMap::new();
    for mut a in arrows {
        let (Some(from), Some(to)) = (up(&a.from, &a.parent), up(&a.to, &a.parent)) else {
            continue;
        };
        if (from.as_str(), to.as_str()) == (a.from.as_str(), a.to.as_str()) {
            out.push(a);
        } else if from != to {
            let key = (
                a.parent.clone(),
                from.clone(),
                to.clone(),
                format!("{:?}", a.origin),
            );
            a.parts = vec![a.clone()];
            (a.from, a.to) = (from, to);
            pairs.entry(key).or_default().push(a);
        }
    }
    out.extend(pairs.into_values().map(|mut list| {
        if list.len() == 1 {
            return list.pop().unwrap();
        }
        let mut kinds: Vec<ArrowKind> = list.iter().flat_map(|a| a.kinds.clone()).collect();
        kinds.sort();
        kinds.dedup();
        let bundle = kinds.len() > 1;
        let label = (NOUNS.iter())
            .filter_map(|(k, noun)| {
                let n = list.iter().filter(|a| a.kinds.contains(k)).count();
                (n > 0).then(|| format!("{n} {noun}{}", if n > 1 { "s" } else { "" }))
            })
            .collect::<Vec<_>>()
            .join(" · ");
        let count = list.iter().map(|a| a.count.max(1)).sum();
        let in_loop = list.iter().any(|a| a.in_loop);
        let parts = list.iter().flat_map(|a| a.parts.clone()).collect();
        let first = list.swap_remove(0);
        MapArrow {
            kind: if bundle { ArrowKind::Bundle } else { kinds[0] },
            label: Some(label),
            count,
            kinds,
            in_loop,
            parts,
            ..first
        }
    }));
    out.sort_by(|a, b| a.id.cmp(&b.id));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::facts::{Guess, Level, facts};

    fn find<'a>(a: &'a [MapArrow], id: &str) -> &'a MapArrow {
        a.iter()
            .find(|x| x.id == id)
            .unwrap_or_else(|| panic!("no arrow {id} in {:#?}", a))
    }

    #[test]
    fn lifts_to_common_parent() {
        let (ws, g, cfg) = crate::map::shop::load();
        let a = arrows(&facts(&ws, &g, &cfg), &g);
        let net = find(&a, "web->api");
        assert_eq!(
            (net.kind, net.parent.as_str()),
            (ArrowKind::Calls, "system")
        );
        assert_eq!(net.label.as_deref(), Some("POST /checkout"));
        let dep = find(&a, "api/domain-order->api/infra-prisma");
        assert_eq!((dep.kind, dep.parent.as_str()), (ArrowKind::Uses, "api"));
        assert!(find(&a, "api/application-checkout->api/infra-prisma").in_loop);
        assert_eq!(find(&a, "api->postgres").kind, ArrowKind::Reads);
        assert_eq!(find(&a, "api->stripe").kind, ArrowKind::Calls);
    }

    fn bx(id: &str, parent: Option<&str>, level: Level, nodes: &[&str]) -> FactBox {
        FactBox {
            id: id.into(),
            parent: parent.map(String::from),
            level,
            made_of: vec![],
            package: None,
            files: nodes.len() as u32,
            nodes: nodes.iter().map(|n| n.to_string()).collect(),
            guess: Guess {
                kind: BoxKind::Usecase,
                title: id.into(),
                pattern: None,
                slot: None,
                logo: None,
            },
        }
    }

    fn edge(from: &str, kind: &str, to: &str) -> Edge {
        Edge {
            from: from.into(),
            to: to.into(),
            kind: kind.into(),
            confidence: "resolved".into(),
            file: "a.ts".into(),
            line: 1,
            namespace: false,
            reexport: false,
            in_loop: false,
        }
    }

    #[test]
    fn bundles_mixed_kinds() {
        let boxes = [
            bx("system", None, Level::System, &[]),
            bx("c", Some("system"), Level::Container, &["a.ts", "b.ts"]),
            bx("c/a", Some("c"), Level::Component, &["a.ts"]),
            bx("c/b", Some("c"), Level::Component, &["b.ts"]),
        ];
        let mut g = Graph::empty();
        g.edges = vec![
            edge("a.ts#f", "calls", "b.ts#g1"),
            edge("a.ts#f", "calls", "b.ts#g2"),
            edge("a.ts#f", "calls", "b.ts#g3"),
            edge("a.ts", "imports", "b.ts"),
        ];
        let a = arrows(&boxes, &g);
        assert_eq!(a.len(), 1);
        assert_eq!(a[0].id, "c/a->c/b");
        assert_eq!(a[0].kind, ArrowKind::Bundle);
        assert_eq!(a[0].label.as_deref(), Some("3 calls · 1 use"));
        assert_eq!(a[0].count, 4);
        assert_eq!(a[0].kinds, [ArrowKind::Calls, ArrowKind::Uses]);
    }

    fn arrow(from: &str, to: &str, parent: &str, kind: ArrowKind) -> MapArrow {
        MapArrow {
            id: format!("{from}->{to}"),
            parent: parent.into(),
            from: from.into(),
            to: to.into(),
            kind,
            label: None,
            count: 0,
            kinds: vec![kind],
            in_loop: false,
            origin: Default::default(),
            parts: vec![],
        }
    }

    #[test]
    fn arrows_between_depths_are_drawn_between_the_boxes_of_their_level() {
        let parents: HashMap<&str, &str> = [
            ("ad", "system"),
            ("ad/gmail", "ad"),
            ("ad/sqlite", "ad"),
            ("agent", "system"),
            ("agent/ports", "agent"),
            ("agent/cli", "agent"),
            ("gmail", "system"),
            ("me", "system"),
        ]
        .into();
        let a = lift(
            &parents,
            vec![
                arrow("ad/gmail", "gmail", "system", ArrowKind::Calls),
                arrow("ad/gmail", "agent/ports", "system", ArrowKind::Implements),
                arrow("ad/sqlite", "agent/ports", "system", ArrowKind::Implements),
                arrow("ad/sqlite", "agent/cli", "system", ArrowKind::Calls),
                arrow("ad", "agent", "system", ArrowKind::Uses),
                arrow("me", "agent/cli", "system", ArrowKind::Uses),
                arrow("agent/cli", "agent/ports", "agent", ArrowKind::Uses),
            ],
        );
        let ends: Vec<(&str, &str, &str)> = (a.iter())
            .map(|a| (a.id.as_str(), a.from.as_str(), a.to.as_str()))
            .collect();
        assert_eq!(
            ends,
            [
                ("ad->agent", "ad", "agent"),
                ("ad/gmail->agent/ports", "ad", "agent"),
                ("ad/gmail->gmail", "ad", "gmail"),
                ("agent/cli->agent/ports", "agent/cli", "agent/ports"),
                ("me->agent/cli", "me", "agent"),
            ]
        );
        // the direct `depends` stays as written; the three lifted ones merge into one bundle
        assert_eq!(a[0].kind, ArrowKind::Uses);
        assert_eq!(a[1].kind, ArrowKind::Bundle);
        assert_eq!(a[1].label.as_deref(), Some("1 call · 2 implementations"));
        assert_eq!(a[1].count, 3);
    }
}
