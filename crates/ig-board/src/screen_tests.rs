use super::*;
use crate::screens::tests::{screen, with};
use crate::{Context, Engine, Input};
use ig_design::{Bridge, Nav};

/// login (0, 0, 400 × 300) with btn-a at (8, 8, 80 × 32); home at (500, 0).
fn engine() -> Engine {
    let mut e = Engine::new(
        Context::default(),
        with(vec![
            screen("login", 0, &[("btn-a", 8, 8)]),
            screen("home", 500, &[]),
        ]),
    );
    e.surface(Surface::Screens);
    e
}

fn drag(e: &mut Engine, a: (f32, f32), b: (f32, f32)) -> Out {
    e.input(Input::Down {
        x: a.0,
        y: a.1,
        button: 0,
        mods: 0,
    });
    e.input(Input::Move {
        x: (a.0 + b.0) / 2.0,
        y: (a.1 + b.1) / 2.0,
        mods: 0,
    });
    e.input(Input::Move {
        x: b.0,
        y: b.1,
        mods: 0,
    });
    e.input(Input::Up {
        x: b.0,
        y: b.1,
        mods: 0,
    })
}

fn key(e: &mut Engine, k: &str) -> Out {
    e.input(Input::Key {
        key: k.into(),
        mods: 0,
    })
}

#[test]
fn piece_keys_match_the_design() {
    use PieceKind::*;
    let table = [
        ("V", Tool::Select),
        ("S", Tool::Screen),
        ("U", Tool::Pieces),
        ("B", Tool::Piece(Button)),
        ("N", Tool::Piece(Input)),
        ("O", Tool::Piece(Select)),
        ("C", Tool::Piece(Checkbox)),
        ("T", Tool::Piece(Table)),
        ("L", Tool::Piece(List)),
        ("D", Tool::Piece(Card)),
        ("J", Tool::Piece(Tabs)),
        ("M", Tool::Piece(Modal)),
        ("H", Tool::Piece(Heading)),
        ("G", Tool::Piece(Image)),
        ("Y", Tool::Piece(Icon)),
        ("P", Tool::Pen),
        ("A", Tool::Arrow),
        ("X", Tool::Piece(Text)),
        ("I", Tool::Import),
        ("K", Tool::Bridge),
    ];
    let mut e = engine();
    for (k, t) in table {
        key(&mut e, "Escape");
        key(&mut e, k);
        assert_eq!(e.tool(), t, "{k}");
    }
    // the architecture keeps its own: M is a module there
    e.surface(Surface::Architecture);
    key(&mut e, "m");
    assert_eq!(e.tool(), Tool::Module);
}

#[test]
fn a_piece_snaps_and_stays_inside_its_screen_then_is_renamed() {
    let mut e = engine();
    key(&mut e, "b");
    let o = e.input(Input::Down {
        x: 395.0,
        y: 297.0,
        button: 0,
        mods: 0,
    });
    assert!(o.changed && o.error.is_none(), "{o:?}");
    let p = &e.board().screens["login"].elements[1];
    assert_eq!(
        (p.id.as_str(), p.x, p.y, p.w, p.h),
        ("btn-button", 280, 260, 120, 40)
    );
    let ed = o.edit.expect("the label is edited at once");
    assert_eq!(
        (ed.id.as_str(), ed.text.as_str()),
        ("login/btn-button", "Button")
    );
    e.rename(&ed.id, "Save");
    assert_eq!(e.board().screens["login"].elements[1].label, "Save");
    let o = e.input(Input::Down {
        x: 450.0,
        y: 10.0,
        button: 0,
        mods: 0,
    });
    assert!(o.error.is_none(), "the tool went back to select");
    key(&mut e, "b");
    let o = e.input(Input::Down {
        x: 450.0,
        y: 10.0,
        button: 0,
        mods: 0,
    });
    assert!(o.error.is_some() && !o.changed);
}

#[test]
fn variant_starts_as_a_copy_with_new_ids() {
    let mut e = engine();
    let mut b = e.board().clone();
    let login = b.screens.get_mut("login").unwrap();
    login.navs.push(Nav {
        from: "btn-a".into(),
        to: "home".into(),
        label: "go".into(),
    });
    e = Engine::new(Context::default(), b);
    e.surface(Surface::Screens);
    let o = e.variant("login", "loading");
    assert!(o.changed && o.error.is_none(), "{o:?}");
    let v = &e.board().screens["login-loading"];
    let base = &e.board().screens["login"];
    assert_eq!(v.variant_of.as_deref(), Some("login"));
    assert_eq!(v.state.as_deref(), Some("loading"));
    assert_eq!((v.x, v.y, v.w, v.h), (0, 380, 400, 300), "below its base");
    assert_eq!(v.elements, base.elements, "the same pieces");
    assert_eq!(v.navs, base.navs);
    assert_eq!(
        e.a11y()
            .iter()
            .filter(|i| i.id == "login-loading/btn-a")
            .count(),
        1
    );
    // a state of a state is one more state of the base, below the others
    e.variant("login-loading", "error");
    let w = &e.board().screens["login-error"];
    assert_eq!((w.variant_of.as_deref(), w.y), (Some("login"), 760));
    // one undo step each
    e.undo();
    e.undo();
    assert_eq!(e.board().screens.len(), 2);
    assert!(e.variant("login", " ").error.is_some());
}

