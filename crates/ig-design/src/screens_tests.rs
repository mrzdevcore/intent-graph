use super::*;
use crate::{Board, Bridge, DesignError, Op, apply};
use serde_json::json;
use std::collections::BTreeMap;

const S: &str = r#"{"version":1,"id":"login","title":"Login","x":0,"y":0,"w":1280,"h":800,
 "elements":[{"id":"btn-go","kind":"button","label":"Go","props":{"primary":true,"text":"Go"},"x":1,"y":2,"w":120,"h":40},
 {"id":"arrow-1","kind":"arrow","label":"","x":0,"y":0,"w":10,"h":10,"from":"btn-go"}],
 "navs":[{"from":"btn-go","to":"home","label":"ok"}]}"#;

fn piece(id: &str, kind: PieceKind) -> Piece {
    Piece {
        id: id.into(),
        kind,
        label: String::new(),
        props: Props::default(),
        x: 0,
        y: 0,
        w: 1,
        h: 1,
        points: None,
        from: None,
        to: None,
    }
}

fn board() -> Board {
    let mut d = crate::parse(
        r#"{"version":1,"title":"T","scope":{"kind":"layer","name":"api"},
        "elements":[{"id":"fn-a","kind":"function","name":"a"}]}"#,
    )
    .unwrap();
    d.bridges = vec![
        Bridge {
            from: "login/btn-go".into(),
            to: "fn-a".into(),
        },
        Bridge {
            from: "home/btn-x".into(),
            to: "fn-a".into(),
        },
    ];
    let mut screens = BTreeMap::new();
    for s in [parse_screen(S).unwrap(), {
        let mut h = parse_screen(S).unwrap();
        h.id = "home".into();
        h.navs.clear();
        h
    }] {
        screens.insert(s.id.clone(), s);
    }
    Board { design: d, screens }
}

fn ops(v: serde_json::Value) -> Vec<Op> {
    serde_json::from_value(v).unwrap()
}

#[test]
fn screen_round_trip_is_canonical() {
    let s = parse_screen(S).unwrap();
    let t = screen_to_text(&s);
    assert_eq!(screen_to_text(&parse_screen(&t).unwrap()), t);
    assert!(t.ends_with("}\n") && t.contains("\"primary\": true") && !t.contains("variantOf"));
    assert!(parse_screen(&t.replace("\"x\": 0,", "\"y2\": 0,")).is_err());
}

#[test]
fn unknown_prop_for_kind_is_refused() {
    let mut s = parse_screen(S).unwrap();
    let mut t = piece("table-x", PieceKind::Table);
    t.props.text = Some("a".into());
    s.elements.push(t);
    let e = validate_screen(&s).unwrap_err();
    assert!(matches!(&e, DesignError::Invalid(m) if m == "table-x: unknown prop text"));
    // a prop no kind has, through an op
    let o = ops(
        json!([{"op":"screen_add","screen":"login","kind":"table","label":"t","props":{"nope":1}}]),
    );
    let e = apply(&board(), &o).unwrap_err();
    assert!(matches!(&e, DesignError::Invalid(m) if m == "table-t: unknown prop nope"));
}

#[test]
fn variant_needs_state_and_existing_parent() {
    let mut s = parse_screen(S).unwrap();
    s.variant_of = Some("home".into());
    assert!(validate_screen(&s).is_err());
    s.state = Some("empty".into());
    assert!(validate_screen(&s).is_ok());
    let o =
        ops(json!([{"op":"add_screen","title":"Login empty","variant_of":"nope","state":"empty"}]));
    assert!(apply(&board(), &o).is_err());
    let o = ops(json!([{"op":"add_screen","title":"Login empty","variant_of":"login"}]));
    assert!(apply(&board(), &o).is_err());
    let o = ops(
        json!([{"op":"add_screen","title":"Login empty","variant_of":"login","state":"empty"}]),
    );
    let (b, t) = apply(&board(), &o).unwrap();
    assert_eq!(t.screens, ["screen-login-empty"]);
    // the base cannot go while a variant names it
    let o = ops(json!([{"op":"remove_screen","id":"login"}]));
    assert!(apply(&b, &o).is_err());
}

