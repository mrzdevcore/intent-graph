//! Pen strokes: a port of perfect-freehand 1.2.2 (https://github.com/steveruizok/perfect-freehand),
//! checked against the JS library's output in `perfect_freehand_matches_reference`. The
//! `perfect_freehand` crate (0.1.1) diverged from it. Only what ig uses: no simulated pressure,
//! no tapers, round caps, identity easing.
//!
//! The port is under perfect-freehand's licence:
//!
//! MIT License
//!
//! Copyright (c) 2021 Stephen Ruiz Ltd
//!
//! Permission is hereby granted, free of charge, to any person obtaining a copy of this software
//! and associated documentation files (the "Software"), to deal in the Software without
//! restriction, including without limitation the rights to use, copy, modify, merge, publish,
//! distribute, sublicense, and/or sell copies of the Software, and to permit persons to whom the
//! Software is furnished to do so, subject to the following conditions:
//!
//! The above copyright notice and this permission notice shall be included in all copies or
//! substantial portions of the Software.
//!
//! THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED, INCLUDING
//! BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND
//! NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM,
//! DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
//! OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.

type V = [f64; 2];

fn add(a: V, b: V) -> V {
    [a[0] + b[0], a[1] + b[1]]
}
fn sub(a: V, b: V) -> V {
    [a[0] - b[0], a[1] - b[1]]
}
fn mul(a: V, n: f64) -> V {
    [a[0] * n, a[1] * n]
}
fn per(a: V) -> V {
    [a[1], -a[0]]
}
fn dot(a: V, b: V) -> f64 {
    a[0] * b[0] + a[1] * b[1]
}
fn dist2(a: V, b: V) -> f64 {
    let d = sub(a, b);
    d[0] * d[0] + d[1] * d[1]
}
fn uni(a: V) -> V {
    mul(a, 1.0 / a[0].hypot(a[1]))
}
fn lrp(a: V, b: V, t: f64) -> V {
    add(a, mul(sub(b, a), t))
}
/// `a` rotated by `r` around `c`.
fn rot(a: V, c: V, r: f64) -> V {
    let (s, co) = r.sin_cos();
    let (x, y) = (a[0] - c[0], a[1] - c[1]);
    [x * co - y * s + c[0], x * s + y * co + c[1]]
}

const SIZE: f64 = 4.0;
const THINNING: f64 = 0.6;
const SMOOTHING: f64 = 0.5;
const STREAMLINE: f64 = 0.5;
const FIXED_PI: f64 = std::f64::consts::PI + 1e-4;

struct Sp {
    point: V,
    pressure: f64,
    vector: V,
    running: f64,
}

/// `getStrokePoints` with `last: true`.
fn stroke_points(input: &[(V, Option<f64>)]) -> Vec<Sp> {
    let t = 0.15 + (1.0 - STREAMLINE) * 0.85;
    let mut n = input.to_vec();
    if n.len() == 2 {
        // JS quirk kept: the points put in between carry no pressure (0.5 below)
        let c = n.pop().unwrap().0;
        for p in 1..5 {
            n.push((lrp(n[0].0, c, p as f64 / 4.0), None));
        }
    }
    let pressure = |p: Option<f64>, or: f64| p.filter(|p| *p >= 0.0).unwrap_or(or);
    let mut out = vec![Sp {
        point: n[0].0,
        pressure: pressure(n[0].1, 0.25),
        vector: [1.0, 1.0],
        running: 0.0,
    }];
    let (mut reached, mut running, last) = (false, 0.0, n.len() - 1);
    for (i, &(q, p)) in n.iter().enumerate().skip(1) {
        let prev = out[out.len() - 1].point;
        let point = if i == last { q } else { lrp(prev, q, t) };
        if point == prev {
            continue;
        }
        running += (point[0] - prev[0]).hypot(point[1] - prev[1]);
        if i < last && !reached {
            if running < SIZE {
                continue;
            }
            reached = true;
        }
        out.push(Sp {
            point,
            pressure: pressure(p, 0.5),
            vector: uni(sub(prev, point)),
            running,
        });
    }
    out[0].vector = out.get(1).map_or([0.0, 0.0], |s| s.vector);
    out
}

