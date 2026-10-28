//! Where `ig view` draws each box: every box with children has its own inner space, its children
//! placed in the shape of its pattern (onion rings, layered bands, a hexagonal core, else a grid),
//! and each arrow routed through the gutters between its siblings.

use super::file::{MapBox, is_unread, slots};
use super::route::route;
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
    pub routes: BTreeMap<String, Vec<(f32, f32)>>,
}

/// The least space between two boxes, and around the inside of a parent.
pub(crate) const GUTTER: f32 = 48.0;
/// A ring's or band's margin around its boxes.
const PAD: f32 = 24.0;
const COLS: usize = 4;
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

/// Every box with children laid out in its inner space and every arrow routed; the notices
/// name the arrows drawn straight for want of a clear path.
pub fn layout(boxes: &[MapBox], arrows: &[MapArrow]) -> (Placed, Vec<String>) {
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
            cx: BTreeMap::new(),
            frames: vec![],
            regions: vec![],
        };
        let pattern = by_id
            .get(parent)
            .and_then(|b| b.pattern.as_ref())
            .map(|f| &f.value);
        match by_id.get(parent) {
            Some(b) if b.level == Level::System || b.parent.is_none() => l.system(kids),
            _ => l.pattern(parent, pattern, kids),
        }
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
    let mut notices = vec![];
    for a in sorted {
        let (Some(from), Some(to), Some(&inner)) = (
            placed.frames.get(&a.from),
            placed.frames.get(&a.to),
            placed.inner.get(a.parent.as_str()),
        ) else {
            notices.push(format!(
                "map: arrow `{}` has an end with no place; not drawn",
                a.id
            ));
            continue;
        };
        let siblings: Vec<Frame> = kids
            .get(a.parent.as_str())
            .into_iter()
            .flatten()
            .map(|b| placed.frames[&b.id])
            .collect();
        let r = route(&siblings, from, to, inner).unwrap_or_else(|| {
            notices.push(format!(
                "map: no clear path for arrow `{}`; drawn straight",
                a.id
            ));
            elbow(from, to)
        });
        placed.routes.insert(a.id.clone(), r);
    }
    (placed, notices)
}

/// From the middle of `from`'s bottom to 1 short of `to`'s side, in two segments.
fn elbow(from: &Frame, to: &Frame) -> Vec<(f32, f32)> {
    let s = (from.x + from.w / 2.0, from.bottom());
    let e = (to.x - 1.0, to.y + to.h / 2.0);
    vec![s, (s.0, e.1), e]
}

/// One parent's layout, centered on x = 0 until `normalize`.
struct Lay<'a> {
    near: &'a BTreeMap<&'a str, Vec<&'a str>>,
    /// Centers of the boxes placed so far, for the barycenter order.
    cx: BTreeMap<&'a str, f32>,
    frames: Vec<(&'a str, Frame)>,
    regions: Vec<(RegionKind, String, Frame)>,
}

impl<'a> Lay<'a> {
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

