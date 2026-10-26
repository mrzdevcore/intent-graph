use super::*;
use crate::snap::tests::{board, el, lanes};
use crate::{CtxBox, Paint};
use ig_design::{ElementKind, ElementPatch};

pub(crate) fn ctx() -> Context {
    let b = |id: &str, kind: &str, y, layer: Option<&str>| CtxBox {
        id: id.into(),
        label: id.into(),
        kind: kind.into(),
        layer: layer.map(String::from),
        x: 400.0,
        y,
        w: 100.0,
        h: 40.0,
    };
    Context {
        lanes: lanes(),
        boxes: vec![
            b("server/x.ts#GET /x", "route", 10.0, Some("api")),
            b("system:pg", "system", 240.0, None),
        ],
        edges: vec![],
    }
}

/// fn-a at (8, 8, 80, 40) in the api layer, fn-b at (208, 8), a calls link between them.
pub(crate) fn engine() -> Engine {
    let mut a = el("fn-a", ElementKind::Function);
    a.layer = Some("api".into());
    let mut b = el("fn-b", ElementKind::Function);
    b.x = Some(208);
    let mut bd = board(vec![a, b]);
    bd.design.links.push(Link {
        from: "fn-a".into(),
        to: "fn-b".into(),
        kind: ig_design::LinkKind::Calls,
    });
    Engine::new(ctx(), bd)
}

fn renamed(b: &Board, id: &str, name: &str) -> Board {
    let set = ElementPatch {
        name: Some(name.into()),
        ..Default::default()
    };
    let op = Op::Update { id: id.into(), set };
    apply(b, &[op]).unwrap().0
}

#[test]
fn hit_picks_topmost() {
    let mut e = engine();
    let mut f = el("frame-1", ElementKind::Frame);
    (f.x, f.y, f.w, f.h) = (Some(0), Some(0), Some(600), Some(200));
    let mut c = el("fn-c", ElementKind::Function);
    (c.x, c.y) = (Some(40), Some(16));
    let mut b = e.board().clone();
    b.design.elements.insert(0, f);
    b.design.elements.push(c);
    let e2 = Engine::new(ctx(), b);
    assert_eq!(e2.hit(20.0, 20.0).as_deref(), Some("fn-a"));
    assert_eq!(
        e2.hit(60.0, 30.0).as_deref(),
        Some("fn-c"),
        "drawn later, on top"
    );
    assert_eq!(e2.hit(148.0, 28.0).as_deref(), Some("fn-a|calls|fn-b"));
    assert_eq!(e2.hit(150.0, 150.0).as_deref(), Some("frame-1"));
    assert_eq!(e.hit(150.0, 150.0), None);
    e.camera.x = 400.0; // screen → world
    assert_eq!(e.hit(10.0, 30.0).as_deref(), Some("server/x.ts#GET /x"));
}

#[test]
fn hit_ignores_hidden_layers() {
    let mut e = engine();
    e.set_hidden(vec!["api".into()]);
    assert_eq!(e.hit(20.0, 20.0), None);
    assert_eq!(e.hit(450.0, 30.0), None);
    assert_eq!(
        e.hit(148.0, 28.0),
        None,
        "a link to a hidden element is hidden"
    );
    assert_eq!(e.hit(220.0, 20.0).as_deref(), Some("fn-b"));
    assert!(e.labels().iter().all(|l| l.id != "fn-a"));
}

#[test]
fn external_agent_merge_highlights_changed() {
    let mut e = engine();
    let disk = renamed(e.board(), "fn-b", "B");
    let o = e.external(disk, Some("t1"));
    assert!(o.changed && o.meaning);
    assert_eq!(e.highlighted(), ["fn-b"]);
    let s = e.scene([0.0, 0.0, 800.0, 600.0]);
    assert!(s.rects.iter().any(|r| r.paint == Paint::Highlight));
    e.dismiss(Some("fn-b"));
    assert!(e.highlighted().is_empty());
}

