//! The screens surface's draw list, labels and accessibility list. Sketches and pen outlines are
//! tessellated once per piece (cached by key and the fields they come from), never per frame.

use crate::engine::Engine;
use crate::input::Gesture;
use crate::pen_outline;
use crate::scene::{A11yItem, Label, Line, Paint, Rect, Scene, meets};
use crate::screens::{nav_key, piece_key};
use crate::sketch::{seed_of, sketch};
use crate::tess::{fill, stroke};
use ig_design::{Board, Nav, Piece, PieceKind, Screen};
use std::collections::{HashMap, HashSet};

/// World px of a sketch line.
const INK: f32 = 1.6;

/// What a cached sketch was made from: kind, size, props and points (never the position).
#[derive(Clone, PartialEq)]
struct Source(PieceKind, i32, i32, ig_design::Props, Option<Vec<[i32; 3]>>);

#[derive(Default)]
pub(crate) struct Inks {
    by_key: HashMap<String, (Source, Vec<[f32; 2]>)>,
    /// The board changed since the last check.
    pub stale: bool,
}

fn source(p: &Piece) -> Source {
    Source(p.kind, p.w, p.h, p.props.clone(), p.points.clone())
}

fn tessellate(
    kind: PieceKind,
    w: i32,
    h: i32,
    points: &Option<Vec<[i32; 3]>>,
    seed: u64,
) -> Vec<[f32; 2]> {
    if kind == PieceKind::Pen {
        return fill(&pen_outline(points.as_deref().unwrap_or_default()))
            .corners()
            .collect();
    }
    let lines = sketch(kind, w as f32, h as f32, seed);
    lines
        .iter()
        .flat_map(|l| stroke(l, INK).corners().collect::<Vec<_>>())
        .collect()
}

pub(crate) fn screen_rect(s: &Screen) -> [f32; 4] {
    [s.x as f32, s.y as f32, s.w as f32, s.h as f32]
}

/// A piece's world rect, `off` added; arrows may run up or left (negative w, h).
pub(crate) fn piece_rect(s: &Screen, p: &Piece, off: (f32, f32)) -> [f32; 4] {
    let (w, h) = (p.w as f32, p.h as f32);
    [
        s.x as f32 + p.x as f32 + w.min(0.0) + off.0,
        s.y as f32 + p.y as f32 + h.min(0.0) + off.1,
        w.abs(),
        h.abs(),
    ]
}

/// From the piece's center to the nearest point of the screen the nav leads to. `off` gives a
/// key's drag offset (a screen id or a piece key).
pub(crate) fn nav_ends(
    b: &Board,
    s: &Screen,
    n: &Nav,
    off: &dyn Fn(&str) -> (f32, f32),
) -> Option<([f32; 2], [f32; 2])> {
    let p = s.elements.iter().find(|p| p.id == n.from)?;
    let t = b.screens.get(&n.to)?;
    let r = piece_rect(s, p, add(off(&s.id), off(&piece_key(&s.id, &p.id))));
    let a = [r[0] + r[2] / 2.0, r[1] + r[3] / 2.0];
    let o = off(&t.id);
    let tr = [t.x as f32 + o.0, t.y as f32 + o.1, t.w as f32, t.h as f32];
    let z = [
        a[0].clamp(tr[0], tr[0] + tr[2]),
        a[1].clamp(tr[1], tr[1] + tr[3]),
    ];
    Some((a, z))
}

fn add(a: (f32, f32), b: (f32, f32)) -> (f32, f32) {
    (a.0 + b.0, a.1 + b.1)
}

impl Engine {
    /// Re-tessellates what changed since the last scene and forgets what is gone.
    pub(crate) fn update_inks(&mut self) {
        if !self.inks.stale {
            return;
        }
        let b = self.board().clone();
        let mut live = HashSet::new();
        for s in b.screens.values() {
            let frame = Source(PieceKind::Frame, s.w, s.h, Default::default(), None);
            self.cache(&s.id, frame, seed_of(&s.id));
            live.insert(s.id.clone());
            for p in &s.elements {
                let key = piece_key(&s.id, &p.id);
                self.cache(&key, source(p), seed_of(&p.id));
                live.insert(key);
            }
        }
        self.inks.by_key.retain(|k, _| live.contains(k));
        self.inks.stale = false;
    }

