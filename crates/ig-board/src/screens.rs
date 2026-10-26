//! The screens of the board: their keys, what changed between two boards, and the three-way
//! merge of screens, pieces and navs (field by field: the user wins per field).
//!
//! Keys: a screen is its id, a piece `<screen>/<piece>`, a nav `nav:<screen>/<from>><to>`.

use crate::Merged;
use crate::merge::fields;
use ig_design::{Board, Nav, Piece, Screen};
use serde::Serialize;
use serde::de::DeserializeOwned;
use std::collections::{BTreeMap, HashMap, HashSet};

pub(crate) type Screens = BTreeMap<String, Screen>;

pub fn piece_key(screen: &str, piece: &str) -> String {
    format!("{screen}/{piece}")
}

pub fn nav_key(screen: &str, n: &Nav) -> String {
    format!("nav:{screen}/{}>{}", n.from, n.to)
}

/// `(screen, piece)` of a piece key.
pub(crate) fn split_key(key: &str) -> Option<(&str, &str)> {
    key.split_once('/').filter(|_| !key.starts_with("nav:"))
}

/// `(screen, from, to)` of a nav key.
pub(crate) fn split_nav(key: &str) -> Option<(&str, &str, &str)> {
    let (s, rest) = key.strip_prefix("nav:")?.split_once('/')?;
    let (from, to) = rest.split_once('>')?;
    Some((s, from, to))
}

/// The screen alone, without its pieces and navs: what its own key covers.
fn frame(s: &Screen) -> Screen {
    Screen {
        elements: vec![],
        navs: vec![],
        ..s.clone()
    }
}

/// Every key of a set of screens, with what it stands for (compared by value).
fn keyed(sc: &Screens) -> Vec<(String, serde_json::Value)> {
    let mut v = vec![];
    for (id, s) in sc {
        v.push((id.clone(), serde_json::to_value(frame(s)).unwrap()));
        for p in &s.elements {
            v.push((piece_key(id, &p.id), serde_json::to_value(p).unwrap()));
        }
        for n in &s.navs {
            v.push((nav_key(id, n), serde_json::to_value(n).unwrap()));
        }
    }
    v
}

/// `(changed or added in b, gone from a)` keys.
pub(crate) fn diff(a: &Screens, b: &Screens) -> (Vec<String>, Vec<String>) {
    let ka: HashMap<String, serde_json::Value> = keyed(a).into_iter().collect();
    let kb = keyed(b);
    let in_b: HashSet<&String> = kb.iter().map(|(k, _)| k).collect();
    let changed = (kb.iter())
        .filter(|(k, v)| ka.get(k) != Some(v))
        .map(|(k, _)| k.clone())
        .collect();
    let gone = (keyed(a).into_iter())
        .filter(|(k, _)| !in_b.contains(k))
        .map(|(k, _)| k)
        .collect();
    (changed, gone)
}

/// Three-way merge of a keyed list: ours where ours changed it, else theirs; an item both
/// changed is merged field by field. Ours' order first, then what theirs added.
fn list<T: Clone + PartialEq + Serialize + DeserializeOwned>(
    b: &[T],
    o: &[T],
    t: &[T],
    key: impl Fn(&T) -> String,
) -> Vec<T> {
    let map = |v: &[T]| -> HashMap<String, T> { v.iter().map(|x| (key(x), x.clone())).collect() };
    let (mb, mo, mt) = (map(b), map(o), map(t));
    let order: Vec<String> = (o.iter().map(&key))
        .chain(t.iter().map(&key).filter(|k| !mo.contains_key(k)))
        .collect();
    let mut out = vec![];
    for k in order {
        let (bv, ov, tv) = (mb.get(&k), mo.get(&k), mt.get(&k));
        let pick = match (bv, ov, tv) {
            (Some(b), Some(o), Some(t)) if o != b && t != b => Some(fields(b, o, t)),
            _ if ov == bv => tv.cloned(),
            _ => ov.cloned(),
        };
        out.extend(pick);
    }
    out
}

fn one(b: &Screen, o: &Screen, t: &Screen) -> Screen {
    let mut s = fields(&frame(b), &frame(o), &frame(t));
    s.elements = list(&b.elements, &o.elements, &t.elements, |p: &Piece| {
        p.id.clone()
    });
    let nk = |n: &Nav| format!("{}>{}", n.from, n.to);
    s.navs = list(&b.navs, &o.navs, &t.navs, nk);
    s
}