    /// Places `items` in rows of `cols`, centered on x = 0, the top at `top` (or the bottom at
    /// `-top` when `up`). Returns their bounds, None when there are none.
    fn grid(&mut self, items: Vec<&'a MapBox>, cols: usize, top: f32, up: bool) -> Option<Frame> {
        if items.is_empty() {
            return None;
        }
        let items = self.order(items);
        let sizes: Vec<(f32, f32)> = items.iter().map(|b| size(b)).collect();
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

    /// People on top, the containers, then the outside systems and stores.
    fn system(&mut self, kids: &[&'a MapBox]) {
        let rank = |b: &MapBox| match b.kind.value {
            BoxKind::Person => 0,
            BoxKind::External | BoxKind::Store | BoxKind::Queue | BoxKind::System => 2,
            _ => 1,
        };
        let row =
            |r| -> Vec<&'a MapBox> { kids.iter().filter(|b| rank(b) == r).copied().collect() };
        let mut y = 0.0;
        for (items, cols) in [(row(0), ROW), (row(1), COLS), (row(2), ROW)] {
            if let Some(b) = self.grid(items, cols, y, false) {
                y = b.bottom() + GUTTER;
            }
        }
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
        let top = self.bounds().map_or(0.0, |b| b.bottom() + GUTTER);
        self.grid(loose, COLS, top, false);
        for u in unread {
            let (x, y) = self
                .bounds()
                .map_or((0.0, 0.0), |b| (b.right() - CHIP.0, b.bottom() + GUTTER));
            self.put(
                &u.id,
                Frame {
                    x,
                    y,
                    w: CHIP.0,
                    h: CHIP.1,
                },
            );
        }
    }

    /// `items` split in two alternately (each half keeps the order): one row group above `inner`,
    /// one below. Returns the bounds of `inner` and both.
    fn around(&mut self, items: Vec<&'a MapBox>, inner: Frame, gap: f32) -> Frame {
        let items = self.order(items);
        let (mut top, mut bottom) = (vec![], vec![]);
        for (i, b) in items.into_iter().enumerate() {
            if i % 2 == 0 {
                top.push(b)
            } else {
                bottom.push(b)
            }
        }
        let mut all = inner;
        for b in [
            self.grid(top, COLS, inner.y - gap, true),
            self.grid(bottom, COLS, inner.bottom() + gap, false),
        ]
        .into_iter()
        .flatten()
        {
            all = all.union(&b);
        }
        all
    }

    /// Three rings, domain innermost.
    fn onion(&mut self, in_slot: &dyn Fn(&[&str]) -> Vec<&'a MapBox>) {
        let core = self.grid(in_slot(&["domain"]), COLS, 0.0, false);
        let mut ring = core
            .unwrap_or(Frame {
                x: -124.0,
                y: 0.0,
                w: 248.0,
                h: 152.0,
            })
            .grow(PAD);
        self.regions.push((RegionKind::Ring, "domain".into(), ring));
        for name in ["application", "infrastructure"] {
            ring = self.around(in_slot(&[name]), ring, GUTTER).grow(PAD);
            self.regions.push((RegionKind::Ring, name.into(), ring));
        }
    }

    /// One band per name, top to bottom, all as wide as the widest.
    fn layered(&mut self, bands: &[String], in_slot: &dyn Fn(&[&str]) -> Vec<&'a MapBox>) {
        let mut y = 0.0;
        let first = self.regions.len();
        for name in bands {
            let h = self
                .grid(in_slot(&[name]), COLS, y + PAD + GUTTER / 2.0, false)
                .map_or(CHIP.1, |b| b.h);
            let h = h + 2.0 * PAD + GUTTER;
            self.regions.push((
                RegionKind::Band,
                name.clone(),
                Frame {
                    x: 0.0,
                    y,
                    w: 0.0,
                    h,
                },
            ));
            y += h;
        }
        let (x, w) = self
            .frames
            .iter()
            .map(|f| f.1)
            .reduce(|a, b| a.union(&b))
            .map_or((-124.0, 248.0), |b| (b.x, b.w));
        for r in &mut self.regions[first..] {
            r.2.x = x - PAD;
            r.2.w = w + 2.0 * PAD;
        }
    }

    /// The core in the middle, the ports on its edge, the adapters around.
    fn hexagonal(&mut self, in_slot: &dyn Fn(&[&str]) -> Vec<&'a MapBox>) {
        let ports = in_slot(&["ports"]);
        let ph = ports.iter().map(|b| size(b).1).fold(152.0, f32::max);
        // the wider row of ports (`around` puts half of them on each edge)
        let k = ports.len().div_ceil(2).min(COLS) as f32;
        let pw =
            k * ports.iter().map(|b| size(b).0).fold(0.0, f32::max) + (k - 1.0).max(0.0) * GUTTER;
        let core = self.grid(in_slot(&["core", "domain"]), COLS, 0.0, false);
        let mut c = core
            .unwrap_or(Frame {
                x: -124.0,
                y: 0.0,
                w: 248.0,
                h: 152.0,
            })
            .grow(ph / 2.0 + GUTTER);
        if c.w < pw + 2.0 * GUTTER {
            c = Frame {
                x: -(pw / 2.0 + GUTTER),
                w: pw + 2.0 * GUTTER,
                ..c
            };
        }
        self.regions.push((RegionKind::Group, "core".into(), c));
        // each port's middle on the core's top or bottom edge
        let edge = self.around(
            ports,
            Frame {
                y: c.y + ph / 2.0,
                h: c.h - ph,
                ..c
            },
            0.0,
        );
        let all = edge.union(&c);
        self.around(in_slot(&["adapters"]), all, GUTTER);
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

#[cfg(test)]
mod tests;
