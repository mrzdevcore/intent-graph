//! Hit-testing: a uniform grid of 256 world px cells over the boxes; links by distance.

use crate::Context;
use crate::snap::rect_of;
use ig_design::{Board, ElementKind};
use std::collections::{HashMap, HashSet};

const CELL: f32 = 256.0;

/// A line segment, world px.
pub(crate) type Seg = ([f32; 2], [f32; 2]);

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Target {
    El(String),
    Ctx(String),
    Link(String),
    /// A screen's frame (its pieces are `El` by piece key, its navs `Link` by nav key).
    Screen(String),
}

impl Target {
    pub fn id(&self) -> &str {
        match self {
            Target::El(s) | Target::Ctx(s) | Target::Link(s) | Target::Screen(s) => s,
        }
    }
}

/// Draw order, low to high: context boxes, frames, links, elements.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Tier {
    Ctx,
    Frame,
    El,
    /// Pens and arrows: in the marquee by their box, hit by distance to their line (`lines`).
    Line,
}

#[derive(Default)]
pub(crate) struct Index {
    boxes: Vec<(Target, [f32; 4], Tier)>,
    grid: HashMap<(i32, i32), Vec<u32>>,
    /// Link key, its ends' centers.
    // ponytail: links are scanned linearly (0.1 µs at 2k); grid them past ~20k links
    pub segs: Vec<(String, [f32; 2], [f32; 2])>,
    /// Pen and arrow pieces: their key and the segments of their line (world).
    // ponytail: scanned linearly like `segs`; grid them if pens run to tens of thousands of points
    pub lines: Vec<(String, Vec<Seg>)>,
    /// Box centers by element or graph id.
    pub centers: HashMap<String, [f32; 2]>,
}

fn cells(r: [f32; 4]) -> impl Iterator<Item = (i32, i32)> {
    let c = |v: f32| (v / CELL).floor() as i32;
    let (x0, x1, y0, y1) = (c(r[0]), c(r[0] + r[2]), c(r[1]), c(r[1] + r[3]));
    (x0..=x1).flat_map(move |x| (y0..=y1).map(move |y| (x, y)))
}

fn inside(r: [f32; 4], x: f32, y: f32) -> bool {
    x >= r[0] && x <= r[0] + r[2] && y >= r[1] && y <= r[1] + r[3]
}

fn seg_dist(p: [f32; 2], a: [f32; 2], b: [f32; 2]) -> f32 {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let l2 = dx * dx + dy * dy;
    let t = if l2 == 0.0 {
        0.0
    } else {
        (((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / l2).clamp(0.0, 1.0)
    };
    ((p[0] - a[0] - t * dx).powi(2) + (p[1] - a[1] - t * dy).powi(2)).sqrt()
}

impl Index {
    pub fn build(b: &Board, ctx: &Context, hidden: &HashSet<String>) -> Index {
        let shown = |l: &Option<String>| l.as_ref().is_none_or(|l| !hidden.contains(l));
        let mut ix = Index::default();
        for c in ctx.boxes.iter().filter(|c| shown(&c.layer)) {
            let r = [c.x, c.y, c.w, c.h];
            ix.add(Target::Ctx(c.id.clone()), r, Tier::Ctx);
        }
        for e in b.design.elements.iter().filter(|e| shown(&e.layer)) {
            if let Some(r) = rect_of(e) {
                let tier = if e.kind == ElementKind::Frame {
                    Tier::Frame
                } else {
                    Tier::El
                };
                ix.add(Target::El(e.id.clone()), r, tier);
            }
        }
        for l in &b.design.links {
            if let (Some(a), Some(z)) = (ix.centers.get(&l.from), ix.centers.get(&l.to)) {
                ix.segs.push((l.key(), *a, *z));
            }
        }
        ix
    }

    /// The screens surface: frames, then pieces (world rects), then navs from a piece's center
    /// to the nearest point of the screen it leads to. Backgrounds are locked: never hit.
    pub fn build_screens(b: &Board) -> Index {
        use crate::screen_scene::{nav_ends, piece_line, piece_rect, screen_rect};
        use crate::screens::{nav_key, piece_key};
        let mut ix = Index::default();
        for s in b.screens.values() {
            ix.add(Target::Screen(s.id.clone()), screen_rect(s), Tier::Frame);
        }
        for s in b.screens.values() {
            for p in &s.elements {
                let key = piece_key(&s.id, &p.id);
                let r = piece_rect(s, p, (0.0, 0.0));
                let line = piece_line(s, p);
                let tier = if line.is_empty() {
                    Tier::El
                } else {
                    Tier::Line
                };
                ix.add(Target::El(key.clone()), r, tier);
                if !line.is_empty() {
                    ix.lines.push((key, line));
                }
            }
        }
        for s in b.screens.values() {
            for n in &s.navs {
                if let Some((a, z)) = nav_ends(b, s, n, &|_| (0.0, 0.0)) {
                    ix.segs.push((nav_key(&s.id, n), a, z));
                }
            }
        }
        ix
    }

    fn add(&mut self, t: Target, r: [f32; 4], tier: Tier) {
        let i = self.boxes.len() as u32;
        self.centers
            .insert(t.id().to_string(), [r[0] + r[2] / 2.0, r[1] + r[3] / 2.0]);
        for c in cells(r) {
            self.grid.entry(c).or_default().push(i);
        }
        self.boxes.push((t, r, tier));
    }

    pub fn rect(&self, t: &Target) -> Option<[f32; 4]> {
        self.boxes.iter().find(|b| &b.0 == t).map(|b| b.1)
    }

    /// The topmost thing at a world point: elements, then links within `tol`, frames, context.
    pub fn at(&self, x: f32, y: f32, tol: f32) -> Option<Target> {
        let here = self.grid.get(&cells([x, y, 0.0, 0.0]).next().unwrap());
        // later boxes of a tier are drawn above earlier ones
        let top = |tier: Tier| {
            here.into_iter()
                .flatten()
                .map(|&i| &self.boxes[i as usize])
                .rev()
                .find(|b| b.2 == tier && inside(b.1, x, y))
                .map(|b| b.0.clone())
        };
        let near =
            |l: &[([f32; 2], [f32; 2])]| l.iter().any(|(a, b)| seg_dist([x, y], *a, *b) <= tol);
        (self.lines.iter().rev())
            .find(|(_, l)| near(l))
            .map(|(k, _)| Target::El(k.clone()))
            .or_else(|| top(Tier::El))
            .or_else(|| {
                self.segs
                    .iter()
                    .rev()
                    .find(|s| seg_dist([x, y], s.1, s.2) <= tol)
                    .map(|s| Target::Link(s.0.clone()))
            })
            .or_else(|| top(Tier::Frame))
            .or_else(|| top(Tier::Ctx))
    }

    /// Elements and context boxes wholly inside a world rect, in draw order.
    pub fn within(&self, r: [f32; 4]) -> Vec<Target> {
        let mut seen = HashSet::new();
        let mut hits: Vec<u32> = cells(r)
            .filter_map(|c| self.grid.get(&c))
            .flatten()
            .copied()
            .filter(|i| seen.insert(*i))
            .filter(|&i| {
                let b = self.boxes[i as usize].1;
                inside(r, b[0], b[1]) && inside(r, b[0] + b[2], b[1] + b[3])
            })
            .collect();
        hits.sort();
        hits.into_iter()
            .map(|i| self.boxes[i as usize].0.clone())
            .collect()
    }
}
