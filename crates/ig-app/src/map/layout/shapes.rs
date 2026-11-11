//! The pattern shapes of a parent's inside: onion rings, layered bands, a hexagonal core.

use super::{CHIP, Frame, GUTTER, Lay, PAD, RegionKind};
use crate::map::file::MapBox;

impl<'a> Lay<'a, '_> {
    /// `items` around `inner`, `gap` away: the first `side` (in order) in a column on its left,
    /// the last `side` on its right, the rest split alternately into rows of `cols` above and
    /// below. Returns the bounds of `inner` and them all.
    fn around(
        &mut self,
        items: Vec<&'a MapBox>,
        inner: Frame,
        gap: f32,
        (cols, side): (usize, usize),
    ) -> Frame {
        let items = self.order(items);
        let side = side.min(items.len() / 2);
        let (left, rest) = items.split_at(side);
        let (mid, right) = rest.split_at(rest.len() - side);
        let (mut top, mut bottom) = (vec![], vec![]);
        for (i, b) in mid.iter().enumerate() {
            if i % 2 == 0 {
                top.push(*b)
            } else {
                bottom.push(*b)
            }
        }
        let mut all = inner;
        for b in [
            self.grid(top, cols, inner.y - gap, true),
            self.grid(bottom, cols, inner.bottom() + gap, false),
        ]
        .into_iter()
        .flatten()
        {
            all = all.union(&b);
        }
        for (column, left) in [(left, true), (right, false)] {
            let total: f32 = column.iter().map(|b| self.size(b).1 + GUTTER).sum::<f32>() - GUTTER;
            let mut y = inner.y + (inner.h - total) / 2.0;
            for b in column {
                let (w, h) = self.size(b);
                let x = if left {
                    inner.x - gap - w
                } else {
                    inner.right() + gap
                };
                let f = Frame { x, y, w, h };
                self.put(&b.id, f);
                all = all.union(&f);
                y += h + GUTTER;
            }
        }
        all
    }

    /// `items` around `inner` as `around` does, with the column count and the side columns'
    /// length nearest 16:10 (a side column never taller than `inner`), and the ring `name`
    /// drawn round them all when given. Returns the bounds of everything placed.
    fn ring(&mut self, items: Vec<&'a MapBox>, inner: Frame, name: Option<&str>) -> Frame {
        let h = items.iter().map(|b| self.size(b).1).fold(0.0, f32::max);
        let tall = ((inner.h + GUTTER) / (h + GUTTER)).floor() as usize;
        let n = items.len();
        let opts = (1..=n.max(1)).flat_map(|c| (0..=tall.min(n / 2)).map(move |s| (c, s)));
        self.pick(opts, |l, o| {
            let r = l.around(items.clone(), inner, GUTTER, o).grow(PAD);
            if let Some(name) = name {
                l.regions.push((RegionKind::Ring, name.into(), r));
            }
        });
        self.bounds().unwrap_or(inner)
    }

    /// Three rings, domain innermost, each as near 16:10 as its insides let it.
    pub(super) fn onion(&mut self, in_slot: &dyn Fn(&[&str]) -> Vec<&'a MapBox>) {
        let domain = in_slot(&["domain"]);
        self.pick(1..=domain.len(), |l, c| {
            l.grid(domain.clone(), c, 0.0, false);
        });
        let mut ring = self
            .bounds()
            .unwrap_or(Frame {
                x: -124.0,
                y: 0.0,
                w: 248.0,
                h: 152.0,
            })
            .grow(PAD);
        self.regions.push((RegionKind::Ring, "domain".into(), ring));
        for name in ["application", "infrastructure"] {
            ring = self.ring(in_slot(&[name]), ring, Some(name));
        }
    }

    /// One band per name, top to bottom, all as wide as the widest, the column count nearest
    /// 16:10.
    pub(super) fn layered(
        &mut self,
        bands: &[String],
        in_slot: &dyn Fn(&[&str]) -> Vec<&'a MapBox>,
    ) {
        let n = bands.iter().map(|b| in_slot(&[b]).len()).max().unwrap_or(0);
        self.pick(1..=n.max(1), |l, c| l.bands(bands, in_slot, c));
    }

    fn bands(
        &mut self,
        bands: &[String],
        in_slot: &dyn Fn(&[&str]) -> Vec<&'a MapBox>,
        cols: usize,
    ) {
        let mut y = 0.0;
        let first = self.regions.len();
        for name in bands {
            let h = self
                .grid(in_slot(&[name]), cols, y + PAD + GUTTER / 2.0, false)
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
    pub(super) fn hexagonal(&mut self, in_slot: &dyn Fn(&[&str]) -> Vec<&'a MapBox>) {
        let ports = in_slot(&["ports"]);
        let ph = ports.iter().map(|b| self.size(b).1).fold(152.0, f32::max);
        // the wider row of ports (`around` puts half of them on each edge)
        let k = ports.len().div_ceil(2);
        let pw = k as f32 * ports.iter().map(|b| self.size(b).0).fold(0.0, f32::max)
            + (k as f32 - 1.0).max(0.0) * GUTTER;
        let core = in_slot(&["core", "domain"]);
        self.pick(1..=core.len().max(1), |l, cols| {
            l.hex_core(core.clone(), cols, ports.clone(), (k.max(1), ph, pw))
        });
        let all = self.bounds().expect("the core is placed");
        self.ring(in_slot(&["adapters"]), all, None);
    }

    /// The core's boxes in rows of `cols`, the core round them as wide as the ports' row `pw`
    /// (`k` ports of height `ph` each) needs, and the ports on its top and bottom edges.
    fn hex_core(
        &mut self,
        core: Vec<&'a MapBox>,
        cols: usize,
        ports: Vec<&'a MapBox>,
        (k, ph, pw): (usize, f32, f32),
    ) {
        let core = self.grid(core, cols, 0.0, false);
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
        self.around(
            ports,
            Frame {
                y: c.y + ph / 2.0,
                h: c.h - ph,
                ..c
            },
            0.0,
            (k, 0),
        );
    }
}
