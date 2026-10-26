//! Grid snapping, rows and the board's own editing rules on top of `ig_design::apply`.

use crate::{Lane, RowKind};
use ig_design::{Board, Element, ElementKind, ElementPatch, LinkKind, Op};

pub const GRID: i32 = 8;

pub fn snap(v: f32) -> i32 {
    (v / GRID as f32).round() as i32 * GRID
}

/// The layer whose row holds `y` ("Other" and "External services" give none).
pub fn row_at(lanes: &[Lane], y: f32) -> Option<&str> {
    lanes
        .iter()
        .find(|l| l.kind == RowKind::Layer && y >= l.top && y <= l.bottom)
        .map(|l| l.name.as_str())
}

pub(crate) fn takes_layer(k: ElementKind) -> bool {
    matches!(
        k,
        ElementKind::Module | ElementKind::Function | ElementKind::Route
    )
}

/// The size drawn when the file gives none.
pub(crate) fn default_size(k: ElementKind) -> (i32, i32) {
    match k {
        ElementKind::Frame => (320, 200),
        ElementKind::Note => (160, 80),
        _ => (144, 48),
    }
}

/// `[x, y, w, h]` in world px, or None while unplaced.
pub(crate) fn rect_of(e: &Element) -> Option<[f32; 4]> {
    let (dw, dh) = default_size(e.kind);
    Some([
        e.x? as f32,
        e.y? as f32,
        e.w.unwrap_or(dw) as f32,
        e.h.unwrap_or(dh) as f32,
    ])
}

/// What a new box must not land on: the context boxes and the placed elements (frames hold
/// others, so they don't count).
pub(crate) fn taken(ctx: &crate::Context, b: &Board) -> Vec<[f32; 4]> {
    let ctx = ctx.boxes.iter().map(|c| [c.x, c.y, c.w, c.h]);
    let els = (b.design.elements.iter())
        .filter(|e| e.kind != ElementKind::Frame)
        .filter_map(rect_of);
    ctx.chain(els).collect()
}

/// The nearest x to `x`, on the grid along the row (right first), where a `w`×`h` box at `y`
/// keeps a grid step clear of every box in `taken`.
pub(crate) fn free_x(taken: &[[f32; 4]], x: i32, y: i32, w: i32, h: i32) -> i32 {
    let g = GRID as f32;
    let hits = |x: i32| {
        let (x, y, w, h) = (x as f32, y as f32, w as f32, h as f32);
        (taken.iter()).any(|t| {
            x < t[0] + t[2] + g && t[0] < x + w + g && y < t[1] + t[3] + g && t[1] < y + h + g
        })
    };
    // ponytail: linear scan of every box per step; an index if boards pass a few thousand boxes
    (0..4000)
        .flat_map(|k| [x + k * GRID, x - k * GRID])
        .find(|&x| !hits(x))
        .unwrap_or(x)
}

/// Moves by `(dx, dy)`, snapped to the grid; layer elements take the row's layer, and only
/// systems may land in External services (others stay put). Unplaced or unknown ids are skipped.
pub fn move_ops(b: &Board, ids: &[String], dx: f32, dy: f32, lanes: &[Lane]) -> Vec<Op> {
    let mut ops = vec![];
    for e in b.design.elements.iter().filter(|e| ids.contains(&e.id)) {
        let (Some(x), Some(y)) = (e.x, e.y) else {
            continue;
        };
        let y = snap(y as f32 + dy);
        let mid = y as f32 + e.h.unwrap_or(default_size(e.kind).1) as f32 / 2.0;
        let lane = lanes.iter().find(|l| mid >= l.top && mid <= l.bottom);
        if lane.is_some_and(|l| l.kind == RowKind::External) && e.kind != ElementKind::System {
            continue;
        }
        ops.push(Op::Move {
            id: e.id.clone(),
            x: snap(x as f32 + dx),
            y,
        });
        let layer = row_at(lanes, mid);
        if takes_layer(e.kind) && e.layer.as_deref() != layer {
            ops.push(Op::Update {
                id: e.id.clone(),
                set: ElementPatch {
                    layer: Some(layer.unwrap_or_default().into()), // "" clears
                    ..Default::default()
                },
            });
        }
    }
    ops
}

/// The kind a new link gets from its ends' kinds (element kinds or graph kinds).
pub fn default_link_kind(from: &str, to: &str) -> LinkKind {
    match (from, to) {
        (_, "route") => LinkKind::Http,
        (_, "system") => LinkKind::Uses,
        ("module", "module") => LinkKind::Imports,
        _ => LinkKind::Calls,
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use ig_design::{Scope, apply, empty};

    pub fn lanes() -> Vec<Lane> {
        let l = |row, name: &str, kind, top, bottom| Lane {
            row,
            name: name.into(),
            kind,
            top,
            bottom,
        };
        vec![
            l(0, "api", RowKind::Layer, 0.0, 100.0),
            l(1, "Other", RowKind::Other, 120.0, 200.0),
            l(2, "External services", RowKind::External, 220.0, 300.0),
        ]
    }

    pub fn el(id: &str, kind: ElementKind) -> Element {
        let mut e: Element = serde_json::from_value(serde_json::json!({
            "id": id, "kind": kind, "name": &id[id.find('-').map_or(0, |i| i + 1)..],
        }))
        .unwrap();
        (e.x, e.y, e.w, e.h) = (Some(8), Some(8), Some(80), Some(40));
        e
    }

    pub fn board(elements: Vec<Element>) -> Board {
        let mut d = empty("t", Scope::Layer { name: "x".into() });
        d.elements = elements;
        Board {
            design: d,
            screens: Default::default(),
        }
    }

    fn moved(b: &Board, ids: &[&str], dx: f32, dy: f32) -> Board {
        let ids: Vec<String> = ids.iter().map(|s| s.to_string()).collect();
        apply(b, &move_ops(b, &ids, dx, dy, &lanes())).unwrap().0
    }

    #[test]
    fn move_snaps_to_8_and_takes_the_lanes_layer() {
        let mut a = el("fn-a", ElementKind::Function);
        (a.layer, a.h) = (Some("x".into()), Some(0));
        let b = board(vec![a]);
        let e = &moved(&b, &["fn-a"], 13.0, 20.0).design.elements[0];
        assert_eq!(
            (e.x, e.y, e.layer.as_deref()),
            (Some(24), Some(32), Some("api"))
        );
        let e = &moved(&b, &["fn-a"], 0.0, 150.0).design.elements[0];
        assert_eq!(e.layer, None);
    }

    #[test]
    fn refuses_a_non_system_in_external_services() {
        let mut a = el("fn-a", ElementKind::Function);
        let mut s = el("sys-db", ElementKind::System);
        (a.h, s.h) = (Some(0), Some(0));
        let b = moved(&board(vec![a, s]), &["fn-a", "sys-db"], 0.0, 240.0);
        assert_eq!(b.design.elements[0].y, Some(8));
        assert_eq!(b.design.elements[1].y, Some(248));
    }

    #[test]
    fn default_link_kinds() {
        assert_eq!(default_link_kind("page", "route"), LinkKind::Http);
        assert_eq!(default_link_kind("function", "system"), LinkKind::Uses);
        assert_eq!(default_link_kind("module", "module"), LinkKind::Imports);
        assert_eq!(default_link_kind("function", "function"), LinkKind::Calls);
    }
}
