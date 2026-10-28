//! Arrow routes: Manhattan paths through the gutters between sibling boxes, on a grid of half a
//! gutter whose cells under a box are blocked.

use super::layout::{Frame, GUTTER};
use std::cmp::Reverse;
use std::collections::BinaryHeap;

const CELL: f32 = GUTTER / 2.0;
/// Extra cost of a turn, so routes keep straight.
const TURN: u32 = 3;
/// Right, down, left, up.
const DIRS: [(i32, i32); 4] = [(1, 0), (0, 1), (-1, 0), (0, -1)];

/// The route from the middle of `from`'s edge facing `to` to 1 short of `to`'s facing edge,
/// crossing none of `boxes` (every sibling, both ends included) in an `inner` space; None when
/// the gutters do not connect them.
pub fn route(
    boxes: &[Frame],
    from: &Frame,
    to: &Frame,
    inner: (f32, f32),
) -> Option<Vec<(f32, f32)>> {
    let cols = (inner.0 / CELL).ceil() as i32;
    let rows = (inner.1 / CELL).ceil() as i32;
    let blocked: Vec<bool> = (0..rows * cols)
        .map(|i| {
            let (x, y) = ((i % cols) as f32 * CELL, (i / cols) as f32 * CELL);
            boxes
                .iter()
                .any(|f| f.x < x + CELL && f.x + f.w > x && f.y < y + CELL && f.y + f.h > y)
        })
        .collect();
    let at = |c: (i32, i32)| (0..cols).contains(&c.0) && (0..rows).contains(&c.1);
    let free = |c: (i32, i32)| at(c) && !blocked[(c.1 * cols + c.0) as usize];
    let center = |c: (i32, i32)| ((c.0 as f32 + 0.5) * CELL, (c.1 as f32 + 0.5) * CELL);
    // the first free cell from `p` going `d`
    let out = |p: (f32, f32), d: (i32, i32)| {
        let p = (p.0 + d.0 as f32 * 0.5, p.1 + d.1 as f32 * 0.5);
        let mut c = ((p.0 / CELL).floor() as i32, (p.1 / CELL).floor() as i32);
        while at(c) && !free(c) {
            c = (c.0 + d.0, c.1 + d.1);
        }
        free(c).then_some(c)
    };
    let (d, s) = side(from, to);
    let (dt, e) = side(to, from);
    let e = (e.0 + dt.0 as f32, e.1 + dt.1 as f32);
    let (c0, c1) = (out(s, d)?, out(e, dt)?);
    let path = search(cols, rows, &free, c0, c1, d)?;
    // leave along the edge's normal into the first free cell; arrive the same way
    let mut pts = vec![s];
    pts.push(if d.0 == 0 {
        (s.0, center(c0).1)
    } else {
        (center(c0).0, s.1)
    });
    pts.extend(path.iter().map(|&c| center(c)));
    let last = center(c1);
    pts.push(if dt.0 == 0 {
        (e.0, last.1)
    } else {
        (last.0, e.1)
    });
    pts.push(e);
    let pts = simplify(pts);
    // a gutter narrower than a cell lets the walk out of an end cross a neighbour
    let others = boxes.iter().filter(|f| *f != from && *f != to);
    let clear = others
        .into_iter()
        .all(|f| pts.windows(2).all(|w| !crosses(w[0], w[1], f)));
    clear.then_some(pts)
}

/// The segment `p`–`q` (axis-aligned) passes through the open inside of `f`.
fn crosses(p: (f32, f32), q: (f32, f32), f: &Frame) -> bool {
    let (x0, x1) = (p.0.min(q.0), p.0.max(q.0));
    let (y0, y1) = (p.1.min(q.1), p.1.max(q.1));
    x0 < f.x + f.w && x1 > f.x && y0 < f.y + f.h && y1 > f.y
}

/// The edge of `a` facing `b`: its outward direction and middle.
fn side(a: &Frame, b: &Frame) -> ((i32, i32), (f32, f32)) {
    let (ax, ay) = (a.x + a.w / 2.0, a.y + a.h / 2.0);
    let (dx, dy) = (b.x + b.w / 2.0 - ax, b.y + b.h / 2.0 - ay);
    if dy.abs() >= dx.abs() {
        if dy >= 0.0 {
            ((0, 1), (ax, a.y + a.h))
        } else {
            ((0, -1), (ax, a.y))
        }
    } else if dx > 0.0 {
        ((1, 0), (a.x + a.w, ay))
    } else {
        ((-1, 0), (a.x, ay))
    }
}

/// The cheapest cell path from `a` to `b` through free cells, a turn costing extra, setting
/// off in direction `d`.
fn search(
    cols: i32,
    rows: i32,
    free: &dyn Fn((i32, i32)) -> bool,
    a: (i32, i32),
    b: (i32, i32),
    d: (i32, i32),
) -> Option<Vec<(i32, i32)>> {
    let n = (cols * rows) as usize;
    let state = |c: (i32, i32), k: usize| (c.1 * cols + c.0) as usize * 4 + k;
    let mut cost = vec![u32::MAX; n * 4];
    let mut prev = vec![usize::MAX; n * 4];
    let mut heap = BinaryHeap::new();
    let k0 = DIRS.iter().position(|&x| x == d).unwrap_or(0);
    cost[state(a, k0)] = 0;
    heap.push(Reverse((0, state(a, k0))));
    while let Some(Reverse((c, s))) = heap.pop() {
        if c > cost[s] {
            continue;
        }
        let (cell, k) = (s / 4, s % 4);
        let here = ((cell % cols as usize) as i32, (cell / cols as usize) as i32);
        if here == b {
            let mut path = vec![here];
            let mut s = s;
            while prev[s] != usize::MAX {
                s = prev[s];
                let cell = s / 4;
                path.push(((cell % cols as usize) as i32, (cell / cols as usize) as i32));
            }
            path.reverse();
            path.dedup();
            return Some(path);
        }
        for (j, dir) in DIRS.iter().enumerate() {
            let next = (here.0 + dir.0, here.1 + dir.1);
            if !free(next) {
                continue;
            }
            let nc = c + 1 + if j == k { 0 } else { TURN };
            let ns = state(next, j);
            if nc < cost[ns] {
                cost[ns] = nc;
                prev[ns] = s;
                heap.push(Reverse((nc, ns)));
            }
        }
    }
    None
}

/// Without repeated points and the middle points of straight runs.
fn simplify(pts: Vec<(f32, f32)>) -> Vec<(f32, f32)> {
    let mut out: Vec<(f32, f32)> = vec![];
    for p in pts {
        if out.last() == Some(&p) {
            continue;
        }
        if let [.., a, b] = out[..]
            && ((a.0 == b.0 && b.0 == p.0) || (a.1 == b.1 && b.1 == p.1))
        {
            out.pop();
        }
        out.push(p);
    }
    out
}