/// `getStrokeOutlinePoints`, from a stroke of 2 or more points.
fn outline(pts: &[Sp]) -> Vec<V> {
    let total = pts[pts.len() - 1].running;
    let min_dist = (SIZE * SMOOTHING).powi(2);
    let (mut left, mut right): (Vec<V>, Vec<V>) = (vec![], vec![]);
    let radius = |p: f64| SIZE * (0.5 - THINNING * (0.5 - p));
    let mut m = radius(pts[pts.len() - 1].pressure);
    let mut prev_vector = pts[0].vector;
    let (mut pl, mut pr) = (pts[0].point, pts[0].point);
    let mut sharp = false;
    let n = pts.len();
    for (i, s) in pts.iter().enumerate() {
        if i < n - 1 && total - s.running < 3.0 {
            continue;
        }
        m = radius(s.pressure).max(0.01);
        let next = if i < n - 1 {
            pts[i + 1].vector
        } else {
            s.vector
        };
        let next_dpr = if i < n - 1 { dot(s.vector, next) } else { 1.0 };
        let prev_sharp = dot(s.vector, prev_vector) < 0.0 && !sharp;
        let next_sharp = next_dpr < 0.0;
        if prev_sharp || next_sharp {
            let v = mul(per(prev_vector), m);
            let (step, mut t) = (1.0 / 13.0, 0.0);
            while t <= 1.0 {
                pl = rot(sub(s.point, v), s.point, FIXED_PI * t);
                left.push(pl);
                pr = rot(add(s.point, v), s.point, FIXED_PI * -t);
                right.push(pr);
                t += step;
            }
            sharp |= next_sharp;
            continue;
        }
        sharp = false;
        if i == n - 1 {
            let v = mul(per(s.vector), m);
            left.push(sub(s.point, v));
            right.push(add(s.point, v));
            continue;
        }
        let off = mul(per(lrp(next, s.vector, next_dpr)), m);
        let l = sub(s.point, off);
        if i <= 1 || dist2(pl, l) > min_dist {
            left.push(l);
            pl = l;
        }
        let r = add(s.point, off);
        if i <= 1 || dist2(pr, r) > min_dist {
            right.push(r);
            pr = r;
        }
        prev_vector = s.vector;
    }
    let (first, last) = (pts[0].point, pts[n - 1].point);
    let mut start_cap = vec![];
    let (step, mut t) = (1.0 / 13.0, 1.0 / 13.0);
    while t <= 1.0 {
        start_cap.push(rot(right[0], first, FIXED_PI * t));
        t += step;
    }
    let mut end_cap = vec![];
    let start = add(last, mul(per(mul(pts[n - 1].vector, -1.0)), m));
    let (step, mut t) = (1.0 / 29.0, 1.0 / 29.0);
    while t < 1.0 {
        end_cap.push(rot(start, last, FIXED_PI * 3.0 * t));
        t += step;
    }
    right.reverse();
    [left, end_cap, right, start_cap].concat()
}

/// The closed outline of a stroke (`[x, y, pressure 0-100]` points): size 4, thinning 0.6,
/// smoothing 0.5, streamline 0.5, the recorded pressure (not simulated). `[]` for fewer than 2
/// points.
pub fn pen_outline(points: &[[i32; 3]]) -> Vec<[f32; 2]> {
    if points.len() < 2 {
        return vec![];
    }
    let input: Vec<(V, Option<f64>)> = (points.iter())
        .map(|p| ([p[0] as f64, p[1] as f64], Some(p[2] as f64 / 100.0)))
        .collect();
    let pts = stroke_points(&input);
    if pts.len() < 2 {
        return vec![];
    }
    (outline(&pts).into_iter())
        .map(|[x, y]| [x as f32, y as f32])
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pen_outline_closed_for_2plus_points_empty_for_1() {
        assert!(pen_outline(&[]).is_empty());
        assert!(pen_outline(&[[0, 0, 50]]).is_empty());
        let o = pen_outline(&[[0, 0, 50], [20, 5, 50], [40, 0, 50]]);
        assert!(o.len() > 4, "{o:?}");
        // a closed ring: it runs out along one side and back along the other
        let (a, z) = (o[0], o[o.len() - 1]);
        assert!((a[0] - z[0]).hypot(a[1] - z[1]) < 6.0, "{a:?} {z:?}");
    }

    /// Outlines recorded from perfect-freehand 1.2.2 (JS) with the same options.
    #[test]
    fn perfect_freehand_matches_reference() {
        #[derive(serde::Deserialize)]
        struct Case {
            points: Vec<[i32; 3]>,
            outline: Vec<[f32; 2]>,
        }
        let cases: Vec<Case> = serde_json::from_str(include_str!("pen_reference.json")).unwrap();
        assert_eq!(cases.len(), 3);
        for c in cases {
            let got = pen_outline(&c.points);
            assert_eq!(got.len(), c.outline.len(), "{:?}", c.points);
            for (g, w) in got.iter().zip(&c.outline) {
                assert!((g[0] - w[0]).hypot(g[1] - w[1]) <= 0.5, "{g:?} vs {w:?}");
            }
        }
    }
}
