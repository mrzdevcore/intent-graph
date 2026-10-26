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
}