#[test]
fn user_touch_clears_highlight() {
    let mut e = engine();
    let disk = renamed(&renamed(e.board(), "fn-b", "B"), "fn-a", "A");
    e.external(disk, Some("t1"));
    let mv = [Op::Move {
        id: "fn-b".into(),
        x: 304,
        y: 8,
    }];
    e.ops(&mv, Step::User);
    assert_eq!(e.highlighted(), ["fn-a"]);
}

#[test]
fn undo_of_agent_step_clears_highlights_and_redo_restores() {
    let mut e = engine();
    let disk = renamed(e.board(), "fn-b", "B");
    e.external(disk, Some("t1"));
    let o = e.undo();
    assert!(o.changed && e.highlighted().is_empty());
    assert_eq!(e.board().design.elements[1].name, "b");
    e.redo();
    assert_eq!(e.highlighted(), ["fn-b"]);
    assert_eq!(e.board().design.elements[1].name, "B");
}

#[test]
fn agent_changes_of_one_turn_undo_together_and_unplaced_ones_get_placed() {
    let mut e = engine();
    let d1 = renamed(e.board(), "fn-b", "B");
    e.external(d1.clone(), Some("t1"));
    let mut d2 = d1;
    let mut n = el("fn-n", ElementKind::Function);
    (n.x, n.y, n.layer) = (None, None, Some("api".into()));
    d2.design.elements.push(n);
    e.external(d2, Some("t1"));
    let n = e.board().design.elements.iter().find(|x| x.id == "fn-n");
    assert_eq!(n.and_then(|n| n.y), Some(64));
    assert_eq!(e.highlighted(), ["fn-b", "fn-n"]);
    e.undo();
    assert_eq!(e.board(), engine().board());
    assert!(!e.undo().changed);
}

#[test]
fn invalid_ops_report_and_change_nothing() {
    let mut e = engine();
    let o = e.ops(&[Op::Remove { id: "fn-zz".into() }], Step::User);
    assert_eq!(o.error.as_deref(), Some("design.json: no element fn-zz"));
    assert!(!o.changed);
    assert_eq!(e.board(), engine().board());
}

#[test]
fn checks_reach_scene_labels_and_a11y() {
    let mut e = engine();
    e.set_check(vec![("fn-a".into(), Severity::Error, "forbidden".into())]);
    let s = e.scene([0.0, 0.0, 800.0, 600.0]);
    assert!(
        s.rects
            .iter()
            .any(|r| r.paint == Paint::Problem(Severity::Error))
    );
    let l = e.labels().into_iter().find(|l| l.id == "fn-a").unwrap();
    assert_eq!(l.title.as_deref(), Some("forbidden"));
    let a = e.a11y();
    assert_eq!(a[0].problem.as_deref(), Some("forbidden"));
    assert_eq!(a[2].text, "fn-a calls fn-b");
    assert!(!a[0].question && a[0].answer.is_none());
}

#[test]
fn lanes_get_their_names_at_the_left_edge() {
    let e = engine();
    let l = e.labels();
    let api = l.iter().find(|l| l.id == "lane:0").expect("a lane label");
    assert_eq!((api.text.as_str(), api.r[0], api.r[1]), ("api", -32.0, 0.0));
    assert!(l.iter().any(|l| l.text == "External services"));
}

#[test]
fn a11y_tells_open_questions_from_answered_ones() {
    let mut a = el("note-1", ElementKind::Note);
    a.question = true;
    a.answer = Some("  ".into());
    let mut b = el("note-2", ElementKind::Note);
    b.question = true;
    b.answer = Some("yes".into());
    let a11y = Engine::new(ctx(), board(vec![a, b])).a11y();
    assert_eq!((a11y[0].question, a11y[0].answer.as_deref()), (true, None));
    assert_eq!(a11y[1].answer.as_deref(), Some("yes"));
}

#[test]
fn undo_of_an_agent_step_clears_only_its_highlights() {
    let mut e = engine();
    let d1 = renamed(e.board(), "fn-b", "B");
    e.external(d1.clone(), Some("t1"));
    e.external(renamed(&d1, "fn-a", "A"), Some("t2"));
    assert_eq!(e.highlighted(), ["fn-b", "fn-a"]);
    e.undo();
    assert_eq!(e.highlighted(), ["fn-b"]);
}

