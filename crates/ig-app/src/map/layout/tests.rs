use super::*;
use crate::map::file::{MapFile, refresh};
use crate::map::{arrows, facts, view};
use ig_core::testing::MemFiles;
use serde_json::json;

/// The shop's boxes and arrows, code level included, as `ig view` lays them out.
fn shop() -> (Vec<MapBox>, Vec<MapArrow>) {
    let (ws, g, cfg) = crate::map::shop::load();
    let b = facts(&ws, &g, &cfg);
    let m: MapFile = refresh(None, &b, arrows(&b, &g), &[]).0;
    let v = view(&MemFiles::with(&[]), &m, &g, &[]);
    (v.boxes, v.arrows)
}

/// xorshift64: the random trees are the same on every run.
struct Rng(u64);
impl Rng {
    fn below(&mut self, n: usize) -> usize {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 % n as u64) as usize
    }
}

fn mbox(
    id: &str,
    parent: &str,
    level: &str,
    kind: &str,
    pattern: Option<serde_json::Value>,
    slot: Option<&str>,
    stale: bool,
) -> MapBox {
    let f = |v: serde_json::Value| json!({ "value": v, "source": "ig" });
    let mut b = json!({
        "id": id, "parent": parent, "level": level, "madeOf": [], "files": 0,
        "kind": f(kind.into()), "title": f(id.into()), "stale": stale,
    });
    if let Some(p) = pattern {
        b["pattern"] = f(p);
    }
    if let Some(s) = slot {
        b["slot"] = f(s.into());
    }
    serde_json::from_value(b).unwrap()
}

fn arrow(parent: &str, from: &str, to: &str) -> MapArrow {
    serde_json::from_value(json!({
        "id": format!("{from}->{to}"), "parent": parent, "from": from, "to": to,
        "kind": "uses", "count": 1, "kinds": ["uses"],
    }))
    .unwrap()
}

/// Random arrows between `kids` (all children of `parent`).
fn link(r: &mut Rng, parent: &str, kids: &[String], out: &mut Vec<MapArrow>) {
    if kids.len() < 2 {
        return;
    }
    for _ in 0..r.below(kids.len() * 2) {
        let (a, b) = (r.below(kids.len()), r.below(kids.len()));
        let id = format!("{}->{}", kids[a], kids[b]);
        if a != b && !out.iter().any(|x| x.id == id) {
            out.push(arrow(parent, &kids[a], &kids[b]));
        }
    }
}

fn random_tree(seed: u64) -> (Vec<MapBox>, Vec<MapArrow>) {
    let mut r = Rng(seed.wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1);
    let mut boxes = vec![{
        let mut b = mbox("system", "x", "system", "system", None, None, false);
        b.parent = None;
        b
    }];
    let mut out = vec![];
    let kinds = [
        "person", "app", "api", "store", "external", "worker", "system",
    ];
    let mut tops = vec![];
    for c in 0..1 + r.below(10) {
        let id = format!("c{c}");
        let kind = kinds[r.below(kinds.len())];
        let pattern = match r.below(5) {
            0 => Some(json!({ "kind": "onion" })),
            1 => Some(json!({ "kind": "layered", "bands": ["ui", "services", "data"] })),
            2 => Some(json!({ "kind": "hexagonal" })),
            3 => Some(json!({ "kind": "none" })),
            _ => None,
        };
        let slots: Vec<&str> = match pattern.as_ref().and_then(|p| p["kind"].as_str()) {
            Some("onion") => vec!["domain", "application", "infrastructure"],
            Some("layered") => vec!["ui", "services", "data"],
            Some("hexagonal") => vec!["core", "ports", "adapters"],
            _ => vec![],
        };
        boxes.push(mbox(&id, "system", "container", kind, pattern, None, false));
        tops.push(id.clone());
        let mut comps = vec![];
        for k in 0..r.below(13) {
            let cid = format!("{id}/k{k}");
            let slot = (!slots.is_empty() && r.below(6) > 0).then(|| slots[r.below(slots.len())]);
            let ckind = ["entry", "usecase", "port", "domain"][r.below(4)];
            boxes.push(mbox(
                &cid,
                &id,
                "component",
                ckind,
                None,
                slot,
                r.below(9) == 0,
            ));
            comps.push(cid.clone());
            let mut code = vec![];
            for f in 0..r.below(5) * r.below(4) {
                let fid = format!("{cid}/f{f}");
                boxes.push(mbox(&fid, &cid, "code", "function", None, None, false));
                code.push(fid);
            }
            link(&mut r, &cid, &code, &mut out);
        }
        if r.below(3) == 0 {
            let u = format!("{id}/unread");
            boxes.push(mbox(&u, &id, "component", "group", None, None, false));
            comps.push(u);
        }
        link(&mut r, &id, &comps, &mut out);
    }
    link(&mut r, "system", &tops, &mut out);
    (boxes, out)
}