/// Screens merged like the design: per screen, piece and nav, the user wins per field.
pub(crate) fn merge_screens(base: &Screens, ours: &Screens, theirs: &Screens) -> Screens {
    let ids: Vec<&String> = (ours.keys())
        .chain(theirs.keys().filter(|k| !ours.contains_key(*k)))
        .chain(base.keys())
        .collect();
    let mut out = Screens::new();
    for id in ids {
        if out.contains_key(id) {
            continue;
        }
        let (b, o, t) = (base.get(id), ours.get(id), theirs.get(id));
        let pick = match (b, o, t) {
            (Some(b), Some(o), Some(t)) if o != b && t != b => Some(one(b, o, t)),
            _ if o == b => t.cloned(),
            _ => o.cloned(),
        };
        if let Some(s) = pick {
            out.insert(id.clone(), s);
        }
    }
    // what lost its other end lapses: navs, arrow ends, the base of a variant
    let ids: HashSet<String> = out.keys().cloned().collect();
    for s in out.values_mut() {
        let pieces: HashSet<String> = s.elements.iter().map(|p| p.id.clone()).collect();
        s.navs
            .retain(|n| pieces.contains(&n.from) && ids.contains(&n.to));
        for p in &mut s.elements {
            for end in [&mut p.from, &mut p.to] {
                if end.as_ref().is_some_and(|e| !pieces.contains(e)) {
                    *end = None;
                }
            }
        }
        if s.variant_of.as_ref().is_some_and(|v| !ids.contains(v)) {
            s.variant_of = None;
        }
    }
    out
}

impl Merged {
    /// Adds the screens' merge: what theirs brought in or took out joins `changed`/`removed`.
    pub(crate) fn with_screens(mut self, base: &Board, ours: &Board, theirs: &Board) -> Merged {
        let sc = merge_screens(&base.screens, &ours.screens, &theirs.screens);
        let (changed, removed) = diff(&ours.screens, &sc);
        self.changed.extend(changed);
        self.removed.extend(removed);
        self.board.screens = sc;
        self
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::merge;
    use crate::snap::tests::board;

    pub fn screen(id: &str, x: i32, pieces: &[(&str, i32, i32)]) -> Screen {
        let mut s: Screen = serde_json::from_value(serde_json::json!({
            "version": 1, "id": id, "title": id, "x": x, "y": 0, "w": 400, "h": 300,
        }))
        .unwrap();
        for (p, px, py) in pieces {
            s.elements.push(
                serde_json::from_value(serde_json::json!({
                    "id": p, "kind": "button", "label": p, "x": px, "y": py, "w": 80, "h": 32,
                }))
                .unwrap(),
            );
        }
        s
    }

    pub fn with(screens: Vec<Screen>) -> Board {
        let mut b = board(vec![]);
        b.screens = screens.into_iter().map(|s| (s.id.clone(), s)).collect();
        b
    }

    #[test]
    fn keys_split_both_ways() {
        let n = Nav {
            from: "btn-go".into(),
            to: "home".into(),
            label: String::new(),
        };
        assert_eq!(
            split_key(&piece_key("login", "btn-go")),
            Some(("login", "btn-go"))
        );
        assert_eq!(
            split_nav(&nav_key("login", &n)),
            Some(("login", "btn-go", "home"))
        );
        assert_eq!(split_key(&nav_key("login", &n)), None);
    }

    #[test]
    fn user_and_agent_fields_of_one_piece_both_survive() {
        let base = with(vec![screen("login", 0, &[("btn-a", 8, 8)])]);
        let mut ours = base.clone();
        ours.screens.get_mut("login").unwrap().elements[0].x = 40;
        let mut theirs = base.clone();
        theirs.screens.get_mut("login").unwrap().elements[0].label = "Save".into();
        let m = merge(&base, &ours, &theirs);
        let p = &m.board.screens["login"].elements[0];
        assert_eq!((p.x, p.label.as_str()), (40, "Save"));
        assert_eq!(m.changed, ["login/btn-a"]);
    }

    #[test]
    fn a_removed_screen_takes_navs_to_it_and_the_user_keeps_an_edited_one() {
        let mut login = screen("login", 0, &[("btn-a", 8, 8)]);
        login.navs.push(Nav {
            from: "btn-a".into(),
            to: "home".into(),
            label: "go".into(),
        });
        let base = with(vec![login, screen("home", 500, &[])]);
        let mut theirs = base.clone();
        theirs.screens.remove("home");
        let m = merge(&base, &base, &theirs);
        assert!(!m.board.screens.contains_key("home"));
        assert!(m.board.screens["login"].navs.is_empty());
        assert!(m.removed.contains(&"home".to_string()));
        assert!(m.removed.contains(&"nav:login/btn-a>home".to_string()));
        // the user retitled it meanwhile: it stays
        let mut ours = base.clone();
        ours.screens.get_mut("home").unwrap().title = "Start".into();
        let m = merge(&base, &ours, &theirs);
        assert_eq!(m.board.screens["home"].title, "Start");
    }
}
