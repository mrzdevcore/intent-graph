//! Validated, all-or-nothing edits of a design: the one code path for the board engine and
//! `design_edit`.

use crate::screen_ops::{self, PiecePatch, ScreenPatch};
use crate::{
    Bridge, Design, DesignError, Element, ElementKind, Link, LinkKind, Mark, MarkKind, PieceKind,
    Scope, Screen, check_refs, new_id, validate, validate_screen,
};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq)]
pub struct Board {
    pub design: Design,
    /// By id: plans/<slug>/screens/<id>.json.
    pub screens: BTreeMap<String, Screen>,
}

/// Fields `update` may change; never the id. An empty string clears an optional text field.
#[derive(Deserialize, Serialize, Clone, Debug, Default)]
#[serde(deny_unknown_fields)]
pub struct ElementPatch {
    pub name: Option<String>,
    pub file: Option<String>,
    pub method: Option<String>,
    pub path: Option<String>,
    pub icon: Option<String>,
    pub layer: Option<String>,
    pub on: Option<String>,
    pub frame: Option<String>,
    pub question: Option<bool>,
    pub answer: Option<String>,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum Op {
    Add {
        id: Option<String>,
        kind: ElementKind,
        name: String,
        file: Option<String>,
        method: Option<String>,
        path: Option<String>,
        layer: Option<String>,
        on: Option<String>,
        x: Option<i32>,
        y: Option<i32>,
        w: Option<i32>,
        h: Option<i32>,
    },
    Update {
        id: String,
        set: ElementPatch,
    },
    Move {
        id: String,
        x: i32,
        y: i32,
    },
    Resize {
        id: String,
        w: i32,
        h: i32,
    },
    /// Cascades: links, bridges, marks, note `on`, frame membership.
    Remove {
        id: String,
    },
    /// Fresh ids via `new_id`; ends that name a pasted element are rewritten.
    Paste {
        elements: Vec<Element>,
        links: Vec<Link>,
        dx: i32,
        dy: i32,
    },
    Link {
        from: String,
        to: String,
        kind: LinkKind,
    },
    Unlink {
        key: String,
    },
    Mark {
        node: String,
        mark: Option<MarkKind>,
    },
    Scope {
        scope: Scope,
    },
    Bridge {
        from: String,
        to: String,
    },
    Unbridge {
        key: String,
    },
    AddScreen {
        id: Option<String>,
        title: String,
        variant_of: Option<String>,
        state: Option<String>,
    },
    /// Cascades: bridges from the screen, navs to it. Refused while a variant names it.
    RemoveScreen {
        id: String,
    },
    ScreenAdd {
        screen: String,
        id: Option<String>,
        kind: PieceKind,
        label: String,
        props: Option<Map<String, Value>>,
        x: Option<i32>,
        y: Option<i32>,
    },
    ScreenUpdate {
        screen: String,
        id: String,
        set: PiecePatch,
    },
    ScreenMove {
        screen: String,
        id: String,
        x: i32,
        y: i32,
    },
    /// Cascades: navs from it, arrow ends, bridges from it.
    ScreenRemove {
        screen: String,
        id: String,
    },
    Nav {
        screen: String,
        from: String,
        to: String,
        label: String,
    },
    Unnav {
        screen: String,
        from: String,
        to: String,
    },
    /// A screen's title, place, size or background (`null` takes the background away).
    ScreenSet {
        id: String,
        set: ScreenPatch,
    },
}

impl Op {
    pub fn is_screen(&self) -> bool {
        use Op::*;
        matches!(
            self,
            AddScreen { .. }
                | RemoveScreen { .. }
                | ScreenAdd { .. }
                | ScreenUpdate { .. }
                | ScreenMove { .. }
                | ScreenRemove { .. }
                | Nav { .. }
                | Unnav { .. }
                | ScreenSet { .. }
        )
    }
}

#[derive(Debug, Default, PartialEq)]
pub struct Touched {
    pub elements: Vec<String>,
    pub links: Vec<String>,
    pub bridges: Vec<String>,
    /// Screen ids; a removed screen is touched too.
    pub screens: Vec<String>,
}

fn push(v: &mut Vec<String>, s: String) {
    if !v.contains(&s) {
        v.push(s);
    }
}

fn set(slot: &mut Option<String>, v: &Option<String>) {
    if let Some(v) = v {
        *slot = (!v.is_empty()).then(|| v.clone());
    }
}

fn find<'a>(d: &'a mut Design, id: &str) -> Result<&'a mut Element, DesignError> {
    d.elements
        .iter_mut()
        .find(|e| e.id == id)
        .ok_or_else(|| DesignError::Invalid(format!("no element {id}")))
}

