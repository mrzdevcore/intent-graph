//! Where `ig view` draws each box: every box with children has its own inner space, its children
//! placed in the shape of its pattern (onion rings, layered bands, a hexagonal core, else a grid).
//! The viewer wires the arrows, since a box moved by hand moves only there.

use super::file::{MapBox, is_unread, slots};
use super::{BoxKind, Level, MapArrow, Pattern};
use serde::Serialize;
use std::collections::BTreeMap;

/// A box's place, in its parent's own inner units.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Frame {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Frame {
    fn bottom(&self) -> f32 {
        self.y + self.h
    }
    fn right(&self) -> f32 {
        self.x + self.w
    }
    fn grow(&self, d: f32) -> Frame {
        Frame {
            x: self.x - d,
            y: self.y - d,
            w: self.w + 2.0 * d,
            h: self.h + 2.0 * d,
        }
    }
    fn union(&self, o: &Frame) -> Frame {
        let (x, y) = (self.x.min(o.x), self.y.min(o.y));
        Frame {
            x,
            y,
            w: self.right().max(o.right()) - x,
            h: self.bottom().max(o.bottom()) - y,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum RegionKind {
    Ring,
    Band,
    Group,
}

/// A labelled area of a parent (an onion ring, a layered band, a hexagon part).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Region {
    pub parent: String,
    pub kind: RegionKind,
    pub label: String,
    pub frame: Frame,
}

/// Where everything is drawn.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Placed {
    pub frames: BTreeMap<String, Frame>,
    pub inner: BTreeMap<String, (f32, f32)>,
    pub regions: Vec<Region>,
}

/// The least space between two boxes, and around the inside of a parent.
pub(crate) const GUTTER: f32 = 96.0;
/// A ring's or band's margin around its boxes.
const PAD: f32 = 24.0;
/// The shape every inside aims for, width over height: a screen's, near the boxes' own.
const WIDE: f32 = 1.6;
/// How much worse too tall is than too wide (see `Lay::skew`).
const TALL: f32 = 1.4;
/// How many times `untangle` goes over every trade at most.
const UNTANGLE: usize = 6;
/// What a box an arrow cuts through costs, against 1 for two arrows crossing.
const THROUGH: f32 = 1.5;
/// The arrow length that costs as much as two arrows crossing.
const LONG: f32 = 1200.0;
/// The width of the people's and the outside systems' rows.
const ROW: usize = 6;
const CHIP: (f32, f32) = (120.0, 40.0);

/// A box's size at "near" detail.
fn size(b: &MapBox) -> (f32, f32) {
    if b.parent.as_deref().is_some_and(|p| is_unread(b, p)) {
        return CHIP;
    }
    match (b.kind.value, b.level) {
        (BoxKind::Person, _) => (180.0, 136.0),
        (BoxKind::System, _) | (_, Level::System) => (280.0, 152.0),
        (_, Level::Container) => (264.0, 152.0),
        (_, Level::Component) => (248.0, 152.0),
        (_, Level::Code) => (220.0, 112.0),
    }
}

/// A table box's width, its header's height and a column's row: the viewer draws its card in
/// these.
pub const TABLE: (f32, f32, f32) = (320.0, 56.0, 26.0);

/// A table box's size: its header and a row for each of its `rows` columns (2 at least).
fn table_size(rows: u32) -> (f32, f32) {
    (TABLE.0, TABLE.1 + TABLE.2 * rows.max(2) as f32 + 8.0)
}

/// Every box with children laid out in its inner space; the notices name the arrows with an end
/// that has no place.
pub fn layout(boxes: &[MapBox], arrows: &[MapArrow]) -> (Placed, Vec<String>) {
    layout_with(boxes, arrows, &BTreeMap::new())
}

