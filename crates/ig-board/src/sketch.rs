//! The hand-drawn look of the screen pieces: roughr (rough.js) shapes flattened to polylines in
//! the piece's own px (0,0 is its corner). Seeded, so a piece looks the same on every redraw.

use ig_design::PieceKind;
use roughr::core::{Drawable, OpType, Options};
use roughr::generator::Generator;

pub type Polyline = Vec<[f32; 2]>;

/// Segments per Bézier curve: enough at 100 % zoom for lines a few hundred px long.
const STEPS: usize = 6;

/// A stable seed for a piece id (FNV-1a: the std hasher's output may change between releases).
pub fn seed_of(id: &str) -> u64 {
    id.bytes().fold(0xcbf2_9ce4_8422_2325, |h, b| {
        (h ^ b as u64).wrapping_mul(0x100_0000_01b3)
    })
}

struct Rough {
    g: Generator,
    seed: u64,
    out: Vec<Polyline>,
}

impl Rough {
    fn opts(&mut self) -> Option<Options> {
        // each shape of a piece gets its own seed, so two lines don't wobble alike
        self.seed = self.seed.wrapping_add(1);
        Some(Options {
            seed: Some(self.seed),
            roughness: Some(1.5),
            bowing: Some(2.0),
            ..Options::default()
        })
    }

    fn take(&mut self, d: Drawable<f32>) {
        for set in d.sets {
            let mut at = [0.0, 0.0];
            for op in set.ops {
                let v = &op.data;
                match op.op {
                    OpType::Move => {
                        at = [v[0], v[1]];
                        self.out.push(vec![at]);
                    }
                    OpType::LineTo => {
                        at = [v[0], v[1]];
                        self.line().push(at);
                    }
                    OpType::BCurveTo => {
                        let (c1, c2, to) = ([v[0], v[1]], [v[2], v[3]], [v[4], v[5]]);
                        let from = at;
                        let line = self.line();
                        for i in 1..=STEPS {
                            let t = i as f32 / STEPS as f32;
                            let u = 1.0 - t;
                            let k = [u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t];
                            line.push([
                                k[0] * from[0] + k[1] * c1[0] + k[2] * c2[0] + k[3] * to[0],
                                k[0] * from[1] + k[1] * c1[1] + k[2] * c2[1] + k[3] * to[1],
                            ]);
                        }
                        at = to;
                    }
                }
            }
        }
    }

    fn line(&mut self) -> &mut Polyline {
        if self.out.is_empty() {
            self.out.push(vec![]);
        }
        self.out.last_mut().unwrap()
    }

    fn rect(&mut self, x: f32, y: f32, w: f32, h: f32) {
        let o = self.opts();
        let d = self.g.rectangle(x, y, w, h, &o);
        self.take(d);
    }

    fn seg(&mut self, a: [f32; 2], b: [f32; 2]) {
        let o = self.opts();
        let d = self.g.line(a[0], a[1], b[0], b[1], &o);
        self.take(d);
    }

    fn ellipse(&mut self, cx: f32, cy: f32, w: f32, h: f32) {
        let o = self.opts();
        let d = self.g.ellipse(cx, cy, w, h, &o);
        self.take(d);
    }
}

/// The piece's sketch as polylines. Text, headings and pen strokes have none (their label and
/// `pen_outline` draw them).
pub fn sketch(kind: PieceKind, w: f32, h: f32, seed: u64) -> Vec<Polyline> {
    use PieceKind::*;
    let mut p = Rough {
        g: Generator::default(),
        seed,
        out: vec![],
    };
    if kind == Arrow {
        // from the corner to (w, h), either way round, with a head
        let len = w.hypot(h).max(1.0);
        let (ux, uy) = (w / len, h / len);
        let head = 12.0_f32.min(len / 2.0);
        p.seg([0.0, 0.0], [w, h]);
        for side in [-1.0, 1.0] {
            let b = [
                w - head * ux + side * head * 0.5 * uy,
                h - head * uy - side * head * 0.5 * ux,
            ];
            p.seg([w, h], b);
        }
        return p.out;
    }
    let (w, h) = (w.abs(), h.abs());
    match kind {
        Frame | Card | Input | Note | Button => p.rect(0.0, 0.0, w, h),
        Select => {
            p.rect(0.0, 0.0, w, h);
            let (x, y) = (w - 18.0, h / 2.0);
            p.seg([x - 6.0, y - 3.0], [x, y + 3.0]);
            p.seg([x, y + 3.0], [x + 6.0, y - 3.0]);
        }
        Checkbox => {
            let s = h.min(20.0);
            p.rect(0.0, (h - s) / 2.0, s, s);
        }
        Table => {
            p.rect(0.0, 0.0, w, h);
            let head = 28.0_f32.min(h / 2.0);
            p.seg([0.0, head], [w, head]);
            for i in 1..3 {
                let y = head + (h - head) * i as f32 / 3.0;
                p.seg([0.0, y], [w, y]);
                let x = w * i as f32 / 3.0;
                p.seg([x, 0.0], [x, h]);
            }
        }
        List => {
            p.rect(0.0, 0.0, w, h);
            for i in 1..4 {
                let y = h * i as f32 / 4.0;
                p.seg([8.0, y], [w - 8.0, y]);
            }
        }
        Tabs => {
            for i in 0..3 {
                p.rect(w * i as f32 / 4.0, 0.0, w / 4.0, h);
            }
            p.seg([0.0, h], [w, h]);
        }
        Modal => {
            p.rect(0.0, 0.0, w, h);
            let t = 40.0_f32.min(h / 3.0);
            p.seg([0.0, t], [w, t]);
            let (x, y) = (w - t / 2.0, t / 2.0);
            p.seg([x - 6.0, y - 6.0], [x + 6.0, y + 6.0]);
            p.seg([x - 6.0, y + 6.0], [x + 6.0, y - 6.0]);
        }
        Image => {
            p.rect(0.0, 0.0, w, h);
            p.seg([0.0, 0.0], [w, h]);
            p.seg([0.0, h], [w, 0.0]);
        }
        Icon => p.ellipse(w / 2.0, h / 2.0, w, h),
        Text | Heading | Pen | Arrow => {}
    }
    p.out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sketch_is_deterministic_per_seed() {
        let a = sketch(PieceKind::Table, 400.0, 240.0, seed_of("table-users"));
        assert_eq!(
            a,
            sketch(PieceKind::Table, 400.0, 240.0, seed_of("table-users"))
        );
        assert_ne!(
            a,
            sketch(PieceKind::Table, 400.0, 240.0, seed_of("table-orders"))
        );
        assert!(a.len() >= 5 && a.iter().all(|l| l.len() >= 2), "{a:?}");
        // hand-drawn: near the box, not on it
        let pts = a.iter().flatten();
        assert!(
            pts.clone()
                .all(|p| p[0] > -10.0 && p[0] < 410.0 && p[1] > -10.0 && p[1] < 250.0)
        );
        assert!(
            pts.clone()
                .any(|p| p[0] != p[0].round() || p[1] != p[1].round())
        );
        assert!(sketch(PieceKind::Text, 100.0, 20.0, 1).is_empty());
        let arrow = sketch(PieceKind::Arrow, -50.0, 30.0, 1);
        assert!(arrow.len() >= 3, "{arrow:?}");
    }
}
