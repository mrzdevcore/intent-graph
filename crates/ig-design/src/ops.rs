//! Validated, all-or-nothing edits of a design: the one code path for the board engine and
//! `design_edit`.

use crate::{
    Bridge, Design, DesignError, Element, ElementKind, Link, LinkKind, Mark, MarkKind, Scope,
    new_id, validate,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq)]
pub struct Board {
    pub design: Design, // `screens` joins in Task 13
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
}

#[derive(Debug, Default, PartialEq)]
pub struct Touched {
    pub elements: Vec<String>,
    pub links: Vec<String>,
    pub bridges: Vec<String>,
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
    let mut t = Touched::default();
    for op in ops {
        step(&mut d, op, &mut t)?;
    }
    crate::caps::caps(&b.design, &d)?;
    validate(&d)?;
    Ok((Board { design: d }, t))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::caps::*;
    use crate::empty;

    fn board() -> Board {
        let mut d = empty("T", Scope::Layer { name: "api".into() });
        d.elements = vec![
            blank("fn-a", ElementKind::Function, "a"),
            blank("fn-b", ElementKind::Function, "b"),
            blank("frame-1", ElementKind::Frame, "F"),
            blank("note-1", ElementKind::Note, "n"),
        ];
        d.elements[0].frame = Some("frame-1".into());
        d.elements[3].on = Some("fn-a".into());
        d.links = vec![Link {
            from: "fn-a".into(),
            to: "fn-b".into(),
            kind: LinkKind::Calls,
        }];
        d.bridges = vec![Bridge {
            from: "login/btn".into(),
            to: "fn-a".into(),
        }];
        Board { design: d }
    }

    fn ops(j: &str) -> Vec<Op> {
        serde_json::from_str(j).unwrap()
    }

    #[test]
    fn long_strings_are_refused_on_what_the_edit_touches() {
        let long = |n| "x".repeat(n);
        let upd = |set: serde_json::Value| {
            let o = vec![Op::Update {
                id: "fn-a".into(),
                set: serde_json::from_value(set).unwrap(),
            }];
            apply(&board(), &o).map(|_| ()).map_err(|e| e.to_string())
        };
        assert!(upd(serde_json::json!({"name": long(MAX_NAME)})).is_ok());
        let e = upd(serde_json::json!({"name": long(MAX_NAME + 1)})).unwrap_err();
        assert_eq!(e, "design.json: fn-a: the name is longer than 200 bytes");
        assert!(upd(serde_json::json!({"file": long(MAX_FILE + 1)})).is_err());
        let note = |n| {
            vec![Op::Update {
                id: "note-1".into(),
                set: ElementPatch {
                    name: Some(long(n)),
                    ..Default::default()
                },
            }]
        };
        assert!(apply(&board(), &note(MAX_TEXT)).is_ok());
        assert!(apply(&board(), &note(MAX_TEXT + 1)).is_err());
        let link = vec![Op::Link {
            from: "fn-a".into(),
            to: long(MAX_ID + 1),
            kind: LinkKind::Calls,
        }];
        assert!(apply(&board(), &link).is_err());
        // an old over-long name elsewhere does not block an edit of another element
        let mut b = board();
        b.design.elements[1].name = long(MAX_NAME + 1);
        assert!(
            apply(
                &b,
                &[Op::Move {
                    id: "fn-a".into(),
                    x: 1,
                    y: 2
                }]
            )
            .is_ok()
        );
        // a move of the over-long element itself is fine too: its name did not change
        assert!(
            apply(
                &b,
                &[Op::Move {
                    id: "fn-b".into(),
                    x: 1,
                    y: 2
                }]
            )
            .is_ok()
        );
        let scope = |id: String| {
            vec![Op::Scope {
                scope: Scope::Dir { id },
            }]
        };
        assert!(apply(&b, &scope(long(MAX_ID))).is_ok());
        let e = apply(&b, &scope(long(MAX_ID + 1))).unwrap_err().to_string();
        assert_eq!(e, "design.json: the scope is longer than 512 bytes");
    }