/// `layout`, a table box as tall as its rows (its column count, by box id).
pub fn layout_with(
    boxes: &[MapBox],
    arrows: &[MapArrow],
    rows: &BTreeMap<String, u32>,
) -> (Placed, Vec<String>) {
    let by_id: BTreeMap<&str, &MapBox> = boxes.iter().map(|b| (b.id.as_str(), b)).collect();
    let mut kids: BTreeMap<&str, Vec<&MapBox>> = BTreeMap::new();
    for b in by_id.values() {
        if let Some(p) = b.parent.as_deref() {
            kids.entry(p).or_default().push(b);
        }
    }
    let mut near: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    let mut sorted: Vec<&MapArrow> = arrows.iter().collect();
    sorted.sort_by(|a, b| a.id.cmp(&b.id));
    for a in &sorted {
        near.entry(&a.from).or_default().push(&a.to);
        near.entry(&a.to).or_default().push(&a.from);
    }
    let mut placed = Placed::default();
    for (parent, kids) in &kids {
        let mut l = Lay {
            near: &near,
            rows,
            cx: BTreeMap::new(),
            frames: vec![],
            regions: vec![],
            groups: vec![],
        };
        let pattern = by_id
            .get(parent)
            .and_then(|b| b.pattern.as_ref())
            .map(|f| &f.value);
        match by_id.get(parent) {
            Some(b) if b.level == Level::System || b.parent.is_none() => l.system(kids),
            _ => l.pattern(parent, pattern, kids),
        }
        l.untangle();
        let (w, h) = l.normalize();
        placed.inner.insert(parent.to_string(), (w, h));
        for (id, f) in l.frames {
            placed.frames.insert(id.to_string(), f);
        }
        placed
            .regions
            .extend(l.regions.into_iter().map(|(kind, label, frame)| Region {
                parent: parent.to_string(),
                kind,
                label,
                frame,
            }));
    }
    let notices = arrows
        .iter()
        .filter(|a| !placed.frames.contains_key(&a.from) || !placed.frames.contains_key(&a.to))
        .map(|a| format!("map: arrow `{}` has an end with no place; not drawn", a.id))
        .collect();
    (placed, notices)
}

/// Whether segments pq and rs cross at a point inside both.
fn crosses(p: (f32, f32), q: (f32, f32), r: (f32, f32), s: (f32, f32)) -> bool {
    let side = |a: (f32, f32), b: (f32, f32), c: (f32, f32)| {
        ((b.0 - a.0) * (c.1 - a.1) - (b.1 - a.1) * (c.0 - a.0)).signum()
    };
    side(p, q, r) * side(p, q, s) < 0.0 && side(r, s, p) * side(r, s, q) < 0.0
}