fn step(d: &mut Design, op: &Op, t: &mut Touched) -> Result<(), DesignError> {
    match op {
        Op::Add {
            id,
            kind,
            name,
            file,
            method,
            path,
            layer,
            on,
            x,
            y,
            w,
            h,
        } => {
            if name.trim().is_empty() {
                return Err(DesignError::Invalid("empty name".into()));
            }
            let id = id.clone().unwrap_or_else(|| new_id(d, *kind, name));
            let mut e = blank(&id, *kind, name);
            (e.file, e.method, e.path, e.layer, e.on) = (
                file.clone(),
                method.clone(),
                path.clone(),
                layer.clone(),
                on.clone(),
            );
            (e.x, e.y, e.w, e.h) = (*x, *y, *w, *h);
            push(&mut t.elements, id);
            d.elements.push(e);
        }
        Op::Update { id, set: p } => {
            let e = find(d, id)?;
            if let Some(n) = &p.name {
                if n.trim().is_empty() {
                    return Err(DesignError::Invalid(format!("{id}: empty name")));
                }
                e.name = n.clone();
            }
            set(&mut e.file, &p.file);
            set(&mut e.method, &p.method);
            set(&mut e.path, &p.path);
            set(&mut e.icon, &p.icon);
            set(&mut e.layer, &p.layer);
            set(&mut e.on, &p.on);
            set(&mut e.frame, &p.frame);
            set(&mut e.answer, &p.answer);
            if let Some(q) = p.question {
                e.question = q;
            }
            push(&mut t.elements, id.clone());
        }
        Op::Move { id, x, y } => {
            let e = find(d, id)?;
            (e.x, e.y) = (Some(*x), Some(*y));
            push(&mut t.elements, id.clone());
        }
        Op::Resize { id, w, h } => {
            let e = find(d, id)?;
            (e.w, e.h) = (Some(*w), Some(*h));
            push(&mut t.elements, id.clone());
        }
        Op::Remove { id } => {
            find(d, id)?;
            d.elements.retain(|e| e.id != *id);
            for e in &mut d.elements {
                let (a, b) = (e.on.as_deref() == Some(id), e.frame.as_deref() == Some(id));
                if a {
                    e.on = None;
                }
                if b {
                    e.frame = None;
                }
                if a || b {
                    push(&mut t.elements, e.id.clone());
                }
            }
            d.links.retain(|l| {
                let hit = l.from == *id || l.to == *id;
                if hit {
                    push(&mut t.links, l.key());
                }
                !hit
            });
            d.bridges.retain(|b| {
                let hit = b.to == *id;
                if hit {
                    push(&mut t.bridges, b.key());
                }
                !hit
            });
            d.marks.retain(|m| m.node != *id);
            t.elements.retain(|e| e != id);
            push(&mut t.elements, id.clone()); // the removed id is touched too
        }
        Op::Paste {
            elements,
            links,
            dx,
            dy,
        } => {
            let mut map = std::collections::HashMap::new();
            for e in elements {
                let id = new_id(d, e.kind, &e.name);
                let mut n = e.clone();
                n.id = id.clone();
                n.x = e.x.map(|x| x + dx);
                n.y = e.y.map(|y| y + dy);
                map.insert(e.id.clone(), id.clone());
                push(&mut t.elements, id);
                d.elements.push(n);
            }
            let re =
                |s: &Option<String>| s.as_ref().and_then(|s| map.get(s).cloned()).or(s.clone());
            for id in map.values() {
                let e = find(d, id)?;
                (e.on, e.frame) = (re(&e.on), re(&e.frame));
            }
            for l in links {
                let l = Link {
                    from: map.get(&l.from).unwrap_or(&l.from).clone(),
                    to: map.get(&l.to).unwrap_or(&l.to).clone(),
                    kind: l.kind,
                };
                if !d.links.contains(&l) {
                    push(&mut t.links, l.key());
                    d.links.push(l);
                }
            }
        }
        Op::Link { from, to, kind } => {
            let l = Link {
                from: from.clone(),
                to: to.clone(),
                kind: *kind,
            };
            if !d.links.contains(&l) {
                push(&mut t.links, l.key());
                d.links.push(l);
            }
        }
        Op::Unlink { key } => {
            let n = d.links.len();
            d.links.retain(|l| l.key() != *key);
            if d.links.len() == n {
                return Err(DesignError::Invalid(format!("no link {key}")));
            }
            push(&mut t.links, key.clone());
        }
        Op::Mark { node, mark } => {
            d.marks.retain(|m| m.node != *node);
            if let Some(mark) = mark {
                d.marks.push(Mark {
                    node: node.clone(),
                    mark: *mark,
                });
            }
            push(&mut t.elements, node.clone());
        }
        Op::Scope { scope } => d.scope = scope.clone(),
        Op::Bridge { from, to } => {
            let b = Bridge {
                from: from.clone(),
                to: to.clone(),
            };
            if !d.bridges.contains(&b) {
                push(&mut t.bridges, b.key());
                d.bridges.push(b);
            }
        }
        Op::Unbridge { key } => {
            let n = d.bridges.len();
            d.bridges.retain(|b| b.key() != *key);
            if d.bridges.len() == n {
                return Err(DesignError::Invalid(format!("no bridge {key}")));
            }
            push(&mut t.bridges, key.clone());
        }
        _ => {} // screen ops: `screen_ops::step`
    }
    Ok(())
}

