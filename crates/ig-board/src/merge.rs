//! Three-way merge of the board on disk (theirs) into the board on screen (ours): the user wins.

use crate::{Lane, RowKind};
use ig_design::{Board, Bridge, Design, Element, ElementKind, Link, Mark, is_element_id, new_id};
use std::collections::{HashMap, HashSet};

pub struct Merged {
    pub board: Board,
    /// Bare keys theirs brought in: element ids, link keys, mark nodes, bridge keys.
    pub changed: Vec<String>,
    pub removed: Vec<String>,
    /// `(theirs, new)`: an element both sides added under one id; theirs got a fresh id.
    pub renamed: Vec<(String, String)>,
}

#[derive(Clone, PartialEq)]
enum Item<'a> {
    E(&'a Element),
    L(&'a Link),
    M(&'a Mark),
    B(&'a Bridge),
}

type Keyed<'a> = (Vec<String>, HashMap<String, Item<'a>>);

fn keyed(d: &Design) -> Keyed<'_> {
    let mut v: Vec<(String, Item<'_>)> = vec![];
    v.extend(
        d.elements
            .iter()
            .map(|e| (format!("e:{}", e.id), Item::E(e))),
    );
    v.extend(
        d.links
            .iter()
            .map(|l| (format!("l:{}", l.key()), Item::L(l))),
    );
    v.extend(
        d.marks
            .iter()
            .map(|m| (format!("m:{}", m.node), Item::M(m))),
    );
    v.extend(
        d.bridges
            .iter()
            .map(|b| (format!("b:{}", b.key()), Item::B(b))),
    );
    (
        v.iter().map(|(k, _)| k.clone()).collect(),
        v.into_iter().collect(),
    )
}

/// Elements both sides added under one id with different content: theirs gets a fresh id, so
/// the agent's element is never silently lost.
fn reid(base: &Design, ours: &Design, theirs: &Design) -> (Design, Vec<(String, String)>) {
    let mut t = theirs.clone();
    let mut taken = ours.clone();
    taken.elements.extend(theirs.elements.iter().cloned());
    let mut renamed = vec![];
    for e in &theirs.elements {
        let clash = !base.elements.iter().any(|b| b.id == e.id)
            && ours.elements.iter().any(|o| o.id == e.id && o != e);
        if !clash {
            continue;
        }
        let new = new_id(&taken, e.kind, &e.name);
        let mut n = e.clone();
        n.id = new.clone();
        taken.elements.push(n);
        let re = |s: &mut String| {
            if *s == e.id {
                *s = new.clone();
            }
        };
        for x in &mut t.elements {
            re(&mut x.id);
            x.on.as_mut().map(re);
            x.frame.as_mut().map(re);
        }
        t.links.iter_mut().for_each(|l| {
            re(&mut l.from);
            re(&mut l.to);
        });
        t.bridges.iter_mut().for_each(|b| re(&mut b.to));
        t.marks.iter_mut().for_each(|m| re(&mut m.node));
        renamed.push((e.id.clone(), new));
    }
    (t, renamed)
}

pub fn merge(base: &Board, ours: &Board, theirs: &Board) -> Merged {
    let (b, o) = (&base.design, &ours.design);
    let (t, renamed) = reid(b, o, &theirs.design);
    let ((_, kb), (order, ko), (tord, kt)) = (keyed(b), keyed(o), keyed(&t));
    let (mut changed, mut removed) = (vec![], vec![]);
    let mut d = o.clone();
    if o.title == b.title {
        d.title = t.title.clone();
    }
    if o.scope == b.scope {
        d.scope = t.scope.clone();
    }
    (d.elements, d.links, d.marks, d.bridges) = (vec![], vec![], vec![], vec![]);
    let seen: HashSet<&String> = order.iter().collect();
    let keys = order
        .iter()
        .chain(tord.iter().filter(|k| !seen.contains(k)));
    for k in keys {
        let (bv, ov, tv) = (kb.get(k), ko.get(k), kt.get(k));
        let theirs_wins = tv != bv && ov == bv;
        if theirs_wins && tv != ov {
            if tv.is_none() {
                &mut removed
            } else {
                &mut changed
            }
            .push(k.clone());
        }
        match if theirs_wins { tv } else { ov } {
            Some(Item::E(e)) => d.elements.push((*e).clone()),
            Some(Item::L(l)) => d.links.push((*l).clone()),
            Some(Item::M(m)) => d.marks.push((*m).clone()),
            Some(Item::B(x)) => d.bridges.push((*x).clone()),
            None => {}
        }
    }
    // dangling ends lapse: an end that looks like an element id must name one
    let ids: HashSet<String> = d.elements.iter().map(|e| e.id.clone()).collect();
    let live = |s: &str| !is_element_id(s) || ids.contains(s);
    d.links.retain(|l| live(&l.from) && live(&l.to));
    d.bridges.retain(|x| live(&x.to));
    d.marks.retain(|m| live(&m.node));
    for e in &mut d.elements {
        if e.on.as_deref().is_some_and(|s| !live(s)) {
            e.on = None;
        }
        if e.frame.as_deref().is_some_and(|s| !ids.contains(s)) {
            e.frame = None;
        }
    }
    let alive = |k: &str| match k.split_at(2) {
        ("e:", id) => ids.contains(id),
        ("l:", key) => d.links.iter().any(|l| l.key() == key),
        ("m:", n) => d.marks.iter().any(|m| m.node == n),
        ("b:", key) => d.bridges.iter().any(|x| x.key() == key),
        _ => true,
    };
    let (ch, gone): (Vec<String>, Vec<String>) = changed.into_iter().partition(|k| alive(k));
    removed.extend(gone);
    let bare = |v: Vec<String>| {
        let mut out: Vec<String> = vec![];
        for k in v {
            let k = k[2..].to_string();
            if !out.contains(&k) {
                out.push(k);
            }
        }
        out
    };
    Merged {
        board: Board { design: d },
        changed: bare(ch),
        removed: bare(removed),
        renamed,
    }
}

/// Puts unplaced elements (no x/y) in their lane: layer elements in their layer's row, systems
/// in External services, the rest below the rows.
pub fn place_new(b: &Board, ids: &[String], lanes: &[Lane]) -> Board {
    let mut out = b.clone();
    let bottom = lanes.iter().map(|l| l.bottom).fold(0.0, f32::max);
    let mut next_x: HashMap<Option<u32>, i32> = HashMap::new();
    for e in out
        .design
        .elements
        .iter_mut()
        .filter(|e| ids.contains(&e.id))
    {
        if e.x.is_some() && e.y.is_some() {
            continue;
        }
        let lane = lanes.iter().find(|l| {
            if e.kind == ElementKind::System {
                l.kind == RowKind::External
            } else {
                l.kind == RowKind::Layer && e.layer.as_deref() == Some(l.name.as_str())
            }
        });
        let x = next_x.entry(lane.map(|l| l.row)).or_insert(80);
        e.x = Some(*x);
        *x += 160;
        e.y = Some(crate::snap(lane.map_or(bottom, |l| l.top) + 60.0));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::snap::tests::board;
    use ig_design::{LinkKind, MarkKind, validate};

    fn el(id: &str, name: &str) -> Element {
        let mut e = crate::snap::tests::el(id, ElementKind::Function);
        (e.name, e.x, e.y, e.w, e.h) = (name.into(), None, None, None, None);
        e
    }
    fn ids(b: &Board) -> Vec<&str> {
        let mut v: Vec<&str> = b.design.elements.iter().map(|e| e.id.as_str()).collect();
        v.sort();
        v
    }
    fn base() -> Board {
        board(vec![el("fn-a", "a"), el("fn-b", "b")])
    }

    #[test]
    fn user_and_agent_edits_on_different_elements_both_survive() {
        let ours = board(vec![el("fn-a", "A"), el("fn-b", "b")]);
        let theirs = board(vec![el("fn-a", "a"), el("fn-b", "B")]);
        let r = merge(&base(), &ours, &theirs);
        let mut names: Vec<_> = r.board.design.elements.iter().map(|e| &e.name).collect();
        names.sort();
        assert_eq!(names, ["A", "B"]);
        assert_eq!(r.changed, ["fn-b"]);
    }

    #[test]
    fn the_user_wins_on_the_same_element() {
        let ours = board(vec![el("fn-a", "U"), el("fn-b", "b")]);
        let theirs = board(vec![el("fn-a", "T"), el("fn-b", "b")]);
        let r = merge(&base(), &ours, &theirs);
        assert_eq!(r.board.design.elements[0].name, "U");
        assert!(r.changed.is_empty());
    }

    #[test]
    fn an_agent_removal_of_an_element_the_user_edited_is_dropped() {
        let ours = board(vec![el("fn-a", "U"), el("fn-b", "b")]);
        let theirs = board(vec![el("fn-b", "b")]);
        assert_eq!(ids(&merge(&base(), &ours, &theirs).board), ["fn-a", "fn-b"]);
        let r = merge(&base(), &base(), &theirs);
        assert_eq!(ids(&r.board), ["fn-b"]);
        assert_eq!(r.removed, ["fn-a"]);
    }

    #[test]
    fn links_and_bridges_to_a_removed_element_are_dropped() {
        let mut b = base();
        b.design.links = vec![Link {
            from: "fn-a".into(),
            to: "fn-b".into(),
            kind: LinkKind::Calls,
        }];
        b.design.bridges = vec![Bridge {
            from: "s/btn".into(),
            to: "fn-b".into(),
        }];
        let mut t = b.clone();
        t.design.elements.truncate(1);
        let r = merge(&b, &b, &t);
        assert!(r.board.design.links.is_empty() && r.board.design.bridges.is_empty());
        assert!(validate(&r.board.design).is_ok());
    }

    #[test]
    fn agent_additions_arrive_marks_merge_by_node() {
        let mut t = board(vec![el("fn-a", "a"), el("fn-b", "b"), el("fn-c", "c")]);
        t.design.marks = vec![Mark {
            node: "x.ts".into(),
            mark: MarkKind::Changed,
        }];
        let r = merge(&base(), &base(), &t);
        assert_eq!(ids(&r.board), ["fn-a", "fn-b", "fn-c"]);
        assert_eq!(r.board.design.marks.len(), 1);
        let mut ch = r.changed.clone();
        ch.sort();
        assert_eq!(ch, ["fn-c", "x.ts"]);
    }

    #[test]
    fn merge_clears_dangling_on_and_frame() {
        let mut b = board(vec![el("fn-a", "a"), el("fn-c", "c"), el("note-1", "n")]);
        b.design.elements[2].kind = ElementKind::Note;
        b.design.elements[2].on = Some("fn-a".into());
        b.design.elements[1].frame = Some("fn-a".into());
        let mut ours = b.clone();
        ours.design.elements[1].name = "C".into();
        ours.design.elements[2].name = "N".into();
        let mut theirs = b.clone();
        theirs.design.elements.remove(0);
        theirs.design.elements[0].frame = None;
        theirs.design.elements[1].on = None;
        let r = merge(&b, &ours, &theirs);
        assert_eq!(ids(&r.board), ["fn-c", "note-1"]);
        let d = &r.board.design;
        assert_eq!(
            (d.elements[0].frame.clone(), d.elements[1].on.clone()),
            (None, None)
        );
        assert!(validate(d).is_ok());
    }

    #[test]
    fn concurrent_same_id_adds_keep_the_agents_element() {
        let mut ours = base();
        ours.design.elements.push(el("fn-x", "x"));
        let mut theirs = base();
        let mut x = el("fn-x", "x");
        x.file = Some("a.ts".into());
        theirs.design.elements.push(x);
        theirs.design.links = vec![Link {
            from: "fn-a".into(),
            to: "fn-x".into(),
            kind: LinkKind::Calls,
        }];
        let r = merge(&base(), &ours, &theirs);
        assert_eq!(ids(&r.board), ["fn-a", "fn-b", "fn-x", "fn-x-2"]);
        assert_eq!(r.renamed, [("fn-x".to_string(), "fn-x-2".to_string())]);
        assert!(r.changed.contains(&"fn-x-2".to_string()));
        assert_eq!(r.board.design.links[0].to, "fn-x-2");
        assert!(validate(&r.board.design).is_ok());
    }

    #[test]
    fn a_bridge_whose_from_names_a_removed_element_stays() {
        let mut b = base();
        b.design.bridges = vec![Bridge {
            from: "login/fn-a".into(),
            to: "fn-b".into(),
        }];
        let mut t = b.clone();
        t.design.elements.remove(0);
        let r = merge(&b, &b, &t);
        assert_eq!(ids(&r.board), ["fn-b"]);
        assert_eq!(
            r.board.design.bridges.len(),
            1,
            "bridges cascade by `to` only"
        );
        assert!(validate(&r.board.design).is_ok());
    }

    #[test]
    fn place_new_puts_an_api_element_into_the_api_row() {
        let l = |row, name: &str, top, bottom| Lane {
            row,
            name: name.into(),
            kind: RowKind::Layer,
            top,
            bottom,
        };
        let lanes = [l(0, "ui", 0.0, 100.0), l(1, "api", 150.0, 250.0)];
        let mut a = el("fn-a", "a");
        a.layer = Some("api".into());
        let b = place_new(&board(vec![a]), &["fn-a".into()], &lanes);
        assert_eq!(
            (b.design.elements[0].x, b.design.elements[0].y),
            (Some(80), Some(208))
        );
    }
}