#[test]
fn external_merge_highlights_clear_on_undo_and_return_on_redo() {
    let mut e = engine();
    e.external(renamed(e.board(), "fn-b", "B"), None);
    assert_eq!(e.highlighted(), ["fn-b"]);
    e.undo();
    assert!(e.highlighted().is_empty());
    e.redo();
    assert_eq!(e.highlighted(), ["fn-b"]);
}

#[test]
fn hit_with_zoom_and_pan() {
    let mut e = engine();
    e.camera = Camera {
        x: -10.0,
        y: -10.0,
        zoom: 2.0,
    };
    assert_eq!(e.hit(60.0, 60.0).as_deref(), Some("fn-a"));
    assert_eq!(e.hit(920.0, 80.0).as_deref(), Some("server/x.ts#GET /x"));
    assert_eq!(e.hit(20.0, 20.0), None, "world (0, 0) is empty");
}

#[test]
fn undo_and_redo_during_a_drag_drop_the_drag() {
    for interrupt in 0..2 {
        let mut e = engine();
        e.ops(
            &[Op::Move {
                id: "fn-b".into(),
                x: 216,
                y: 8,
            }],
            Step::User,
        );
        let b = |x, y| crate::Input::Down {
            x,
            y,
            button: 0,
            mods: 0,
        };
        e.input(b(20.0, 20.0));
        e.input(crate::Input::Move {
            x: 60.0,
            y: 20.0,
            mods: 0,
        });
        match interrupt {
            0 => _ = e.undo(),
            _ => _ = e.redo(),
        }
        let s = e.scene([0.0, 0.0, 800.0, 600.0]);
        assert!(!s.rects.iter().any(|r| r.r[0] == 48.0), "no stale offset");
        e.input(crate::Input::Up {
            x: 60.0,
            y: 20.0,
            mods: 0,
        });
        assert_eq!(e.board().design.elements[0].x, Some(8), "case {interrupt}");
    }
}

#[test]
fn labels_are_culled_by_the_viewport() {
    let mut e = engine();
    e.scene([0.0, 0.0, 300.0, 200.0]);
    let ids: Vec<String> = e.labels().into_iter().map(|l| l.id).collect();
    assert_eq!(ids, ["lane:0", "lane:1", "fn-a", "fn-b"]);
}

#[test]
fn lanes_span_from_the_leftmost_box() {
    let mut e = engine();
    e.ops(
        &[Op::Move {
            id: "fn-a".into(),
            x: -200,
            y: 8,
        }],
        Step::User,
    );
    e.camera.x = -400.0;
    let s = e.scene([0.0, 0.0, 1200.0, 600.0]);
    let lane = s.rects.iter().find(|r| r.paint == Paint::Lane).unwrap();
    assert_eq!((lane.r[0], lane.r[2]), (-240.0, 780.0));
}

#[test]
fn select_keeps_only_live_ids() {
    let mut e = engine();
    e.select(vec!["fn-b".into(), "fn-gone".into(), "system:pg".into()]);
    assert_eq!(e.out().selection, ["fn-b", "system:pg"]);
}

#[test]
fn saved_takes_the_written_board_not_a_later_edit() {
    let mut e = engine();
    e.ops(
        &[Op::Update {
            id: "fn-a".into(),
            set: ElementPatch {
                name: Some("A".into()),
                ..Default::default()
            },
        }],
        Step::User,
    );
    let written = e.board().clone();
    // an edit lands while the write is in flight
    e.ops(
        &[Op::Update {
            id: "fn-b".into(),
            set: ElementPatch {
                name: Some("B".into()),
                ..Default::default()
            },
        }],
        Step::User,
    );
    e.saved(written.clone());
    e.external(written, None); // the watcher reports our own write
    let names: Vec<&str> = e
        .board()
        .design
        .elements
        .iter()
        .map(|x| x.name.as_str())
        .collect();
    assert_eq!(names, ["A", "B"]);
}

