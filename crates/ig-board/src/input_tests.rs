use super::*;
use crate::engine::tests::engine;
use crate::{Paint, Rect};

fn down(e: &mut Engine, x: f32, y: f32) -> Out {
    e.input(Input::Down {
        x,
        y,
        button: 0,
        mods: 0,
    })
}
fn mv(e: &mut Engine, x: f32, y: f32) -> Out {
    e.input(Input::Move { x, y, mods: 0 })
}
fn up(e: &mut Engine, x: f32, y: f32) -> Out {
    e.input(Input::Up { x, y, mods: 0 })
}
fn click(e: &mut Engine, x: f32, y: f32) -> Out {
    down(e, x, y);
    up(e, x, y)
}
fn key(e: &mut Engine, k: &str, mods: u8) -> Out {
    e.input(Input::Key {
        key: k.into(),
        mods,
    })
}
fn links(e: &Engine) -> Vec<String> {
    e.board().design.links.iter().map(|l| l.key()).collect()
}

#[test]
fn drag_is_one_history_step() {
    let mut e = engine();
    down(&mut e, 20.0, 20.0);
    for x in [30.0, 40.0, 60.0] {
        let o = mv(&mut e, x, 20.0);
        assert!(o.redraw && !o.changed);
    }
    assert_eq!(e.board().design.elements[0].x, Some(8), "display only");
    let s = e.scene([0.0, 0.0, 800.0, 600.0]);
    let r = Rect {
        r: [48.0, 8.0, 80.0, 40.0],
        paint: Paint::Element(ElementKind::Function),
    };
    assert!(s.rects.contains(&r));
    let o = up(&mut e, 60.0, 20.0);
    assert!(o.changed && !o.meaning);
    assert_eq!(e.board().design.elements[0].x, Some(48));
    e.undo();
    assert_eq!(e.board().design.elements[0].x, Some(8));
    assert!(!e.undo().changed);
}

#[test]
fn placing_in_a_row_reports_placed_with_layer() {
    let mut e = engine();
    e.input(Input::Tool(Tool::Function));
    let o = down(&mut e, 300.0, 50.0);
    assert_eq!(o.placed, Some(("fn-function".into(), Some("api".into()))));
    assert_eq!(o.edit.as_ref().map(|r| r.text.as_str()), Some("Function"));
    assert_eq!(o.selection, ["fn-function"]);
    assert_eq!(e.tool(), Tool::Select);
    e.input(Input::Tool(Tool::Function));
    let o = down(&mut e, 300.0, 250.0);
    assert!(
        o.error.is_some() && o.placed.is_none(),
        "not in External services"
    );
}

#[test]
fn edge_drag_links_with_default_kind() {
    let mut e = engine();
    e.input(Input::Tool(Tool::Link));
    down(&mut e, 48.0, 28.0);
    mv(&mut e, 300.0, 30.0);
    up(&mut e, 450.0, 30.0);
    assert!(links(&e).contains(&"fn-a|http|server/x.ts#GET /x".into()));
    e.input(Input::Tool(Tool::Select));
    down(&mut e, 287.0, 28.0); // fn-b's right-middle handle
    mv(&mut e, 400.0, 200.0);
    up(&mut e, 450.0, 260.0);
    assert!(links(&e).contains(&"fn-b|uses|system:pg".into()));
    assert_eq!(
        e.board().design.elements[1].x,
        Some(208),
        "a handle drag never moves"
    );
}

#[test]
fn click_on_link_cycles_kind() {
    let mut e = engine();
    let o = click(&mut e, 148.0, 28.0);
    assert_eq!(links(&e), ["fn-a|imports|fn-b"]);
    assert_eq!(o.selection, ["fn-a|imports|fn-b"]);
    e.undo();
    assert_eq!(links(&e), ["fn-a|calls|fn-b"]);
}

#[test]
fn marquee_selects() {
    let mut e = engine();
    down(&mut e, 0.0, 0.0);
    mv(&mut e, 100.0, 60.0);
    let s = e.scene([0.0, 0.0, 800.0, 600.0]);
    assert!(s.rects.iter().any(|r| r.paint == Paint::Marquee));
    let o = up(&mut e, 100.0, 60.0);
    assert_eq!(o.selection, ["fn-a"]);
    down(&mut e, 0.0, 0.0);
    let o = up(&mut e, 520.0, 300.0);
    assert_eq!(o.selection.len(), 4, "elements and context boxes");
}

#[test]
fn delete_key_removes_selection() {
    let mut e = engine();
    click(&mut e, 20.0, 20.0);
    e.input(Input::Down {
        x: 148.0,
        y: 28.0,
        button: 0,
        mods: SHIFT,
    });
    let o = key(&mut e, "Delete", 0);
    assert!(o.changed && o.error.is_none() && o.selection.is_empty());
    assert_eq!(e.board().design.elements.len(), 1);
    assert!(links(&e).is_empty());
    e.undo();
    assert_eq!(e.board(), engine().board(), "one step");
}