    fn cache(&mut self, key: &str, src: Source, seed: u64) {
        if self.inks.by_key.get(key).is_some_and(|(s, _)| *s == src) {
            return;
        }
        let tris = tessellate(src.0, src.1, src.2, &src.4, seed);
        self.inks.by_key.insert(key.into(), (src, tris));
    }

    /// The drag offset of a screen id or piece key.
    pub(crate) fn offset(&self, key: &str) -> (f32, f32) {
        match &self.gesture {
            Gesture::Drag { ids, dx, dy, .. } if ids.iter().any(|i| i == key) => (*dx, *dy),
            _ => (0.0, 0.0),
        }
    }
}

pub(crate) fn build(e: &Engine, view: [f32; 4]) -> Scene {
    let mut sc = Scene::default();
    let b = e.board();
    let seen = e.camera.visible(view[2], view[3]);
    let ink = |sc: &mut Scene, key: &str, at: (f32, f32)| {
        if let Some((_, tris)) = e.inks.by_key.get(key) {
            sc.ink
                .extend(tris.iter().map(|p| [p[0] + at.0, p[1] + at.1]));
        }
    };
    let mut over: Vec<Rect> = vec![];
    let overlays = |over: &mut Vec<Rect>, key: &str, r: [f32; 4]| {
        let lit = [
            (e.highlights.iter().any(|h| h == key), Paint::Highlight),
            (e.selection.iter().any(|h| h == key), Paint::Selected),
        ];
        let sev = e.checks.get(key).map(|c| (true, Paint::Problem(c.0)));
        for (_, paint) in lit.into_iter().chain(sev).filter(|l| l.0) {
            over.push(Rect { r, paint });
        }
    };
    for s in b.screens.values() {
        let o = e.offset(&s.id);
        let r = [s.x as f32 + o.0, s.y as f32 + o.1, s.w as f32, s.h as f32];
        if !meets(r, seen) {
            continue;
        }
        sc.rects.push(Rect {
            r,
            paint: Paint::Paper,
        });
        if let Some(bg) = &s.background {
            let br = [
                r[0] + bg.x as f32,
                r[1] + bg.y as f32,
                bg.w as f32,
                bg.h as f32,
            ];
            match e.images.contains_key(&bg.asset) {
                true => sc.images.push((br, bg.asset.clone())),
                false => sc.rects.push(Rect {
                    r: br,
                    paint: Paint::Placeholder,
                }),
            }
        }
        ink(&mut sc, &s.id, (r[0], r[1]));
        overlays(&mut over, &s.id, r);
        for p in &s.elements {
            let key = piece_key(&s.id, &p.id);
            let po = add(o, e.offset(&key));
            let pr = piece_rect(s, p, po);
            if !meets(pr, seen) {
                continue;
            }
            let asset = p.props.asset.as_ref().filter(|a| e.images.contains_key(*a));
            if let (PieceKind::Image, Some(a)) = (p.kind, asset) {
                sc.images.push((pr, a.clone()));
            } else if p.kind == PieceKind::Note || p.props.primary == Some(true) {
                sc.rects.push(Rect {
                    r: pr,
                    paint: Paint::Tint(p.kind),
                });
            }
            ink(
                &mut sc,
                &key,
                (
                    s.x as f32 + p.x as f32 + po.0,
                    s.y as f32 + p.y as f32 + po.1,
                ),
            );
            overlays(&mut over, &key, pr);
        }
    }
    let off = |k: &str| e.offset(k);
    for s in b.screens.values() {
        for n in &s.navs {
            let Some((a, z)) = nav_ends(b, s, n, &off) else {
                continue;
            };
            let key = nav_key(&s.id, n);
            let (dx, dy) = (z[0] - a[0], z[1] - a[1]);
            let len = dx.hypot(dy).max(1.0);
            let (ux, uy) = (dx / len, dy / len);
            let mut seg = |a, b, paint| sc.lines.push(Line { a, b, paint });
            seg(a, z, Paint::Nav);
            for side in [-1.0, 1.0] {
                let h = [
                    z[0] - 12.0 * ux + side * 6.0 * uy,
                    z[1] - 12.0 * uy - side * 6.0 * ux,
                ];
                seg(z, h, Paint::Nav);
            }
            if e.highlights.contains(&key) {
                seg(a, z, Paint::Highlight);
            }
            if e.selection.contains(&key) {
                seg(a, z, Paint::Selected);
            }
        }
    }
    sc.top = sc.rects.len();
    sc.rects.extend(over);
    match &e.gesture {
        Gesture::Marquee { start, end } => sc.rects.push(Rect {
            r: [
                start.0.min(end.0),
                start.1.min(end.1),
                (start.0 - end.0).abs(),
                (start.1 - end.1).abs(),
            ],
            paint: Paint::Marquee,
        }),
        Gesture::Pen { points, .. } => {
            for w in points.windows(2) {
                sc.lines.push(Line {
                    a: [w[0][0], w[0][1]],
                    b: [w[1][0], w[1][1]],
                    paint: Paint::Ink,
                });
            }
        }
        Gesture::Arrow { start, to } => sc.lines.push(Line {
            a: [start.0, start.1],
            b: [to.0, to.1],
            paint: Paint::Preview,
        }),
        _ => {}
    }
    sc
}