/// The open interiors of `a` and `b` meet.
fn overlap(a: &Frame, b: &Frame) -> bool {
    a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h
}

fn inside(a: &Frame, b: &Frame) -> bool {
    a.x >= b.x && a.y >= b.y && a.x + a.w <= b.x + b.w && a.y + a.h <= b.y + b.h
}

/// The segment `p`–`q` (axis-aligned) passes through the open interior of `f`.
fn crosses(p: (f32, f32), q: (f32, f32), f: &Frame) -> bool {
    let (x0, x1) = (p.0.min(q.0), p.0.max(q.0));
    let (y0, y1) = (p.1.min(q.1), p.1.max(q.1));
    x0 < f.x + f.w && x1 > f.x && y0 < f.y + f.h && y1 > f.y
}

fn kids<'b>(boxes: &'b [MapBox], parent: &str) -> Vec<&'b MapBox> {
    boxes
        .iter()
        .filter(|b| b.parent.as_deref() == Some(parent))
        .collect()
}

fn check_no_overlap(boxes: &[MapBox], p: &Placed) {
    for b in boxes {
        let Some(parent) = b.parent.as_deref() else {
            assert!(!p.frames.contains_key(&b.id), "the root has no frame");
            continue;
        };
        let f = p
            .frames
            .get(&b.id)
            .unwrap_or_else(|| panic!("no frame for {}", b.id));
        let (w, h) = p.inner[parent];
        let space = Frame {
            x: 0.0,
            y: 0.0,
            w,
            h,
        };
        assert!(
            inside(f, &space),
            "{} {f:?} is outside {parent} {w}x{h}",
            b.id
        );
        for s in kids(boxes, parent).into_iter().filter(|s| s.id > b.id) {
            let g = &p.frames[&s.id];
            assert!(!overlap(f, g), "{} {f:?} overlaps {} {g:?}", b.id, s.id);
        }
    }
}

fn check_routes(boxes: &[MapBox], arrows: &[MapArrow], p: &Placed) {
    for a in arrows {
        let r = p
            .routes
            .get(&a.id)
            .unwrap_or_else(|| panic!("no route for {}", a.id));
        assert!(r.len() >= 2, "{}: {r:?}", a.id);
        for w in r.windows(2) {
            assert!(
                w[0].0 == w[1].0 || w[0].1 == w[1].1,
                "{}: not Manhattan {r:?}",
                a.id
            );
            for s in kids(boxes, &a.parent) {
                if s.id != a.from && s.id != a.to {
                    let f = &p.frames[&s.id];
                    assert!(
                        !crosses(w[0], w[1], f),
                        "{} crosses {} {f:?}: {r:?}",
                        a.id,
                        s.id
                    );
                }
            }
        }
        let (from, to) = (&p.frames[&a.from], &p.frames[&a.to]);
        let (s, e) = (r[0], r[r.len() - 1]);
        // within the middle part of an edge, where ports spread
        let within =
            |v: f32, lo: f32, len: f32| v >= lo + 0.2 * len - 0.01 && v <= lo + 0.8 * len + 0.01;
        let on_edge = (s.0 == from.x || s.0 == from.x + from.w) && within(s.1, from.y, from.h)
            || (s.1 == from.y || s.1 == from.y + from.h) && within(s.0, from.x, from.w);
        assert!(on_edge, "{} leaves {s:?}, not on an edge of {from:?}", a.id);
        let short = (e.0 == to.x - 1.0 || e.0 == to.x + to.w + 1.0) && within(e.1, to.y, to.h)
            || (e.1 == to.y - 1.0 || e.1 == to.y + to.h + 1.0) && within(e.0, to.x, to.w);
        assert!(short, "{} ends at {e:?}, not 1 short of {to:?}", a.id);
    }
}

