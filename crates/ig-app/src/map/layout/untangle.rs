//! Boxes of a group trading places so the arrows between them cross and cut through boxes less.

use super::{Frame, LONG, Lay, THROUGH, UNTANGLE};
use std::collections::BTreeMap;

/// Whether segments pq and rs cross at a point inside both.
pub(super) fn crosses(p: (f32, f32), q: (f32, f32), r: (f32, f32), s: (f32, f32)) -> bool {
    let side = |a: (f32, f32), b: (f32, f32), c: (f32, f32)| {
        ((b.0 - a.0) * (c.1 - a.1) - (b.1 - a.1) * (c.0 - a.0)).signum()
    };
    side(p, q, r) * side(p, q, s) < 0.0 && side(r, s, p) * side(r, s, q) < 0.0
}

/// Whether segment pq runs through frame `f` (its edges not counted).
pub(super) fn cuts(p: (f32, f32), q: (f32, f32), f: &Frame) -> bool {
    let (mut t0, mut t1) = (0.0f32, 1.0f32);
    let d = (q.0 - p.0, q.1 - p.1);
    for (dp, lo, hi) in [
        (d.0, f.x - p.0, f.right() - p.0),
        (d.1, f.y - p.1, f.bottom() - p.1),
    ] {
        if dp == 0.0 {
            if lo >= 0.0 || hi <= 0.0 {
                return false;
            }
        } else {
            let (a, b) = (lo / dp, hi / dp);
            t0 = t0.max(a.min(b));
            t1 = t1.min(a.max(b));
        }
    }
    t0 < t1
}

impl Lay<'_, '_> {
    /// Boxes of one group trade places while that leaves the arrows between them all less
    /// tangled (see `tangle`), each drawn straight between box centres; the first trade found
    /// better is kept, until none is. Returns the tangle left, None with fewer than two arrows.
    pub(super) fn untangle(&mut self) -> Option<f32> {
        let at: BTreeMap<&str, usize> = (self.frames.iter().enumerate())
            .map(|(i, f)| (f.0, i))
            .collect();
        let mut edges = vec![];
        for (i, (id, _)) in self.frames.iter().enumerate() {
            for n in self.near.get(id).into_iter().flatten() {
                match at.get(n) {
                    Some(&j) if j > i => edges.push((i, j)),
                    _ => {}
                }
            }
        }
        if edges.len() < 2 {
            return None;
        }
        let mut best = self.tangle(&edges);
        for _ in 0..UNTANGLE {
            let mut better = false;
            for (start, end, any) in self.groups.clone() {
                for i in start..end {
                    for j in i + 1..end {
                        let (a, b) = (self.frames[i].1, self.frames[j].1);
                        if !any && (a.w, a.h) != (b.w, b.h) {
                            continue;
                        }
                        self.trade(i, j);
                        let t = self.tangle(&edges);
                        if t < best - 1e-3 {
                            (best, better) = (t, true);
                        } else {
                            self.trade(i, j);
                        }
                    }
                }
            }
            if !better {
                break;
            }
        }
        Some(best)
    }

    /// Boxes `i` and `j` swap centres.
    fn trade(&mut self, i: usize, j: usize) {
        let (a, b) = (self.frames[i].1, self.frames[j].1);
        let c = |f: &Frame| (f.x + f.w / 2.0, f.y + f.h / 2.0);
        let (ca, cb) = (c(&a), c(&b));
        self.frames[i].1 = Frame {
            x: cb.0 - a.w / 2.0,
            y: cb.1 - a.h / 2.0,
            ..a
        };
        self.frames[j].1 = Frame {
            x: ca.0 - b.w / 2.0,
            y: ca.1 - b.h / 2.0,
            ..b
        };
        for k in [i, j] {
            let (id, f) = self.frames[k];
            self.cx.insert(id, f.x + f.w / 2.0);
        }
    }

    /// How tangled the arrows `edges` (pairs of frame indexes) are, drawn straight between box
    /// centres: 1 per two crossing, `THROUGH` per box one cuts through, and their length.
    fn tangle(&self, edges: &[(usize, usize)]) -> f32 {
        let c = |i: usize| {
            let f = self.frames[i].1;
            (f.x + f.w / 2.0, f.y + f.h / 2.0)
        };
        let segs: Vec<_> = edges.iter().map(|&(a, b)| (c(a), c(b))).collect();
        let mut t = 0.0;
        for (k, &(a, b)) in edges.iter().enumerate() {
            let (p, q) = segs[k];
            t += ((q.0 - p.0).abs() + (q.1 - p.1).abs()) / LONG;
            for (m, &(e, g)) in edges.iter().enumerate().skip(k + 1) {
                let (r, s) = segs[m];
                if ![a, b].contains(&e) && ![a, b].contains(&g) && crosses(p, q, r, s) {
                    t += 1.0;
                }
            }
            for (i, (_, f)) in self.frames.iter().enumerate() {
                if i != a && i != b && cuts(p, q, f) {
                    t += THROUGH;
                }
            }
        }
        t
    }
}
