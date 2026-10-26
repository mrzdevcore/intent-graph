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
    Board {
        design: d,
        screens: Default::default(),
    }
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
    let e = serde_json::from_str::<Vec<Op>>(r#"[{"op":"update","id":"fn-a","set":{"id":"x"}}]"#)
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