/// Whether segment pq runs through frame `f` (its edges not counted).
fn cuts(p: (f32, f32), q: (f32, f32), f: &Frame) -> bool {
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

/// One parent's layout, centered on x = 0 until `normalize`.
#[derive(Clone)]
struct Lay<'a> {
    near: &'a BTreeMap<&'a str, Vec<&'a str>>,
    /// See `layout_with`.
    rows: &'a BTreeMap<String, u32>,
    /// Centers of the boxes placed so far, for the barycenter order.
    cx: BTreeMap<&'a str, f32>,
    frames: Vec<(&'a str, Frame)>,
    regions: Vec<(RegionKind, String, Frame)>,
    /// The runs of `frames` placed together (start, end, any two may trade places: a grid's;
    /// else only two of one size: a column's), for `untangle`.
    groups: Vec<(usize, usize, bool)>,
}

impl<'a> Lay<'a> {
    /// `b`'s size at "near" detail (`size`), a table's from its rows.
    fn size(&self, b: &MapBox) -> (f32, f32) {
        match self.rows.get(&b.id) {
            Some(n) if b.kind.value == BoxKind::Table => table_size(*n),
            _ => size(b),
        }
    }

    /// `items` ordered by the mean center of their placed neighbours (those with none last),
    /// ties by id.
    fn order(&self, mut items: Vec<&'a MapBox>) -> Vec<&'a MapBox> {
        let bary = |b: &MapBox| {
            let xs: Vec<f32> = self
                .near
                .get(b.id.as_str())
                .into_iter()
                .flatten()
                .filter_map(|n| self.cx.get(n).copied())
                .collect();
            if xs.is_empty() {
                f32::MAX
            } else {
                xs.iter().sum::<f32>() / xs.len() as f32
            }
        };
        items.sort_by(|a, b| bary(a).total_cmp(&bary(b)).then(a.id.cmp(&b.id)));
        items
    }

    /// How far the inside so far is from 16:10 (0 when exactly), leaning wide: too tall counts
    /// `TALL` times as far as too wide, as the boxes' frames are wider than 16:10.
    fn skew(&self) -> f32 {
        self.bounds().map_or(0.0, |b| {
            let r = ((b.w + 2.0 * GUTTER) / (b.h + 2.0 * GUTTER) / WIDE).ln();
            if r < 0.0 { -r * TALL } else { r }
        })
    }

    /// Runs `f` with each option on a copy and keeps the copy nearest 16:10 (the first of equals).
    fn pick<T>(&mut self, opts: impl IntoIterator<Item = T>, f: impl Fn(&mut Self, T)) {
        let mut best: Option<(f32, Self)> = None;
        for o in opts {
            let mut l = self.clone();
            f(&mut l, o);
            let s = l.skew();
            if best.as_ref().is_none_or(|b| s < b.0) {
                best = Some((s, l));
            }
        }
        if let Some((_, l)) = best {
            *self = l;
        }
    }

    /// Places `items` in rows of `cols`, centered on x = 0, the top at `top` (or the bottom at
    /// `-top` when `up`). Returns their bounds, None when there are none.
    fn grid(&mut self, items: Vec<&'a MapBox>, cols: usize, top: f32, up: bool) -> Option<Frame> {
        if items.is_empty() {
            return None;
        }
        let items = self.order(items);
        let start = self.frames.len();
        let sizes: Vec<(f32, f32)> = items.iter().map(|b| self.size(b)).collect();
        let cw = sizes.iter().map(|s| s.0).fold(0.0, f32::max);
        let rows: Vec<&[(f32, f32)]> = sizes.chunks(cols).collect();
        let heights: Vec<f32> = rows
            .iter()
            .map(|r| r.iter().map(|s| s.1).fold(0.0, f32::max))
            .collect();
        let total = heights.iter().sum::<f32>() + GUTTER * (rows.len() - 1) as f32;
        let mut y = if up { top - total } else { top };
        let mut bounds: Option<Frame> = None;
        let mut i = 0;
        for (row, rh) in rows.iter().zip(&heights) {
            let rw = cw * row.len() as f32 + GUTTER * (row.len() - 1) as f32;
            for (k, s) in row.iter().enumerate() {
                let cx = -rw / 2.0 + k as f32 * (cw + GUTTER) + cw / 2.0;
                let f = Frame {
                    x: cx - s.0 / 2.0,
                    y: y + (rh - s.1) / 2.0,
                    w: s.0,
                    h: s.1,
                };
                self.put(&items[i].id, f);
                bounds = Some(bounds.map_or(f, |b| b.union(&f)));
                i += 1;
            }
            y += rh + GUTTER;
        }
        self.groups.push((start, self.frames.len(), true));
        bounds
    }

    fn put(&mut self, id: &'a str, f: Frame) {
        self.cx.insert(id, f.x + f.w / 2.0);
        self.frames.push((id, f));
    }

    fn bounds(&self) -> Option<Frame> {
        let fs = self.frames.iter().map(|f| f.1);
        fs.chain(self.regions.iter().map(|r| r.2))
            .reduce(|a, b| a.union(&b))
    }

    /// The people, the containers, then the outside systems and stores, each group apart: the
    /// people in a row on top or a column on the left, the containers' grid, the outside in a
    /// row below or a column on the right; the column count and sides nearest 16:10.
    fn system(&mut self, kids: &[&'a MapBox]) {
        let rank = |b: &MapBox| match b.kind.value {
            BoxKind::Person => 0,
            BoxKind::External
            | BoxKind::Store
            | BoxKind::Queue
            | BoxKind::Model
            | BoxKind::System => 2,
            _ => 1,
        };
        let row =
            |r| -> Vec<&'a MapBox> { kids.iter().filter(|b| rank(b) == r).copied().collect() };
        let sides = [(false, false), (true, false), (false, true), (true, true)];
        let opts = (1..=kids.len()).flat_map(|c| sides.map(|s| (c, s)));
        self.pick(opts, |l, (c, (left, right))| {
            let (mut y, mut people) = (0.0, None);
            if left {
                people = l.column(row(0), 0.0, 0.0);
            } else if let Some(b) = l.grid(row(0), ROW, 0.0, false) {
                y = b.bottom() + GUTTER;
            }
            let n = l.frames.len();
            if let (Some(m), Some(p)) = (l.grid(row(1), c, y, false), people) {
                l.shift(n, p.right() + GUTTER - m.x);
            }
            match l.bounds() {
                Some(b) if right => l.column(row(2), b.right() + GUTTER, y),
                b => l.grid(row(2), ROW, b.map_or(0.0, |b| b.bottom() + GUTTER), false),
            };
        });
    }

    /// `items` stacked top to bottom, their left at `x` and the top at `top`. Returns their
    /// bounds, None when there are none.
    fn column(&mut self, items: Vec<&'a MapBox>, x: f32, top: f32) -> Option<Frame> {
        let mut y = top;
        let mut bounds: Option<Frame> = None;
        let start = self.frames.len();
        for b in self.order(items) {
            let (w, h) = self.size(b);
            let f = Frame { x, y, w, h };
            self.put(&b.id, f);
            bounds = Some(bounds.map_or(f, |b| b.union(&f)));
            y += h + GUTTER;
        }
        self.groups.push((start, self.frames.len(), false));
        bounds
    }

    /// The children of a container or component in its pattern's shape; those with no slot of
    /// it in a grid below, and refresh's unread box as a chip in the bottom right corner.
    fn pattern(&mut self, parent: &str, p: Option<&Pattern>, kids: &[&'a MapBox]) {
        let ok = slots(p);
        let slot = |b: &'a MapBox| -> Option<&'a str> {
            let s = b.slot.as_ref().map(|s| s.value.as_str())?;
            (!b.stale && ok.contains(&s)).then_some(s)
        };
        let in_slot = |names: &[&str]| -> Vec<&'a MapBox> {
            kids.iter()
                .filter(|b| slot(b).is_some_and(|s| names.contains(&s)))
                .copied()
                .collect()
        };
        let unread: Vec<&MapBox> = kids
            .iter()
            .filter(|b| is_unread(b, parent))
            .copied()
            .collect();
        let loose: Vec<&MapBox> = kids
            .iter()
            .filter(|b| slot(b).is_none() && !is_unread(b, parent))
            .copied()
            .collect();
        match p {
            Some(Pattern::Onion) => self.onion(&in_slot),
            Some(Pattern::Layered { bands }) => self.layered(bands, &in_slot),
            Some(Pattern::Hexagonal) => self.hexagonal(&in_slot),
            Some(Pattern::None) | None => {}
        }
        // below the pattern's shape, or beside it: whichever is nearer 16:10
        let shape = self.bounds();
        let sides: &[bool] = if shape.is_some() {
            &[false, true]
        } else {
            &[false]
        };
        let opts = (1..=loose.len()).flat_map(|c| sides.iter().map(move |&s| (c, s)));
        self.pick(opts, |l, (c, beside)| {
            let Some(s) = shape else {
                l.grid(loose.clone(), c, 0.0, false);
                return;
            };
            let n = l.frames.len();
            let top = if beside { s.y } else { s.bottom() + GUTTER };
            if let Some(g) = l.grid(loose.clone(), c, top, false)
                && beside
            {
                l.shift(n, s.right() + GUTTER - g.x);
            }
        });
        for u in unread {
            self.pick([false, true], |l, right| {
                let (x, y) = l.bounds().map_or((0.0, 0.0), |b| {
                    if right {
                        (b.right() + GUTTER, b.bottom() - CHIP.1)
                    } else {
                        (b.right() - CHIP.0, b.bottom() + GUTTER)
                    }
                });
                let (w, h) = CHIP;
                l.put(&u.id, Frame { x, y, w, h });
            });
        }
    }

    /// Boxes of one group trade places while that leaves the arrows between them all less
    /// tangled (see `tangle`), each drawn straight between box centres; the first trade found
    /// better is kept, until none is.
    fn untangle(&mut self) {
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
            return;
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

    /// Moves the boxes placed from the `from`th on right by `dx`.
    fn shift(&mut self, from: usize, dx: f32) {
        for (id, f) in &mut self.frames[from..] {
            f.x += dx;
            self.cx.insert(id, f.x + f.w / 2.0);
        }
    }

    /// Moves everything so the bounds start at the gutter; returns the inner size.
    fn normalize(&mut self) -> (f32, f32) {
        let Some(b) = self.bounds() else {
            return (2.0 * GUTTER, 2.0 * GUTTER);
        };
        let (dx, dy) = (GUTTER - b.x, GUTTER - b.y);
        let fs = self.frames.iter_mut().map(|f| &mut f.1);
        for f in fs.chain(self.regions.iter_mut().map(|r| &mut r.2)) {
            f.x += dx;
            f.y += dy;
        }
        (b.w + 2.0 * GUTTER, b.h + 2.0 * GUTTER)
    }
}

mod shapes;
#[cfg(test)]
mod tests;