fn blank(id: &str, kind: ElementKind, name: &str) -> Element {
    Element {
        id: id.into(),
        kind,
        name: name.into(),
        file: None,
        method: None,
        path: None,
        icon: None,
        layer: None,
        on: None,
        question: false,
        answer: None,
        frame: None,
        x: None,
        y: None,
        w: None,
        h: None,
    }
}

/// All or nothing: on any error the input board is untouched, and the result passes `validate`.
pub fn apply(b: &Board, ops: &[Op]) -> Result<(Board, Touched), DesignError> {
    let mut d = b.design.clone();
    let mut screens = b.screens.clone();
    let mut t = Touched::default();
    for op in ops {
        if !screen_ops::step(&mut d, &mut screens, op, &mut t)? {
            step(&mut d, op, &mut t)?;
        }
    }
    crate::caps::caps(&b.design, &d)?;
    crate::screens::caps(&b.screens, &screens)?;
    validate(&d)?;
    new_bridges(&b.design, &d, &screens)?;
    // Only what the edit touched is checked: an old broken screen never blocks another edit.
    for s in t.screens.iter().filter_map(|id| screens.get(id)) {
        validate_screen(s)?;
        check_refs(s, &screens)?;
    }
    Ok((Board { design: d, screens }, t))
}

/// A bridge this edit adds starts at a piece of a screen and ends at an element that stands for
/// code (not a note or frame) or at a graph id (never a link key).
fn new_bridges(
    old: &Design,
    d: &Design,
    screens: &std::collections::BTreeMap<String, crate::Screen>,
) -> Result<(), DesignError> {
    for b in d.bridges.iter().filter(|b| !old.bridges.contains(b)) {
        let bad = |why: &str| Err(DesignError::Invalid(format!("bridge {}: {why}", b.key())));
        let piece = (b.from.split_once('/'))
            .and_then(|(s, p)| screens.get(s)?.elements.iter().find(|e| e.id == p));
        if piece.is_none() {
            return bad("from must be a piece of a screen");
        }
        if b.to.contains('|') {
            return bad("to must be an element or graph id, not a link");
        }
        let kind = d.elements.iter().find(|e| e.id == b.to).map(|e| e.kind);
        if matches!(kind, Some(ElementKind::Note | ElementKind::Frame)) {
            return bad("to must not be a note or frame");
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "ops_tests.rs"]
mod tests;
