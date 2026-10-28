//! Arrow routes: Manhattan paths through the gutters between sibling boxes, on a grid of half a
//! gutter whose cells under a box are blocked.

use super::layout::{Frame, GUTTER};
use std::cmp::Reverse;
use std::collections::{BTreeMap, BinaryHeap};

const CELL: f32 = GUTTER / 2.0;
/// Extra cost of a turn, so routes keep straight.
const TURN: u32 = 3;
/// Right, down, left, up.
const DIRS: [(i32, i32); 4] = [(1, 0), (0, 1), (-1, 0), (0, -1)];

/// Extra cost of a step through a cell another route already takes, so routes spread out.
const USED: u32 = 2;
/// The gap between two lanes in one gutter, and the farthest a lane moves.
const LANE: f32 = 6.0;
const LANE_MAX: f32 = 10.0;

/// One parent's inner space as cells: those under a sibling are blocked, and each counts the
/// routes through it.
pub struct Grid<'a> {
    boxes: &'a [Frame],
    cols: i32,
    rows: i32,
    blocked: Vec<bool>,
    used: Vec<u32>,
}

impl<'a> Grid<'a> {
    /// `boxes` are every child of the parent, `inner` its size.
    pub fn new(boxes: &'a [Frame], inner: (f32, f32)) -> Grid<'a> {
        let cols = (inner.0 / CELL).ceil() as i32;
        let rows = (inner.1 / CELL).ceil() as i32;
        let blocked = (0..rows * cols)
            .map(|i| {
                let (x, y) = ((i % cols) as f32 * CELL, (i / cols) as f32 * CELL);
                boxes
                    .iter()
                    .any(|f| f.x < x + CELL && f.x + f.w > x && f.y < y + CELL && f.y + f.h > y)
            })
            .collect();
        Grid {
            boxes,
            cols,
            rows,
            blocked,
            used: vec![0; (rows * cols) as usize],
        }
    }

    fn at(&self, c: (i32, i32)) -> bool {
        (0..self.cols).contains(&c.0) && (0..self.rows).contains(&c.1)
    }

    fn free(&self, c: (i32, i32)) -> bool {
        self.at(c) && !self.blocked[(c.1 * self.cols + c.0) as usize]
    }

    /// The first free cell from `p` going `d`.
    fn out(&self, p: (f32, f32), d: (i32, i32)) -> Option<(i32, i32)> {
        let p = (p.0 + d.0 as f32 * 0.5, p.1 + d.1 as f32 * 0.5);
        let mut c = ((p.0 / CELL).floor() as i32, (p.1 / CELL).floor() as i32);
        while self.at(c) && !self.free(c) {
            c = (c.0 + d.0, c.1 + d.1);
        }
        self.free(c).then_some(c)
    }

    /// The route from the middle of `from`'s edge facing `to` to 1 short of `to`'s facing
    /// edge, crossing no other box; None when the gutters do not connect them.
    pub fn route(&mut self, from: &Frame, to: &Frame) -> Option<Vec<(f32, f32)>> {
        let center = |c: (i32, i32)| ((c.0 as f32 + 0.5) * CELL, (c.1 as f32 + 0.5) * CELL);
        let (d, s) = side(from, to);
        let (dt, e) = side(to, from);
        let e = (e.0 + dt.0 as f32, e.1 + dt.1 as f32);
        let (c0, c1) = (self.out(s, d)?, self.out(e, dt)?);
        let path = self.search(c0, c1, d)?;
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
        let mut others = self.boxes.iter().filter(|f| *f != from && *f != to);
        if !others.all(|f| pts.windows(2).all(|w| !crosses(w[0], w[1], f))) {
            return None;
        }
        for c in path {
            self.used[(c.1 * self.cols + c.0) as usize] += 1;
        }
        Some(pts)
    }

    /// The cheapest cell path from `a` to `b` through free cells, a turn costing extra, setting
    /// off in direction `d`.
    fn search(&self, a: (i32, i32), b: (i32, i32), d: (i32, i32)) -> Option<Vec<(i32, i32)>> {
        let cols = self.cols;
        let n = (cols * self.rows) as usize;
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
                if !self.free(next) {
                    continue;
                }
                let busy = USED * self.used[(next.1 * cols + next.0) as usize];
                let nc = c + 1 + busy + if j == k { 0 } else { TURN };
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

/// An interior segment on its line: (from, to, route, segment).
type Seg = (f32, f32, usize, usize);

/// Moves apart the interior segments (not a route's first or last) that share a stretch of one
/// line, or come within reach of it once their neighbours move: each such group, in route order, spreads `LANE` apart around the line, at most
/// `LANE_MAX` off it, closer when there are many (a cell's middle is half a cell from any box). The points at both ends of
/// a segment move with it, so the routes stay Manhattan.
pub fn lanes(routes: &mut [&mut Vec<(f32, f32)>]) {
    // keyed by (vertical, the line's coordinate as bits)
    let mut lines: BTreeMap<(bool, u32), Vec<Seg>> = BTreeMap::new();
    for (r, pts) in routes.iter().enumerate() {
        for k in 1..pts.len().saturating_sub(2) {
            let (p, q) = (pts[k], pts[k + 1]);
            let (v, at, a, b) = if p.0 == q.0 {
                (true, p.0, p.1, q.1)
            } else {
                (false, p.1, p.0, q.0)
            };
            lines
                .entry((v, at.to_bits()))
                .or_default()
                .push((a.min(b), a.max(b), r, k));
        }
    }
    let mut moves = vec![];
    for ((v, _), mut segs) in lines {
        segs.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.2.cmp(&b.2)).then(a.3.cmp(&b.3)));
        let mut start = 0;
        let mut end = f32::MIN;
        for i in 0..=segs.len() {
            if i == segs.len() || segs[i].0 >= end + 2.0 * LANE_MAX {
                let mut group: Vec<(usize, usize)> =
                    segs[start..i].iter().map(|s| (s.2, s.3)).collect();
                group.sort();
                let n = group.len() as f32;
                // past 4 lanes they close up, to stay within reach
                let gap = LANE.min(2.0 * LANE_MAX / (n - 1.0).max(1.0));
                for (j, (r, k)) in group.into_iter().enumerate() {
                    let off = (j as f32 - (n - 1.0) / 2.0) * gap;
                    moves.push((r, k, v, off));
                }
                start = i;
                end = f32::MIN;
            }
            if let Some(s) = segs.get(i) {
                end = end.max(s.1);
            }
        }
    }
    for (r, k, v, off) in moves.into_iter().filter(|m| m.3 != 0.0) {
        for p in &mut routes[r][k..k + 2] {
            if v { p.0 += off } else { p.1 += off }
        }
    }
}