#[test]
fn pen_point_cap_pressure_and_ends() {
    let mut s = parse_screen(S).unwrap();
    let mut p = piece("pen-1", PieceKind::Pen);
    p.points = Some(vec![[0, 0, 100]; MAX_POINTS]);
    s.elements.push(p);
    assert!(validate_screen(&s).is_ok());
    s.elements[2].points.as_mut().unwrap().push([0, 0, 0]);
    assert!(validate_screen(&s).is_err());
    s.elements[2].points = Some(vec![[0, 0, 101]]);
    assert!(validate_screen(&s).is_err());
    s.elements[2].points = None;
    s.elements[1].to = Some("gone".into());
    assert!(validate_screen(&s).is_err());
}

#[test]
fn new_piece_id_vectors() {
    let s = parse_screen(S).unwrap();
    let id = |k, l| new_piece_id(&s, k, l);
    assert_eq!(id(PieceKind::Button, "Export CSV"), "btn-export-csv");
    assert_eq!(id(PieceKind::Table, "Patients"), "table-patients");
    assert_eq!(id(PieceKind::Button, "Go"), "btn-go-2");
    assert_eq!(id(PieceKind::Arrow, ""), "arrow-2");
    let mut s = s;
    s.elements.push(piece("pen-1", PieceKind::Pen));
    s.elements.push(piece("pen-2", PieceKind::Pen));
    assert_eq!(new_piece_id(&s, PieceKind::Pen, "x"), "pen-3");
}

#[test]
fn screen_ops_apply_all_or_nothing() {
    let before = board();
    let o = ops(json!([
        {"op":"add_screen","id":"cart","title":"Cart"},
        {"op":"screen_add","screen":"cart","kind":"button","label":"Pay"},
        {"op":"screen_move","screen":"cart","id":"btn-pay","x":5,"y":6},
        {"op":"screen_move","screen":"cart","id":"nope","x":5,"y":6}
    ]));
    assert!(apply(&before, &o).is_err());
    assert_eq!(before, board());
    let (b, t) = apply(&before, &o[..3]).unwrap();
    assert_eq!(t.screens, ["cart"]);
    let p = &b.screens["cart"].elements[0];
    assert_eq!((p.id.as_str(), p.x, p.y, p.w), ("btn-pay", 5, 6, 120));
    assert!(b.screens["cart"].x > 1280);
    // update merges props; a nav goes to an existing screen only
    let o = ops(json!([
        {"op":"screen_update","screen":"login","id":"btn-go","set":{"props":{"text":"Send","primary":null}}},
        {"op":"nav","screen":"login","from":"btn-go","to":"home","label":"again"}
    ]));
    let (b, _) = apply(&before, &o).unwrap();
    let p = &b.screens["login"].elements[0];
    assert_eq!(
        (p.props.text.as_deref(), p.props.primary),
        (Some("Send"), None)
    );
    assert_eq!(b.screens["login"].navs[0].label, "again");
    let o = ops(json!([{"op":"nav","screen":"login","from":"btn-go","to":"zzz","label":""}]));
    assert!(apply(&before, &o).is_err());
}

#[test]
fn remove_screen_drops_its_bridges() {
    let (b, t) = apply(&board(), &ops(json!([{"op":"remove_screen","id":"home"}]))).unwrap();
    assert_eq!(b.design.bridges.len(), 1);
    assert_eq!(t.bridges, ["home/btn-x>fn-a"]);
    // the nav that led there goes with it, in the same step
    assert!(b.screens["login"].navs.is_empty());
    assert_eq!(t.screens, ["login", "home"]);
    // removing a piece takes its bridge, navs and arrow ends
    let o = ops(json!([{"op":"screen_remove","screen":"login","id":"btn-go"}]));
    let (b, t) = apply(&board(), &o).unwrap();
    assert_eq!(t.bridges, ["login/btn-go>fn-a"]);
    assert!(b.screens["login"].navs.is_empty() && b.screens["login"].elements[0].from.is_none());
}

#[test]
fn caps_hit_only_what_the_edit_changed() {
    let long = "x".repeat(crate::MAX_NAME + 1);
    let o = ops(json!([{"op":"screen_add","screen":"login","kind":"text","label":long}]));
    assert!(apply(&board(), &o).is_err());
    let mut b = board();
    b.screens.get_mut("login").unwrap().elements[0].label = long;
    let o = ops(json!([{"op":"screen_move","screen":"home","id":"btn-go","x":1,"y":1}]));
    assert!(apply(&b, &o).is_ok());
    let o = ops(json!([{"op":"screen_move","screen":"login","id":"arrow-1","x":1,"y":1}]));
    assert!(apply(&b, &o).is_ok());
}