    #[test]
    fn rename_keeps_the_id() {
        let o = ops(r#"[{"op":"update","id":"fn-a","set":{"name":"renamed"}}]"#);
        let (b, t) = apply(&board(), &o).unwrap();
        assert_eq!(b.design.elements[0].id, "fn-a");
        assert_eq!(b.design.elements[0].name, "renamed");
        assert_eq!(t.elements, ["fn-a"]);
    }

    #[test]
    fn remove_cascades_links_note_targets_frames_and_bridges() {
        let o = ops(r#"[{"op":"remove","id":"fn-a"}]"#);
        let (b, t) = apply(&board(), &o).unwrap();
        assert!(b.design.links.is_empty() && b.design.bridges.is_empty());
        assert_eq!(
            b.design
                .elements
                .iter()
                .find(|e| e.id == "note-1")
                .unwrap()
                .on,
            None
        );
        assert_eq!(t.links.len() + t.bridges.len(), 2);
        let (b, _) = apply(&board(), &ops(r#"[{"op":"remove","id":"frame-1"}]"#)).unwrap();
        assert_eq!(b.design.elements[0].frame, None);
    }

    #[test]
    fn paste_gives_fresh_ids_and_rewrites_internal_link_ends() {
        let src = board().design;
        let o = vec![Op::Paste {
            elements: src.elements[..2].to_vec(),
            links: src.links.clone(),
            dx: 5,
            dy: 5,
        }];
        let (b, t) = apply(&board(), &o).unwrap();
        assert_eq!(t.elements, ["fn-a-2", "fn-b-2"]);
        assert!(
            b.design
                .links
                .iter()
                .any(|l| l.from == "fn-a-2" && l.to == "fn-b-2")
        );
    }

    #[test]
    fn ops_apply_all_or_nothing() {
        let before = board();
        let o = ops(
            r#"[{"op":"move","id":"fn-a","x":1,"y":2},{"op":"resize","id":"fn-b","w":3,"h":4},
               {"op":"mark","node":"fn-a","mark":"changed"},{"op":"remove","id":"fn-missing"}]"#,
        );
        assert!(apply(&before, &o).is_err());
        assert_eq!(before, board());
    }

    #[test]
    fn add_without_id_uses_new_id() {
        let o = ops(
            r#"[{"op":"add","id":null,"kind":"function","name":"a","file":null,"method":null,"path":null,"layer":null,"on":null,"x":1,"y":2,"w":null,"h":null}]"#,
        );
        let (b, t) = apply(&board(), &o).unwrap();
        assert_eq!(t.elements, ["fn-a-2"]);
        assert_eq!(b.design.elements.last().unwrap().x, Some(1));
    }

    #[test]
    fn link_to_unknown_element_is_refused() {
        let o = ops(r#"[{"op":"link","from":"fn-a","to":"fn-zzz","kind":"calls"}]"#);
        assert!(matches!(apply(&board(), &o), Err(DesignError::Invalid(_))));
    }

    #[test]
    fn remove_keeps_bridges_whose_from_tail_matches() {
        let mut b = board();
        b.design.bridges.push(Bridge {
            from: "login/fn-b".into(),
            to: "fn-a".into(),
        });
        let (b, _) = apply(&b, &ops(r#"[{"op":"remove","id":"fn-b"}]"#)).unwrap();
        assert_eq!(b.design.bridges.len(), 2);
    }

    #[test]
    fn empty_names_and_dangling_on_frame_are_refused() {
        for j in [
            r#"[{"op":"update","id":"fn-a","set":{"name":" "}}]"#,
            r#"[{"op":"update","id":"fn-a","set":{"frame":"frame-9"}}]"#,
            r#"[{"op":"update","id":"note-1","set":{"on":"fn-zzz"}}]"#,
            r#"[{"op":"add","id":null,"kind":"function","name":"","file":null,"method":null,"path":null,"layer":null,"on":null,"x":null,"y":null,"w":null,"h":null}]"#,
        ] {
            assert!(apply(&board(), &ops(j)).is_err(), "{j}");
        }
    }

    #[test]
    fn update_cannot_change_the_id() {
        let e =
            serde_json::from_str::<Vec<Op>>(r#"[{"op":"update","id":"fn-a","set":{"id":"x"}}]"#)
                .unwrap_err();
        assert!(matches!(DesignError::from(e), DesignError::Json(_)));
    }

    #[test]
    fn new_id_vectors_are_unchanged() {
        let d = &board().design;
        assert_eq!(new_id(d, ElementKind::Function, "a"), "fn-a-2");
        assert_eq!(new_id(d, ElementKind::Note, "x"), "note-2");
        assert_eq!(new_id(d, ElementKind::Route, "GET /x!"), "route-get-x");
        assert_eq!(new_id(d, ElementKind::System, "!!"), "sys-1");
    }
}