fn title(s: &Screen) -> String {
    match &s.state {
        Some(st) if !s.title.contains(st.as_str()) => format!("{} · {st}", s.title),
        _ => s.title.clone(),
    }
}

/// The label rect of a screen's title (above its frame), a piece or a nav (at its middle).
pub(crate) fn label_rect(b: &Board, key: &str) -> Option<[f32; 4]> {
    use crate::screens::{split_key, split_nav};
    if let Some((s, f, t)) = split_nav(key) {
        let s = b.screens.get(s)?;
        let n = s.navs.iter().find(|n| n.from == f && n.to == t)?;
        let (a, z) = nav_ends(b, s, n, &|_| (0.0, 0.0))?;
        return Some([
            (a[0] + z[0]) / 2.0 - 70.0,
            (a[1] + z[1]) / 2.0 - 22.0,
            140.0,
            20.0,
        ]);
    }
    if let Some((s, p)) = split_key(key) {
        let s = b.screens.get(s)?;
        let p = s.elements.iter().find(|x| x.id == p)?;
        let mut r = piece_rect(s, p, (0.0, 0.0));
        if p.kind == PieceKind::Checkbox {
            let d = r[3].min(20.0) + 4.0;
            (r[0], r[2]) = (r[0] + d, (r[2] - d).max(40.0));
        }
        return Some(r);
    }
    let s = b.screens.get(key)?;
    Some([s.x as f32, s.y as f32 - 28.0, s.w as f32, 24.0])
}

pub(crate) fn labels(e: &Engine) -> Vec<Label> {
    let b = e.board();
    let seen = e.camera.visible(e.view[2], e.view[3]);
    let reason = |id: &str| e.checks.get(id).map(|c| c.1.clone());
    let mut out = vec![];
    let mut push = |id: String, text: String, paper: bool| {
        if let Some(r) = label_rect(b, &id).filter(|r| meets(*r, seen) && !text.is_empty()) {
            out.push(Label {
                title: reason(&id),
                id,
                text,
                r,
                faded: false,
                paper,
            });
        }
    };
    for s in b.screens.values() {
        push(s.id.clone(), title(s), false);
        for p in s.elements.iter().filter(|p| p.kind != PieceKind::Pen) {
            push(piece_key(&s.id, &p.id), p.label.clone(), true);
        }
        for n in &s.navs {
            push(nav_key(&s.id, n), n.label.clone(), false);
        }
    }
    out
}

pub(crate) fn a11y(e: &Engine) -> Vec<A11yItem> {
    let b = e.board();
    let item = |id: String, kind: &str, text: String| A11yItem {
        selected: e.selection.contains(&id),
        highlighted: e.highlights.contains(&id),
        problem: e.checks.get(&id).map(|c| c.1.clone()),
        kind: kind.into(),
        text,
        id,
        question: false,
        answer: None,
    };
    let mut out = vec![];
    for s in b.screens.values() {
        out.push(item(s.id.clone(), "screen", title(s)));
        for p in &s.elements {
            let kind = serde_json::to_value(p.kind).unwrap();
            let kind = kind.as_str().unwrap_or("piece");
            let text = if p.label.is_empty() {
                kind.to_string()
            } else {
                p.label.clone()
            };
            out.push(item(piece_key(&s.id, &p.id), kind, text));
        }
        for n in &s.navs {
            let to = b.screens.get(&n.to).map_or(n.to.clone(), title);
            out.push(item(
                nav_key(&s.id, n),
                "nav",
                format!("{} to {to}: {}", n.from, n.label),
            ));
        }
    }
    out
}