/// The wasm smoke test (viewer/test/wasm.smoke.test.ts) expects this exact text from `board()`.
#[test]
fn smoke_fixture_is_the_canonical_text() {
    let d = ig_design::parse(r#"{"version":1,"title":"Smoke","scope":{"kind":"dir","id":"src"}}"#);
    let mut e = Engine::new(
        Context::default(),
        Board {
            design: d.unwrap(),
            screens: Default::default(),
        },
    );
    let add = |id: &str, kind, x| Op::Add {
        id: Some(id.into()),
        kind,
        name: id.into(),
        file: None,
        method: None,
        path: None,
        layer: None,
        on: None,
        x: Some(x),
        y: Some(8),
        w: None,
        h: None,
    };
    let link = Op::Link {
        from: "function-a".into(),
        to: "system-pg".into(),
        kind: ig_design::LinkKind::Uses,
    };
    let ops = [
        add("system-pg", ElementKind::System, 200),
        add("function-a", ElementKind::Function, 8),
        link,
    ];
    assert_eq!(e.ops(&ops, Step::User).error, None);
    let text = ig_design::to_text(&e.board().design);
    assert_eq!(
        text,
        include_str!("../../../viewer/test/fixtures/smoke-design.json")
    );
}

#[test]
fn set_context_keeps_the_board_and_history_and_drops_gone_boxes() {
    let mut e = engine();
    e.select(vec!["fn-a".into(), "system:pg".into()]);
    let before = e.board().clone();
    e.set_context(Context::default());
    assert_eq!(e.board(), &before);
    assert_eq!(e.selection, vec!["fn-a".to_string()]);
    assert_eq!(e.hit(450.0, 260.0), None);
    let o = e.ops(&[Op::Remove { id: "fn-b".into() }], Step::User);
    assert!(o.changed);
    assert!(e.undo().changed);
    assert_eq!(e.board(), &before);
}

#[test]
fn an_amend_joins_the_placement_into_one_undo_step() {
    let mut e = engine();
    let before = e.board().clone();
    let add = Op::Add {
        id: Some("fn-c".into()),
        kind: ElementKind::Function,
        name: "C".into(),
        file: None,
        method: None,
        path: None,
        layer: None,
        on: None,
        x: Some(8),
        y: Some(200),
        w: None,
        h: None,
    };
    e.ops(&[add], Step::User);
    let set = ElementPatch {
        file: Some("server/c.ts".into()),
        ..Default::default()
    };
    let o = e.ops(
        &[Op::Update {
            id: "fn-c".into(),
            set,
        }],
        Step::Amend,
    );
    assert!(o.changed);
    assert!(e.undo().changed);
    assert_eq!(e.board(), &before);
}

#[test]
fn new_elements_are_dashed_context_and_overlays_are_not() {
    let mut e = engine();
    let s = e.scene([0.0, 0.0, 1200.0, 800.0]);
    let dashed: Vec<bool> = (s.rects.iter())
        .map(|r| crate::scene::dashed(r.paint))
        .collect();
    assert!(dashed.iter().any(|d| *d));
    for r in &s.rects {
        let el = matches!(r.paint, Paint::Element(k) if k != ElementKind::Note && k != ElementKind::Frame);
        assert_eq!(crate::scene::dashed(r.paint), el, "{:?}", r.paint);
    }
    assert!(!crate::scene::dashed(Paint::Element(ElementKind::Note)));
    assert!(!crate::scene::dashed(Paint::Context));
}

fn mv(id: &str, x: i32, y: i32) -> Op {
    Op::Move {
        id: id.into(),
        x,
        y,
    }
}

#[test]
fn revert_of_an_older_agent_step_keeps_a_later_user_edit() {
    let mut e = engine();
    let mut d1 = renamed(e.board(), "fn-b", "B");
    let mut n = el("fn-n", ElementKind::Function);
    n.y = Some(200);
    d1.design.elements.push(n);
    e.external(d1.clone(), Some("t1"));
    e.ops(&[mv("fn-a", 8, 104)], Step::User);
    let mut d2 = e.board().clone(); // t2 read the file with the user's move saved
    let mut m = el("fn-m", ElementKind::Function);
    m.y = Some(300);
    d2.design.elements.push(m);
    e.saved(e.board().clone());
    e.external(d2, Some("t2"));
    let o = e.revert("t1");
    assert!(o.changed);
    let b = &e.board().design;
    assert!(
        b.elements.iter().all(|x| x.id != "fn-n"),
        "the agent's add is gone"
    );
    assert_eq!(b.elements[1].name, "b", "its rename is undone");
    assert_eq!(b.elements[0].y, Some(104), "the user's later move stays");
    assert!(
        b.elements.iter().any(|x| x.id == "fn-m"),
        "a later turn stays"
    );
    assert_eq!(e.highlighted(), ["fn-m"], "only t2's highlight stays");
    e.undo();
    assert_eq!(
        e.board().design.elements.len(),
        4,
        "the revert is one undo step"
    );
    assert!(!e.revert("t9").changed, "an unknown turn changes nothing");
}

#[test]
fn concurrent_user_and_agent_edits_on_different_elements_both_survive() {
    let mut e = engine();
    let disk = renamed(e.board(), "fn-b", "B"); // the agent read the file before the user's move
    e.ops(&[mv("fn-a", 8, 104)], Step::User);
    let o = e.external(disk, Some("t1"));
    assert_eq!(o.touched, ["fn-b"]);
    let b = &e.board().design;
    assert_eq!(
        (b.elements[0].y, b.elements[1].name.as_str()),
        (Some(104), "B")
    );
}

#[test]
fn the_users_in_flight_drag_wins() {
    let mut e = engine();
    e.input(crate::Input::Down {
        x: 20.0,
        y: 20.0,
        button: 0,
        mods: 0,
    });
    e.input(crate::Input::Move {
        x: 60.0,
        y: 20.0,
        mods: 0,
    });
    let disk = apply(e.board(), &[mv("fn-a", 8, 304)]).unwrap().0;
    e.external(renamed(&disk, "fn-a", "A"), Some("t1"));
    e.input(crate::Input::Up {
        x: 60.0,
        y: 20.0,
        mods: 0,
    });
    let a = &e.board().design.elements[0];
    assert_eq!((a.x, a.y, a.name.as_str()), (Some(48), Some(8), "A"));
}

#[test]
fn user_move_and_agent_rename_of_one_element_both_survive() {
    let mut e = engine();
    let disk = renamed(e.board(), "fn-a", "A"); // read before the user's move was saved
    e.ops(&[mv("fn-a", 8, 104)], Step::User);
    let o = e.external(disk, Some("t1"));
    let a = &e.board().design.elements[0];
    assert_eq!((a.y, a.name.as_str()), (Some(104), "A"));
    assert_eq!(o.touched, ["fn-a"]);
}

#[test]
fn user_rename_beats_agent_rename() {
    let mut e = engine();
    let disk = renamed(e.board(), "fn-a", "Claude's");
    e.ops(
        &[Op::Update {
            id: "fn-a".into(),
            set: ElementPatch {
                name: Some("mine".into()),
                ..Default::default()
            },
        }],
        Step::User,
    );
    e.external(disk, Some("t1"));
    assert_eq!(e.board().design.elements[0].name, "mine");
}

#[test]
fn revert_of_a_claude_rename_keeps_the_users_later_move() {
    let mut e = engine();
    e.external(renamed(e.board(), "fn-b", "B"), Some("t1"));
    e.ops(&[mv("fn-b", 208, 104)], Step::User);
    assert!(e.revert("t1").changed);
    let b = &e.board().design.elements[1];
    assert_eq!((b.name.as_str(), b.y), ("b", Some(104)));
}

#[test]
fn revert_keeps_an_agent_added_element_the_user_linked() {
    let mut e = engine();
    let mut d = e.board().clone();
    let mut n = el("fn-n", ElementKind::Function);
    n.y = Some(200);
    d.design.elements.push(n);
    e.external(d, Some("t1"));
    let link = Op::Link {
        from: "fn-a".into(),
        to: "fn-n".into(),
        kind: ig_design::LinkKind::Calls,
    };
    e.ops(&[link], Step::User);
    e.revert("t1");
    let b = &e.board().design;
    assert!(b.elements.iter().any(|x| x.id == "fn-n"));
    assert!(b.links.iter().any(|l| l.key() == "fn-a|calls|fn-n"));
}
