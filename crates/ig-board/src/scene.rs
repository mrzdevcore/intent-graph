//! The draw list (world px; the renderer applies the camera), labels and the accessibility list.

use crate::Severity;
use crate::engine::Engine;
use crate::input::Gesture;
use ig_design::{ElementKind, LinkKind, MarkKind};
use serde::Serialize;
use std::collections::HashMap;

/// What a rect or line looks like; the renderer maps it to the theme.
#[derive(Serialize, Clone, Copy, Debug, PartialEq)]
#[serde(tag = "t", content = "v", rename_all = "lowercase")]
pub enum Paint {
    Lane,
    Context,
    ContextEdge,
    Element(ElementKind),
    Link(LinkKind),
    Mark(MarkKind),
    /// Outlines, drawn over the thing they mark.
    Selected,
    Highlight,
    Problem(Severity),
    Marquee,
    Preview,
}

#[derive(Serialize, Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub r: [f32; 4],
    pub paint: Paint,
}

#[derive(Serialize, Clone, Copy, Debug, PartialEq)]
pub struct Line {
    pub a: [f32; 2],
    pub b: [f32; 2],
    pub paint: Paint,
}

#[derive(Serialize, Default, Debug)]
pub struct Scene {
    pub rects: Vec<Rect>,
    pub lines: Vec<Line>,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct Label {
    pub id: String,
    pub text: String,
    /// World rect.
    pub r: [f32; 4],
    pub faded: bool,
    /// The check's reason, for the hover title.
    pub title: Option<String>,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct A11yItem {
    pub id: String,
    /// An element kind or `link`.
    pub kind: String,
    pub text: String,
    pub selected: bool,
    pub highlighted: bool,
    pub problem: Option<String>,
}

fn meets(a: [f32; 4], b: [f32; 4]) -> bool {
    a[0] <= b[0] + b[2] && b[0] <= a[0] + a[2] && a[1] <= b[1] + b[3] && b[1] <= a[1] + a[3]
}

pub(crate) fn build(e: &Engine, view: [f32; 4]) -> Scene {
    let mut s = Scene::default();
    let d = &e.board().design;
    let seen = e.camera.visible(view[2], view[3]);
    let rect = |s: &mut Scene, r: [f32; 4], paint| {
        if meets(r, seen) {
            s.rects.push(Rect { r, paint });
        }
    };
    let mark = |id: &str| d.marks.iter().find(|m| m.node == id).map(|m| m.mark);
    for c in e.ctx.boxes.iter().filter(|c| e.shown(&c.layer)) {
        let r = [c.x, c.y, c.w, c.h];
        rect(&mut s, r, Paint::Context);
        if let Some(m) = mark(&c.id) {
            rect(&mut s, r, Paint::Mark(m));
        }
        if e.selection.contains(&c.id) {
            rect(&mut s, r, Paint::Selected);
        }
    }
    let at = |id: &str| e.index.centers.get(id).copied();
    for (from, to, _) in &e.ctx.edges {
        if let (Some(a), Some(b)) = (at(from), at(to)) {
            s.lines.push(Line {
                a,
                b,
                paint: Paint::ContextEdge,
            });
        }
    }
    let overlays = |s: &mut Scene, id: &str, r: [f32; 4]| {
        if e.highlights.iter().any(|h| h == id) {
            rect(s, r, Paint::Highlight);
        }
        if let Some((sev, _)) = e.checks.get(id) {
            rect(s, r, Paint::Problem(*sev));
        }
        if e.selection.iter().any(|x| x == id) {
            rect(s, r, Paint::Selected);
        }
    };
    let els: Vec<_> = d
        .elements
        .iter()
        .filter_map(|x| Some((x, e.shown_rect(x)?)))
        .collect();
    for (x, r) in els.iter().filter(|(x, _)| x.kind == ElementKind::Frame) {
        rect(&mut s, *r, Paint::Element(x.kind));
        overlays(&mut s, &x.id, *r);
    }
    let mid: HashMap<&str, [f32; 2]> = (els.iter())
        .map(|(x, r)| (x.id.as_str(), [r[0] + r[2] / 2.0, r[1] + r[3] / 2.0]))
        .collect();
    let center = |id: &str| mid.get(id).copied().or_else(|| at(id));
    for l in &d.links {
        if let (Some(a), Some(b)) = (center(&l.from), center(&l.to)) {
            let key = l.key();
            s.lines.push(Line {
                a,
                b,
                paint: Paint::Link(l.kind),
            });
            let over = [
                (e.highlights.contains(&key), Paint::Highlight),
                (e.selection.contains(&key), Paint::Selected),
            ];
            let sev = e.checks.get(&key).map(|c| (true, Paint::Problem(c.0)));
            for (_, paint) in over.into_iter().chain(sev).filter(|o| o.0) {
                s.lines.push(Line { a, b, paint });
            }
        }
    }
    for (x, r) in els.iter().filter(|(x, _)| x.kind != ElementKind::Frame) {
        rect(&mut s, *r, Paint::Element(x.kind));
        if let Some(m) = mark(&x.id) {
            rect(&mut s, *r, Paint::Mark(m));
        }
        overlays(&mut s, &x.id, *r);
    }
    match &e.gesture {
        Gesture::Marquee { start, end } => {
            let r = [
                start.0.min(end.0),
                start.1.min(end.1),
                (start.0 - end.0).abs(),
                (start.1 - end.1).abs(),
            ];
            s.rects.push(Rect {
                r,
                paint: Paint::Marquee,
            });
        }
        Gesture::Link { from, to, .. } => {
            if let Some(a) = center(from) {
                s.lines.push(Line {
                    a,
                    b: [to.0, to.1],
                    paint: Paint::Preview,
                });
            }
        }
        _ => {}
    }
    let spans = (e.ctx.boxes.iter().map(|c| (c.x, c.x + c.w)))
        .chain(els.iter().map(|(_, r)| (r[0], r[0] + r[2])));
    let (left, right) = spans.fold((f32::MAX, f32::MIN), |(l, r), (a, b)| (l.min(a), r.max(b)));
    let (left, right) = if left > right {
        (0.0, 0.0)
    } else {
        (left, right)
    };
    let lanes: Vec<Rect> = e
        .ctx
        .lanes
        .iter()
        .map(|l| Rect {
            r: [left - 40.0, l.top, right - left + 80.0, l.bottom - l.top],
            paint: Paint::Lane,
        })
        .filter(|r| meets(r.r, seen))
        .collect();
    s.rects.splice(0..0, lanes);
    s
}

/// Labels of what the last viewport shows.
pub(crate) fn labels(e: &Engine) -> Vec<Label> {
    let seen = e.camera.visible(e.view[2], e.view[3]);
    let title = |id: &str| e.checks.get(id).map(|c| c.1.clone());
    let ctx = e
        .ctx
        .boxes
        .iter()
        .filter(|c| e.shown(&c.layer))
        .map(|c| Label {
            id: c.id.clone(),
            text: c.label.clone(),
            r: [c.x, c.y, c.w, c.h],
            faded: true,
            title: title(&c.id),
        });
    let els = e.board().design.elements.iter().filter_map(|x| {
        Some(Label {
            id: x.id.clone(),
            text: x.name.clone(),
            r: e.shown_rect(x)?,
            faded: false,
            title: title(&x.id),
        })
    });
    ctx.chain(els).filter(|l| meets(l.r, seen)).collect()
}

pub(crate) fn a11y(e: &Engine) -> Vec<A11yItem> {
    let d = &e.board().design;
    let item = |id: String, kind: &str, text: String| A11yItem {
        selected: e.selection.contains(&id),
        highlighted: e.highlights.contains(&id),
        problem: e.checks.get(&id).map(|c| c.1.clone()),
        kind: kind.into(),
        text,
        id,
    };
    let els = d.elements.iter().map(|x| {
        let kind = serde_json::to_value(x.kind).unwrap();
        item(x.id.clone(), kind.as_str().unwrap(), x.name.clone())
    });
    let links = d.links.iter().map(|l| {
        let text = format!("{} {} {}", l.from, l.kind.as_str(), l.to);
        item(l.key(), "link", text)
    });
    els.chain(links).collect()
}