fn check_rings(boxes: &[MapBox], p: &Placed) {
    let onions = boxes.iter().filter(|b| {
        b.pattern
            .as_ref()
            .is_some_and(|f| f.value == crate::map::Pattern::Onion)
    });
    // a box with no children has no inside to draw rings in
    for o in onions.filter(|o| !kids(boxes, &o.id).is_empty()) {
        let ring = |l: &str| {
            p.regions
                .iter()
                .find(|r| r.parent == o.id && r.kind == RegionKind::Ring && r.label == l)
                .unwrap_or_else(|| panic!("{}: no {l} ring", o.id))
                .frame
        };
        let rings = [ring("domain"), ring("application"), ring("infrastructure")];
        assert!(
            inside(&rings[0], &rings[1]) && rings[0] != rings[1],
            "{rings:?}"
        );
        assert!(
            inside(&rings[1], &rings[2]) && rings[1] != rings[2],
            "{rings:?}"
        );
        for k in kids(boxes, &o.id).into_iter().filter(|k| !k.stale) {
            let Some(i) = ["domain", "application", "infrastructure"]
                .iter()
                .position(|s| k.slot.as_ref().is_some_and(|f| f.value == *s))
            else {
                continue;
            };
            let f = &p.frames[&k.id];
            assert!(inside(f, &rings[i]), "{} is outside its ring", k.id);
            if i > 0 {
                assert!(
                    !overlap(f, &rings[i - 1]),
                    "{} overlaps an inner ring",
                    k.id
                );
            }
        }
    }
}

fn trees() -> Vec<(Vec<MapBox>, Vec<MapArrow>)> {
    let mut t = vec![shop()];
    t.extend((1..=12).map(random_tree));
    t
}

#[test]
fn no_overlap() {
    for (b, a) in trees() {
        let (p, notices) = layout(&b, &a);
        assert!(notices.is_empty(), "{notices:?}");
        check_no_overlap(&b, &p);
    }
}

#[test]
fn routes_avoid_boxes() {
    for (b, a) in trees() {
        check_routes(&b, &a, &layout(&b, &a).0);
    }
}

#[test]
fn onion_rings_nest() {
    let (b, a) = shop();
    let p = layout(&b, &a).0;
    assert_eq!(p.regions.iter().filter(|r| r.parent == "api").count(), 3);
    for (b, a) in trees() {
        check_rings(&b, &layout(&b, &a).0);
    }
}

#[test]
fn deterministic() {
    for (b, a) in trees() {
        let one = serde_json::to_string(&layout(&b, &a)).unwrap();
        let mut rb = b.clone();
        rb.reverse();
        let mut ra = a.clone();
        ra.reverse();
        assert_eq!(one, serde_json::to_string(&layout(&b, &a)).unwrap());
        assert_eq!(
            one,
            serde_json::to_string(&layout(&rb, &ra)).unwrap(),
            "input order"
        );
    }
}

#[test]
fn unread_is_a_chip() {
    let mut seen = 0;
    for (b, a) in trees() {
        let p = layout(&b, &a).0;
        for u in b.iter().filter(|b| b.id.ends_with("/unread")) {
            let f = p.frames[&u.id];
            assert_eq!((f.w, f.h), (120.0, 40.0));
            seen += 1;
        }
    }
    assert!(seen > 0);
}

/// Pairs of interior segments (not a route's first or last) of two arrows that lie on one line
/// and share a stretch of it.
fn shared_lanes(p: &Placed, arrows: &[&MapArrow]) -> Vec<String> {
    let inner = |a: &MapArrow| {
        let r = &p.routes[&a.id];
        let n = r.len().saturating_sub(1);
        (1..n.saturating_sub(1))
            .map(|k| (r[k], r[k + 1]))
            .collect::<Vec<_>>()
    };
    let mut out = vec![];
    for (i, a) in arrows.iter().enumerate() {
        for b in &arrows[i + 1..] {
            for (p0, p1) in inner(a) {
                for (q0, q1) in inner(b) {
                    let span = |x: f32, y: f32| (x.min(y), x.max(y));
                    let shared = if p0.1 == p1.1 && q0.1 == q1.1 && p0.1 == q0.1 {
                        Some((span(p0.0, p1.0), span(q0.0, q1.0)))
                    } else if p0.0 == p1.0 && q0.0 == q1.0 && p0.0 == q0.0 {
                        Some((span(p0.1, p1.1), span(q0.1, q1.1)))
                    } else {
                        None
                    };
                    if let Some((s, t)) = shared.filter(|(s, t)| s.0 < t.1 && t.0 < s.1) {
                        out.push(format!("{} {s:?} / {} {t:?}", a.id, b.id));
                    }
                }
            }
        }
    }
    out
}

