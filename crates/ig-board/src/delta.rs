//! What a change means: a geometry-free fingerprint and human lines for the agent.

use ig_design::{Board, Element, ElementKind, to_text};
use std::collections::{BTreeMap, BTreeSet};
use std::hash::{DefaultHasher, Hash, Hasher};

/// A hash of everything but geometry; collection order never matters.
pub fn meaning(b: &Board) -> u64 {
    let mut d = b.design.clone();
    for e in &mut d.elements {
        (e.x, e.y, e.w, e.h) = (None, None, None, None);
    }
    let mut h = DefaultHasher::new();
    to_text(&d).hash(&mut h);
    h.finish()
}

fn describe(e: &Element) -> String {
    let what = if e.kind == ElementKind::Route {
        let o = |s: &Option<String>| s.clone().unwrap_or_default();
        format!("{} {}", o(&e.method), o(&e.path))
    } else {
        e.name.clone()
    };
    match &e.file {
        Some(f) => format!("{what}, {f}"),
        None => what,
    }
}

fn fields(e: &Element) -> [(&'static str, Option<String>); 9] {
    [
        ("file", e.file.clone()),
        ("method", e.method.clone()),
        ("path", e.path.clone()),
        ("icon", e.icon.clone()),
        ("layer", e.layer.clone()),
        ("frame", e.frame.clone()),
        ("on", e.on.clone()),
        ("question", e.question.then(|| "true".into())),
        ("answer", e.answer.clone()),
    ]
}

/// Human lines for what changed; a geometry-only change gives none.
pub fn delta(prev: &Board, next: &Board) -> Vec<String> {
    let (a, b) = (&prev.design, &next.design);
    let mut out = vec![];
    let was: BTreeMap<_, _> = a.elements.iter().map(|e| (&e.id, e)).collect();
    let now: BTreeSet<_> = b.elements.iter().map(|e| &e.id).collect();
    for e in b.elements.iter().filter(|e| !was.contains_key(&e.id)) {
        out.push(format!("added {} {} ({})", kind(e.kind), e.id, describe(e)));
    }
    for e in &b.elements {
        let Some(o) = was.get(&e.id) else {
            continue;
        };
        if o.name != e.name {
            out.push(format!("renamed {}: {} → {}", e.id, o.name, e.name));
        }
        for ((f, x), (_, y)) in fields(o).into_iter().zip(fields(e)) {
            if x != y {
                let n = |v: Option<String>| v.unwrap_or_else(|| "none".into());
                out.push(format!("set {} {f}: {} → {}", e.id, n(x), n(y)));
            }
        }
    }
    for e in a.elements.iter().filter(|e| !now.contains(&e.id)) {
        out.push(format!("removed {}", e.id));
    }
    let keys = |d: &ig_design::Design| d.links.iter().map(|l| l.key()).collect::<BTreeSet<_>>();
    let (la, lb) = (keys(a), keys(b));
    for l in b.links.iter().filter(|l| !la.contains(&l.key())) {
        out.push(format!("linked {} {} {}", l.from, l.kind.as_str(), l.to));
    }
    for l in a.links.iter().filter(|l| !lb.contains(&l.key())) {
        out.push(format!("unlinked {} {} {}", l.from, l.kind.as_str(), l.to));
    }
    let marks = |d: &ig_design::Design| {
        d.marks
            .iter()
            .map(|m| (m.node.clone(), m.mark))
            .collect::<BTreeMap<_, _>>()
    };
    let (ma, mb) = (marks(a), marks(b));
    for (n, m) in &mb {
        if ma.get(n) != Some(m) {
            let m = serde_json::to_value(m).unwrap();
            out.push(format!("marked {n} {}", m.as_str().unwrap_or_default()));
        }
    }
    for n in ma.keys().filter(|n| !mb.contains_key(*n)) {
        out.push(format!("unmarked {n}"));
    }
    let bkeys = |d: &ig_design::Design| d.bridges.iter().map(|x| x.key()).collect::<BTreeSet<_>>();
    let (ba, bb) = (bkeys(a), bkeys(b));
    for x in b.bridges.iter().filter(|x| !ba.contains(&x.key())) {
        out.push(format!("bridged {} to {}", x.from, x.to));
    }
    for x in a.bridges.iter().filter(|x| !bb.contains(&x.key())) {
        out.push(format!("unbridged {} to {}", x.from, x.to));
    }
    if a.scope != b.scope {
        out.push(format!(
            "scope: {}",
            serde_json::to_string(&b.scope).unwrap()
        ));
    }
    if a.title != b.title {
        out.push(format!("title: {}", b.title));
    }
    out.extend(crate::screens::screen_delta(&prev.screens, &next.screens));
    out
}

fn kind(k: ElementKind) -> String {
    serde_json::to_value(k).unwrap().as_str().unwrap().into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::snap::tests::{board, el};
    use ig_design::{ElementPatch, LinkKind, Op, apply};

    #[test]
    fn meaning_ignores_geometry() {
        let b = board(vec![el("fn-a", ElementKind::Function)]);
        let mv = [Op::Move {
            id: "fn-a".into(),
            x: 48,
            y: 8,
        }];
        let m = apply(&b, &mv).unwrap().0;
        assert_ne!(m, b);
        assert_eq!(meaning(&m), meaning(&b));
        assert!(delta(&b, &m).is_empty());
        let r = [Op::Update {
            id: "fn-a".into(),
            set: ElementPatch {
                name: Some("z".into()),
                ..Default::default()
            },
        }];
        assert_ne!(meaning(&apply(&b, &r).unwrap().0), meaning(&b));
    }

    #[test]
    fn delta_lines() {
        let mut a = el("fn-a", ElementKind::Function);
        a.name = "a".into();
        let prev = board(vec![a]);
        let mut ops: Vec<Op> = serde_json::from_value(serde_json::json!([
            {"op":"add","id":null,"kind":"route","name":"GET /x","file":"server/api/x.get.ts",
             "method":"GET","path":"/x","layer":null,"on":null,"x":null,"y":null,"w":null,"h":null},
            {"op":"update","id":"fn-a","set":{"name":"b"}},
        ]))
        .unwrap();
        ops.push(Op::Link {
            from: "fn-a".into(),
            to: "server/r.ts#f".into(),
            kind: LinkKind::Calls,
        });
        let next = apply(&prev, &ops).unwrap().0;
        assert_eq!(
            delta(&prev, &next),
            [
                "added route route-get-x (GET /x, server/api/x.get.ts)",
                "renamed fn-a: a → b",
                "linked fn-a calls server/r.ts#f",
            ]
        );
    }
}