#[test]
fn context_boxes_cannot_move() {
    let mut e = engine();
    down(&mut e, 450.0, 30.0);
    mv(&mut e, 500.0, 80.0);
    let o = up(&mut e, 500.0, 80.0);
    assert!(!o.changed);
    assert_eq!(o.selection, ["server/x.ts#GET /x"]);
    assert_eq!(e.ctx.boxes[0].x, 400.0);
    // with an element in the selection, only the element moves
    e.input(Input::Down {
        x: 20.0,
        y: 20.0,
        button: 0,
        mods: SHIFT,
    });
    mv(&mut e, 60.0, 20.0);
    assert!(up(&mut e, 60.0, 20.0).changed);
    assert_eq!(e.board().design.elements[0].x, Some(48));
}

#[test]
fn enter_requests_edit_with_rect() {
    let mut e = engine();
    click(&mut e, 20.0, 20.0);
    e.camera.zoom = 2.0;
    let o = key(&mut e, "Enter", 0).edit.unwrap();
    assert_eq!((o.id.as_str(), o.text.as_str()), ("fn-a", "a"));
    assert_eq!(o.rect, [16.0, 16.0, 160.0, 80.0]);
    let o = e.input(Input::Down {
        x: 40.0,
        y: 40.0,
        button: 0,
        mods: DOUBLE,
    });
    assert_eq!(o.edit.map(|r| r.id), Some("fn-a".into()));
}

#[test]
fn keys_map_to_tools() {
    let mut e = engine();
    for (k, t) in [
        ("m", Tool::Module),
        ("f", Tool::Function),
        ("r", Tool::Route),
        ("e", Tool::System),
        ("n", Tool::Note),
        ("g", Tool::Frame),
        ("l", Tool::Link),
        ("v", Tool::Select),
    ] {
        key(&mut e, k, 0);
        assert_eq!(e.tool(), t, "{k}");
    }
    key(&mut e, "l", 0);
    key(&mut e, "Escape", 0);
    assert_eq!(e.tool(), Tool::Select);
    click(&mut e, 20.0, 20.0);
    key(&mut e, "ArrowRight", SHIFT);
    assert_eq!(e.board().design.elements[0].x, Some(48));
    key(&mut e, "z", META);
    assert_eq!(e.board().design.elements[0].x, Some(8));
    key(&mut e, "Z", META | SHIFT);
    assert_eq!(e.board().design.elements[0].x, Some(48));
    key(&mut e, "a", CTRL);
    assert_eq!(e.selection, ["fn-a", "fn-b"]);
    key(&mut e, "Escape", 0);
    assert!(e.selection.is_empty());
    key(&mut e, "!", SHIFT);
    assert!(e.camera().zoom <= 1.0 && e.camera().x < 0.0);
    key(&mut e, "0", CTRL);
    assert_eq!(e.camera().zoom, 1.0);
}

#[test]
fn copy_paste_and_duplicate() {
    let mut e = engine();
    key(&mut e, "a", META);
    key(&mut e, "c", META);
    let o = key(&mut e, "v", META);
    assert_eq!(o.selection, ["fn-a-2", "fn-b-2"]);
    assert!(links(&e).contains(&"fn-a-2|calls|fn-b-2".into()));
    let o = key(&mut e, "d", META);
    assert_eq!(o.selection, ["fn-a-3", "fn-b-3"]);
}

#[test]
fn paste_drops_a_note_target_no_longer_on_the_board() {
    let mut e = engine();
    let note = Op::Add {
        id: None,
        kind: ElementKind::Note,
        name: "n".into(),
        file: None,
        method: None,
        path: None,
        layer: None,
        on: Some("fn-a".into()),
        x: Some(8),
        y: Some(120),
        w: None,
        h: None,
    };
    e.ops(&[note], Step::User);
    click(&mut e, 20.0, 130.0);
    key(&mut e, "c", META);
    e.ops(&[Op::Remove { id: "fn-a".into() }], Step::User);
    let o = key(&mut e, "v", META);
    assert!(o.error.is_none(), "{:?}", o.error);
    let n = e.board().design.elements.iter().find(|x| x.id == "note-2");
    assert_eq!(n.map(|n| n.on.clone()), Some(None));
}

#[test]
fn wheel_pans_and_ctrl_wheel_zooms() {
    let mut e = engine();
    let w = |dy, mods| Input::Wheel {
        x: 100.0,
        y: 100.0,
        dx: 0.0,
        dy,
        mods,
    };
    e.input(w(50.0, 0));
    assert_eq!(e.camera().y, 50.0);
    e.input(w(-100.0, CTRL));
    assert!(e.camera().zoom > 2.0);
    e.input(Input::Down {
        x: 0.0,
        y: 0.0,
        button: 1,
        mods: 0,
    });
    let x = e.camera().x;
    mv(&mut e, -27.0, 0.0);
    assert!(e.camera().x > x);
}

#[test]
fn escape_cancels_a_drag_without_a_step() {
    let mut e = engine();
    down(&mut e, 20.0, 20.0);
    mv(&mut e, 60.0, 20.0);
    key(&mut e, "Escape", 0);
    let o = up(&mut e, 60.0, 20.0);
    assert!(!o.changed);
    assert_eq!(e.board().design.elements[0].x, Some(8));
    assert!(!e.undo().changed, "no history step");
}

#[test]
fn double_click_on_a_link_steps_its_kind_once() {
    let mut e = engine();
    click(&mut e, 148.0, 28.0);
    e.input(Input::Down {
        x: 148.0,
        y: 28.0,
        button: 0,
        mods: DOUBLE,
    });
    up(&mut e, 148.0, 28.0);
    assert_eq!(links(&e), ["fn-a|imports|fn-b"]);
}
