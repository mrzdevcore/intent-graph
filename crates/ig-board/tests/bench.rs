//! Headless engine benchmarks (release): `cargo test --release -p ig-board -- --ignored bench`.

use ig_board::{Context, CtxBox, Engine, Input, Lane, RowKind, Surface};
use ig_design::{Board, Element, Link, LinkKind, Scope, empty};
use std::time::Instant;

fn engine(elements: usize, boxes: usize) -> Engine {
    let mut d = empty("bench", Scope::Layer { name: "api".into() });
    for i in 0..elements {
        let mut e: Element = serde_json::from_value(serde_json::json!({
            "id": format!("fn-{i}"), "kind": "function", "name": format!("f{i}"),
        }))
        .unwrap();
        (e.x, e.y) = (Some((i % 40) as i32 * 160), Some((i / 40) as i32 * 80));
        d.elements.push(e);
        if i > 0 {
            d.links.push(Link {
                from: format!("fn-{}", i - 1),
                to: format!("fn-{i}"),
                kind: LinkKind::Calls,
            });
        }
    }
    let n = boxes;
    let boxes = (0..n)
        .map(|i| CtxBox {
            id: format!("src/m{i}.ts"),
            label: format!("m{i}"),
            kind: "module".into(),
            layer: Some("api".into()),
            x: 7000.0 + (i % 40) as f32 * 120.0,
            y: (i / 40) as f32 * 60.0,
            w: 100.0,
            h: 40.0,
        })
        .collect();
    let ctx = Context {
        lanes: vec![Lane {
            row: 0,
            name: "api".into(),
            kind: RowKind::Layer,
            top: 0.0,
            bottom: 4000.0,
        }],
        boxes,
        edges: (1..n)
            .map(|i| {
                (
                    format!("src/m{}.ts", i - 1),
                    format!("src/m{i}.ts"),
                    "imports".into(),
                )
            })
            .collect(),
    };
    let mut e = Engine::new(
        ctx,
        Board {
            design: d,
            screens: Default::default(),
        },
    );
    e.scene([0.0, 0.0, 1600.0, 1000.0]);
    e.input(Input::Key {
        key: "!".into(),
        mods: 1,
    }); // fit: everything is in view
    e
}

fn median_ms(mut f: impl FnMut()) -> f64 {
    let mut t: Vec<f64> = (0..50)
        .map(|_| {
            let s = Instant::now();
            f();
            s.elapsed().as_secs_f64() * 1000.0
        })
        .collect();
    t.sort_by(|a, b| a.partial_cmp(b).unwrap());
    t[25]
}

fn scene_ms(elements: usize, boxes: usize) -> f64 {
    let mut e = engine(elements, boxes);
    median_ms(|| {
        e.set_check(vec![]); // dirty
        assert!(!e.scene([0.0, 0.0, 1600.0, 1000.0]).rects.is_empty());
    })
}

#[test]
#[ignore]
fn bench_scene_200_over_1000_context() {
    let ms = scene_ms(200, 1000);
    println!("scene 200 + 1000 context: {ms:.3} ms");
    assert!(ms < 2.0);
}

#[test]
#[ignore]
fn bench_scene_2000() {
    let ms = scene_ms(2000, 1000);
    println!("scene 2000 + 1000 context: {ms:.3} ms");
    assert!(ms < 8.0);
}

#[test]
#[ignore]
fn bench_labels_2000() {
    let mut e = engine(2000, 1000);
    e.scene([0.0, 0.0, 1600.0, 1000.0]);
    let ms = median_ms(|| assert!(!e.labels().is_empty()));
    println!("labels at 2000 + 1000 context: {ms:.3} ms");
    assert!(ms < 8.0);
}

#[test]
#[ignore]
fn bench_hit_2000() {
    let e = engine(2000, 1000);
    let c = e.camera();
    let (sx, sy) = ((50.0 - c.x) * c.zoom, (100.0 - c.y) * c.zoom);
    let ms = median_ms(|| assert!(e.hit(sx, sy).is_some()));
    println!("hit at 2000: {ms:.4} ms");
    assert!(ms < 0.2);
}

#[test]
#[ignore]
fn bench_drag_frame_200() {
    let mut e = engine(200, 1000);
    let c = e.camera();
    let (sx, sy) = ((20.0 - c.x) * c.zoom, (20.0 - c.y) * c.zoom);
    e.input(Input::Down {
        x: sx,
        y: sy,
        button: 0,
        mods: 0,
    });
    let mut x = sx;
    let ms = median_ms(|| {
        x += 1.0;
        e.input(Input::Move { x, y: sy, mods: 0 });
        e.scene([0.0, 0.0, 1600.0, 1000.0]);
    });
    println!("drag frame at 200: {ms:.3} ms");
    assert!(ms < 1.0);
}

/// 5 screens of 40 sketched pieces (8 kinds, a pen stroke each 8th), all in view.
fn screens_engine(per: usize) -> Engine {
    let kinds = [
        "button", "input", "select", "table", "list", "card", "tabs", "pen",
    ];
    let screens = (0..5)
        .map(|s| {
            let elements: Vec<_> = (0..per)
                .map(|i| {
                    let k = kinds[i % kinds.len()];
                    let points: Vec<[i32; 3]> = (0..40).map(|j| [j * 4, (j % 7) * 3, 50]).collect();
                    serde_json::json!({
                        "id": format!("{k}-{i}"), "kind": k, "label": format!("p{i}"),
                        "x": (i % 5) * 250, "y": (i / 5) * 90, "w": 220, "h": 70,
                        "points": if k == "pen" { Some(points) } else { None },
                    })
                })
                .collect();
            serde_json::json!({
                "version": 1, "id": format!("s{s}"), "title": format!("S{s}"),
                "x": s * 1400, "y": 0, "w": 1280, "h": 800, "elements": elements,
            })
        })
        .map(|v| serde_json::from_value::<ig_design::Screen>(v).unwrap())
        .map(|s| (s.id.clone(), s))
        .collect();
    let mut e = Engine::new(
        Context::default(),
        Board {
            design: empty("bench", Scope::Layer { name: "api".into() }),
            screens,
        },
    );
    e.surface(Surface::Screens);
    let first = Instant::now();
    e.scene([0.0, 0.0, 1600.0, 1000.0]);
    let ms = first.elapsed().as_secs_f64() * 1000.0;
    println!("screens, 5 x {per}: first scene (tessellating) {ms:.2} ms");
    e.input(Input::Key {
        key: "!".into(),
        mods: 1,
    });
    e
}

#[test]
#[ignore]
fn bench_screens_scene_200_sketched() {
    let mut e = screens_engine(40);
    let ms = median_ms(|| {
        e.set_check(vec![]); // dirty: rebuilt from the cache
        assert!(!e.scene([0.0, 0.0, 1600.0, 1000.0]).ink.is_empty());
    });
    println!("screens scene, 5 x 40 sketched pieces, from the cache: {ms:.3} ms");
    assert!(ms < 4.0);
}

#[test]
#[ignore]
fn bench_screens_scene_2000_sketched() {
    let mut e = screens_engine(400);
    let ms = median_ms(|| {
        e.set_check(vec![]);
        assert!(!e.scene([0.0, 0.0, 1600.0, 1000.0]).ink.is_empty());
    });
    println!("screens scene, 5 x 400 sketched pieces, from the cache: {ms:.3} ms");
    assert!(ms < 40.0);
}
