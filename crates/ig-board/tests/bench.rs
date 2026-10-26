//! Headless engine benchmarks (release): `cargo test --release -p ig-board -- --ignored bench`.

use ig_board::{Context, CtxBox, Engine, Input, Lane, RowKind};
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
    let mut e = Engine::new(ctx, Board { design: d });
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