#[test]
fn parallel_arrows_get_lanes() {
    let mut boxes = vec![mbox("c", "system", "container", "app", None, None, false)];
    for k in 0..8 {
        boxes.push(mbox(
            &format!("c/k{k}"),
            "c",
            "component",
            "entry",
            None,
            None,
            false,
        ));
    }
    // two rows of four: every arrow crosses the middle gutter
    let arrows: Vec<MapArrow> = [(0, 7), (1, 6), (2, 5), (3, 4), (0, 6), (3, 5)]
        .iter()
        .map(|(a, b)| arrow("c", &format!("c/k{a}"), &format!("c/k{b}")))
        .collect();
    let (p, notices) = layout(&boxes, &arrows);
    assert!(notices.is_empty(), "{notices:?}");
    check_routes(&boxes, &arrows, &p);
    let all: Vec<&MapArrow> = arrows.iter().collect();
    let shared = shared_lanes(&p, &all);
    assert!(shared.is_empty(), "{shared:#?}");
    for (b, a) in trees() {
        let p = layout(&b, &a).0;
        let mut by_parent: BTreeMap<&str, Vec<&MapArrow>> = BTreeMap::new();
        for x in &a {
            by_parent.entry(&x.parent).or_default().push(x);
        }
        for arrows in by_parent.values() {
            let shared = shared_lanes(&p, arrows);
            assert!(shared.is_empty(), "{shared:#?}");
        }
    }
}

#[test]
fn insides_are_wide() {
    let mut bad = vec![];
    for (b, a) in trees() {
        let p = layout(&b, &a).0;
        for (parent, (w, h)) in &p.inner {
            let n = kids(&b, parent).len();
            if n >= 3 && !(1.2..=2.4).contains(&(w / h)) {
                bad.push(format!("{parent} ({n} children): {w}x{h} = {:.2}", w / h));
            }
        }
    }
    assert!(bad.is_empty(), "{bad:#?}");
}

#[test]
fn root_keeps_its_groups() {
    let mut root = mbox("system", "x", "system", "system", None, None, false);
    root.parent = None;
    let boxes = vec![
        root,
        mbox("p", "system", "container", "person", None, None, false),
        mbox("a", "system", "container", "app", None, None, false),
        mbox("b", "system", "container", "api", None, None, false),
        mbox("x", "system", "container", "external", None, None, false),
    ];
    let p = layout(&boxes, &[]).0;
    let f = |id: &str| p.frames[id];
    // each group as a whole: the people above or left of all the containers, the outside
    // below or right of them (so no row or column mixes groups)
    let (pe, c, ex) = (f("p"), f("a").union(&f("b")), f("x"));
    assert!(
        pe.bottom() <= c.y || pe.right() <= c.x,
        "person {pe:?} / {c:?}"
    );
    assert!(
        ex.y >= c.bottom() || ex.x >= c.right(),
        "external {ex:?} / {c:?}"
    );
    let (w, h) = p.inner["system"];
    assert!((1.2..=2.4).contains(&(w / h)), "{w}x{h}");
}

#[test]
fn arrows_leave_a_box_apart_and_straight() {
    for (b, a) in trees().into_iter().chain([shop()]) {
        let p = layout(&b, &a).0;
        let mut starts: BTreeMap<(&str, [u32; 2]), &str> = BTreeMap::new();
        for x in &a {
            let r = &p.routes[&x.id];
            let f = &p.frames[&x.from];
            // the first leg runs along the edge's normal
            let (s, n) = (r[0], r[1]);
            let normal = if s.0 == f.x || s.0 == f.x + f.w {
                s.1 == n.1
            } else {
                s.0 == n.0
            };
            assert!(normal, "{} sets off along its box: {r:?}", x.id);
            let key = (x.from.as_str(), [s.0.to_bits(), s.1.to_bits()]);
            if let Some(other) = starts.insert(key, &x.id) {
                panic!("{} and {other} leave from one point {s:?}", x.id);
            }
        }
    }
}

#[test]
fn boxes_trade_places_to_untangle_arrows() {
    let mut b = vec![mbox("c", "x", "component", "usecase", None, None, false)];
    b[0].parent = None;
    for id in ["a", "b", "c1", "d", "e", "f"] {
        b.push(mbox(id, "c", "code", "function", None, None, false));
    }
    // by name a b c1 / d e f: every arrow would cross or cut through a box
    let pairs = [("a", "f"), ("b", "e"), ("c1", "d"), ("a", "e")];
    let a: Vec<MapArrow> = pairs.iter().map(|(x, y)| arrow("c", x, y)).collect();
    let p = layout(&b, &a).0;
    let mid = |id: &str| {
        let f = p.frames[id];
        (f.x + f.w / 2.0, f.y + f.h / 2.0)
    };
    for (i, (x, y)) in pairs.iter().enumerate() {
        for (u, v) in &pairs[i + 1..] {
            let shared = [x, y].contains(&u) || [x, y].contains(&v);
            assert!(
                shared || !super::crosses(mid(x), mid(y), mid(u), mid(v)),
                "{x}-{y} crosses {u}-{v}"
            );
        }
        for o in ["a", "b", "c1", "d", "e", "f"] {
            assert!(
                [x, y].contains(&&o) || !super::cuts(mid(x), mid(y), &p.frames[o]),
                "{x}-{y} cuts through {o}"
            );
        }
    }
}