#[test]
fn deleting_a_piece_drops_navs_and_bridges() {
    let mut b = engine().board().clone();
    b.screens.get_mut("login").unwrap().navs.push(Nav {
        from: "btn-a".into(),
        to: "home".into(),
        label: "go".into(),
    });
    let mut f: ig_design::Element =
        serde_json::from_value(serde_json::json!({"id": "fn-x", "kind": "function", "name": "x"}))
            .unwrap();
    f.x = Some(0);
    b.design.elements.push(f);
    b.design.bridges.push(Bridge {
        from: "login/btn-a".into(),
        to: "fn-x".into(),
    });
    let mut e = Engine::new(Context::default(), b);
    e.surface(Surface::Screens);
    e.select(vec!["login/btn-a".into(), "nav:login/btn-a>home".into()]);
    let o = key(&mut e, "Delete");
    assert!(o.changed && o.error.is_none(), "{o:?}");
    let s = &e.board().screens["login"];
    assert!(s.elements.is_empty() && s.navs.is_empty());
    assert!(e.board().design.bridges.is_empty());
    e.undo();
    assert_eq!(e.board().design.bridges.len(), 1);
    assert_eq!(e.board().screens["login"].navs.len(), 1);
}

#[test]
fn arrow_onto_a_screen_frame_creates_a_nav() {
    let mut e = engine();
    key(&mut e, "a");
    let o = drag(&mut e, (40.0, 20.0), (600.0, 100.0));
    assert!(o.changed, "{o:?}");
    let n = &e.board().screens["login"].navs;
    assert_eq!((n[0].from.as_str(), n[0].to.as_str()), ("btn-a", "home"));
    let ed = o.edit.expect("the label is edited in place");
    assert_eq!(ed.id, "nav:login/btn-a>home");
    e.rename(&ed.id, "Sign in");
    assert_eq!(e.board().screens["login"].navs[0].label, "Sign in");
    // inside one screen: an arrow piece, tied to the piece it starts on
    let o = drag(&mut e, (40.0, 20.0), (200.0, 200.0));
    assert!(o.changed && o.edit.is_none());
    let a = &e.board().screens["login"].elements[1];
    assert_eq!(
        (a.kind, a.from.as_deref(), a.to.as_deref()),
        (PieceKind::Arrow, Some("btn-a"), None)
    );
    assert_eq!((a.w, a.h), (160, 176));
}

#[test]
fn a_pen_stroke_is_one_step_with_pressure() {
    let mut e = engine();
    key(&mut e, "p");
    e.set_pressure(0.8);
    e.input(Input::Down {
        x: 100.0,
        y: 100.0,
        button: 0,
        mods: 0,
    });
    for i in 1..=10 {
        e.input(Input::Move {
            x: 100.0 + i as f32 * 5.0,
            y: 100.0 + i as f32,
            mods: 0,
        });
    }
    e.input(Input::Up {
        x: 150.0,
        y: 110.0,
        mods: 0,
    });
    let p = &e.board().screens["login"].elements[1];
    assert_eq!((p.kind, p.x, p.y), (PieceKind::Pen, 100, 100));
    let pts = p.points.as_ref().unwrap();
    assert_eq!((pts.len(), pts[0], pts[10]), (11, [0, 0, 80], [50, 10, 80]));
    assert_eq!(e.tool(), Tool::Pen, "the pen stays for the next stroke");
    e.undo();
    assert_eq!(e.board().screens["login"].elements.len(), 1);
}

#[test]
fn agent_adds_a_piece_while_the_user_moves_another_both_survive() {
    let mut e = engine();
    e.select(vec!["login/btn-a".into()]);
    let o = drag(&mut e, (20.0, 20.0), (60.0, 60.0));
    assert!(o.changed);
    let mut disk = e.merge_base().clone();
    let (b, _) = ig_design::apply(
        &disk,
        &[Op::ScreenAdd {
            screen: "login".into(),
            id: Some("table-users".into()),
            kind: PieceKind::Table,
            label: "Users".into(),
            props: None,
            x: Some(0),
            y: Some(100),
        }],
    )
    .unwrap();
    disk = b;
    let o = e.external(disk, Some("t1"));
    let s = &e.board().screens["login"];
    assert_eq!(
        (s.elements[0].x, s.elements[0].y),
        (48, 48),
        "the user's move stays"
    );
    assert_eq!(s.elements[1].id, "table-users", "the agent's piece arrives");
    assert_eq!(o.touched, ["login/table-users"]);
    assert_eq!(e.highlighted(), ["login/table-users"]);
    // Claude's turn is one undo step; the highlight goes with it
    e.undo();
    assert_eq!(e.board().screens["login"].elements.len(), 1);
    assert!(e.highlighted().is_empty());
    e.redo();
    let o = e.revert("t1");
    assert!(o.changed);
    assert_eq!(e.board().screens["login"].elements.len(), 1);
}

#[test]
fn a_chat_card_on_a_piece_switches_to_the_screens_and_an_image_becomes_the_background() {
    let mut e = engine();
    e.surface(Surface::Architecture);
    e.select(vec!["login/btn-a".into()]);
    assert_eq!(e.current_surface(), Surface::Screens);
    assert_eq!(e.selection, ["login/btn-a"]);
    e.set_image("0123456789ab.png", 800, 600);
    let o = e.import_image("0123456789ab.png");
    assert!(o.changed, "{o:?}");
    let bg = e.board().screens["login"].background.clone().unwrap();
    assert_eq!((bg.w, bg.h), (400, 300));
    let sc = e.scene([0.0, 0.0, 1200.0, 800.0]);
    assert_eq!(sc.images.len(), 1);
    assert!(!sc.ink.is_empty() && sc.ink.len().is_multiple_of(3));
    // never hit: the button and the frame are
    assert_eq!(e.hit(200.0, 200.0).as_deref(), Some("login"));
    assert_eq!(e.hit(20.0, 20.0).as_deref(), Some("login/btn-a"));
}

#[test]
fn a_click_off_a_pen_line_inside_its_box_selects_the_piece_below() {
    let mut b = engine().board().clone();
    let s = b.screens.get_mut("login").unwrap();
    // a stroke along the top and down the right of the button's box: (8, 8) to (88, 8) to (88, 40)
    let pen: Piece = serde_json::from_value(serde_json::json!({
        "id": "pen-1", "kind": "pen", "label": "", "x": 8, "y": 8, "w": 80, "h": 32,
        "points": [[0, 0, 50], [80, 0, 50], [80, 32, 50]],
    }))
    .unwrap();
    s.elements.push(pen);
    let mut e = Engine::new(Context::default(), b);
    e.surface(Surface::Screens);
    assert_eq!(
        e.hit(40.0, 26.0).as_deref(),
        Some("login/btn-a"),
        "inside the box, off the line"
    );
    assert_eq!(
        e.hit(40.0, 9.0).as_deref(),
        Some("login/pen-1"),
        "on the line"
    );
    assert_eq!(e.hit(87.0, 30.0).as_deref(), Some("login/pen-1"));
    // the marquee still takes it by its box
    e.camera.y = -20.0; // room above the screen to start a marquee
    let o = drag(&mut e, (2.0, 2.0), (120.0, 80.0));
    assert!(
        o.selection.contains(&"login/pen-1".to_string()),
        "{:?}",
        o.selection
    );
}

#[test]
fn screen_names_and_state_chips_are_labelled_at_any_zoom() {
    let mut e = engine();
    e.variant("login", "loading");
    (e.camera.zoom, e.camera.y) = (0.2, -100.0);
    e.scene([0.0, 0.0, 1200.0, 800.0]);
    let ls = e.labels();
    let name = |id: &str| ls.iter().find(|l| l.id == id).cloned();
    let login = name("login").expect("the name is there at 20 %");
    assert!(login.name && login.chip.is_none());
    let v = name("login-loading").unwrap();
    assert_eq!(
        (v.text.as_str(), v.chip.as_deref(), v.name),
        ("login", Some("LOADING"), true)
    );
    assert!(!name("login/btn-a").unwrap().name);
}

#[test]
fn ink_stays_visible_when_zoomed_out() {
    use crate::screen_scene::ink_width;
    assert_eq!(ink_width(1.0), 1.6);
    for z in [0.05, 0.1, 0.2, 0.33, 0.5, 1.0, 4.0] {
        assert!(ink_width(z) * z >= 1.2, "{z}");
        assert!(ink_width(z) * z < 2.5 || ink_width(z) == 1.6, "{z}");
    }
    let mut e = engine();
    let near = e.scene([0.0, 0.0, 1200.0, 800.0]).ink.clone();
    e.camera.zoom = 0.2;
    let far = e.scene([0.0, 0.0, 1200.0, 800.0]).ink.clone();
    assert_ne!(near, far, "re-tessellated wider");
}

#[test]
fn screen_edits_read_as_lines_and_moves_as_none() {
    let mut e = engine();
    let before = e.board().clone();
    drag(&mut e, (20.0, 20.0), (60.0, 60.0));
    assert!(crate::delta(&before, e.board()).is_empty(), "a move alone");
    e.rename("login/btn-a", "Sign in");
    e.variant("login", "error");
    key(&mut e, "a");
    drag(&mut e, (60.0, 60.0), (600.0, 100.0));
    let lines = crate::delta(&before, e.board());
    assert_eq!(
        lines,
        [
            "relabeled login/btn-a: btn-a → Sign in",
            "nav login/btn-a → screen home ()",
            "added screen login-error (login, state error of login)",
            "added button login-error/btn-a (Sign in)",
        ]
    );
}
