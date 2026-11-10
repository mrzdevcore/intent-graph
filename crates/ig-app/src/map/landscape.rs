//! A folder of projects as one map: a box per project, each holding its own map, those that run the
//! others on a band below them, and the links their files suggest as proposals.

use super::file::{Field, MapBox, Source};
use super::kinds::Kinds;
use super::layers::Mark;
use super::layout::{Frame, Placed, Region, RegionKind};
use super::origin::Origin;
use super::{ArrowKind, BoxKind, Level, MapArrow, View};
use serde::Serialize;

const ROOT: &str = "system";
const W: f32 = 320.0;
const H: f32 = 190.0;
const GAP: f32 = 96.0;
const PAD: f32 = 32.0;

pub struct Project {
    pub dir: String,
    pub why: String,
    pub ground: bool,
    /// Its own map; none until it was read.
    pub view: Option<View>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Link {
    pub from: String,
    pub to: String,
    pub files: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Skipped {
    pub dir: String,
    pub why: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Landscape {
    pub projects: Vec<Skipped>,
    pub unsure: Vec<Skipped>,
    pub left_out: Vec<Skipped>,
    pub links: Vec<Link>,
}

fn plain(id: &str, parent: Option<&str>, kind: BoxKind, title: &str) -> MapBox {
    MapBox {
        id: id.into(),
        parent: parent.map(Into::into),
        level: Level::System,
        made_of: vec![],
        made_of_source: Source::Ig,
        package: None,
        files: 0,
        kind: Field::ig(kind),
        custom: None,
        props: Default::default(),
        title: Field::ig(title.into()),
        tech: None,
        description: None,
        logo: None,
        badges: None,
        pattern: None,
        slot: None,
        service_name: None,
        pinned: vec![],
        needs_reading: false,
        stale: false,
        origin: Origin::Built,
        moved_from: vec![],
        nodes: vec![],
    }
}

fn moved(id: &str, dir: &str) -> String {
    if id == ROOT {
        dir.to_string()
    } else {
        format!("{dir}/{id}")
    }
}

fn moved_arrow(a: MapArrow, dir: &str) -> MapArrow {
    let (from, to) = (moved(&a.from, dir), moved(&a.to, dir));
    MapArrow {
        id: format!("{from}->{to}"),
        parent: moved(&a.parent, dir),
        from,
        to,
        parts: a.parts.into_iter().map(|p| moved_arrow(p, dir)).collect(),
        ..a
    }
}

// a mark's target is a box, or an arrow by its `from->to` id
fn moved_mark(m: Mark, dir: &str) -> Mark {
    let target = match m.target.split_once("->") {
        Some((f, t)) => format!("{}->{}", moved(f, dir), moved(t, dir)),
        None => moved(&m.target, dir),
    };
    Mark { target, ..m }
}

fn placed_row(
    ids: &[&Project],
    y: f32,
    x: f32,
    frames: &mut std::collections::BTreeMap<String, Frame>,
) {
    for (i, p) in ids.iter().enumerate() {
        frames.insert(
            p.dir.clone(),
            Frame {
                x: x + i as f32 * (W + GAP),
                y,
                w: W,
                h: H,
            },
        );
    }
}

fn width(n: usize) -> f32 {
    n as f32 * W + n.saturating_sub(1) as f32 * GAP
}

pub fn combine(title: &str, projects: Vec<Project>, mut info: Landscape) -> View {
    let (grounds, tops): (Vec<&Project>, Vec<&Project>) = projects
        .iter()
        .partition(|p| p.ground && projects.iter().any(|q| !q.ground));
    // wider than square: a screen's shape, with the band below
    let cols = ((tops.len() as f32) * 1.2).sqrt().ceil().max(1.0) as usize;
    let mut frames = Default::default();
    let mut y = GAP;
    let rows: Vec<&[&Project]> = tops.chunks(cols).collect();
    for row in &rows {
        placed_row(row, y, GAP, &mut frames);
        y += H + GAP;
    }
    let mut regions = vec![];
    let mut right = width(cols);
    if !grounds.is_empty() {
        placed_row(&grounds, y + PAD * 1.5, GAP + PAD, &mut frames);
        let band = width(grounds.len()) + 2.0 * PAD;
        right = right.max(band);
        regions.push(Region {
            parent: ROOT.into(),
            kind: RegionKind::Band,
            label: "Where it runs".into(),
            frame: Frame {
                x: GAP,
                y,
                w: right,
                h: H + PAD * 2.5,
            },
        });
        y += H + PAD * 2.5 + GAP;
    }
    let mut placed = Placed {
        frames,
        inner: Default::default(),
        regions,
    };
    placed.inner.insert(ROOT.into(), (right + 2.0 * GAP, y));

    let mut view = View {
        version: 1,
        boxes: vec![plain(ROOT, None, BoxKind::System, title)],
        arrows: vec![],
        marks: vec![],
        placed,
        sources: Default::default(),
        parts: Default::default(),
        tables: Default::default(),
        notices: vec![],
        warnings: vec![],
        kinds: Kinds::builtin().views(),
        proposals: vec![],
        read: Default::default(),
        broken: false,
        landscape: None,
    };
    info.projects = projects
        .iter()
        .map(|p| Skipped {
            dir: p.dir.clone(),
            why: p.why.clone(),
        })
        .collect();
    for p in projects {
        let mut own = plain(&p.dir, Some(ROOT), BoxKind::System, &p.dir);
        own.made_of = vec![format!("{}/**", p.dir)];
        own.needs_reading = p.view.is_none();
        if let Some(v) = p.view {
            if let Some(root) = v.boxes.iter().find(|b| b.id == ROOT) {
                own.title = root.title.clone();
                own.tech = root.tech.clone();
                own.description = root.description.clone();
                own.logo = root.logo.clone();
            }
            let dir = p.dir.as_str();
            view.boxes.extend(
                v.boxes
                    .into_iter()
                    .filter(|b| b.id != ROOT)
                    .map(|b| MapBox {
                        id: moved(&b.id, dir),
                        parent: Some(moved(b.parent.as_deref().unwrap_or(ROOT), dir)),
                        made_of: b.made_of.iter().map(|m| format!("{dir}/{m}")).collect(),
                        ..b
                    }),
            );
            view.arrows
                .extend(v.arrows.into_iter().map(|a| moved_arrow(a, dir)));
            view.marks
                .extend(v.marks.into_iter().map(|m| moved_mark(m, dir)));
            view.placed.frames.extend(
                v.placed
                    .frames
                    .into_iter()
                    .map(|(k, f)| (moved(&k, dir), f)),
            );
            view.placed
                .inner
                .extend(v.placed.inner.into_iter().map(|(k, s)| (moved(&k, dir), s)));
            view.placed
                .regions
                .extend(v.placed.regions.into_iter().map(|r| Region {
                    parent: moved(&r.parent, dir),
                    ..r
                }));
            view.sources
                .extend(v.sources.into_iter().map(|(k, s)| (moved(&k, dir), s)));
            view.parts
                .extend(v.parts.into_iter().map(|(k, s)| (moved(&k, dir), s)));
            view.notices
                .extend(v.notices.into_iter().map(|n| format!("{dir}: {n}")));
            view.warnings.extend(v.warnings);
            for k in v.kinds {
                if !view.kinds.iter().any(|x| x.name == k.name) {
                    view.kinds.push(k);
                }
            }
        }
        view.boxes.push(own);
    }
    view.arrows.extend(info.links.iter().map(|l| MapArrow {
        id: format!("{}->{}", l.from, l.to),
        parent: ROOT.into(),
        from: l.from.clone(),
        to: l.to.clone(),
        kind: ArrowKind::Uses,
        label: Some(format!(
            "{} {}",
            l.files.len(),
            if l.files.len() == 1 { "file" } else { "files" }
        )),
        count: l.files.len() as u32,
        kinds: vec![ArrowKind::Uses],
        in_loop: false,
        origin: Origin::Build,
        parts: vec![],
    }));
    view.landscape = Some(info);
    view
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inner() -> View {
        let mut v = view_of();
        v.boxes.iter_mut().find(|b| b.id == ROOT).unwrap().title = Field::ig("Shop".to_string());
        v
    }
    fn view_of() -> View {
        let mut root = plain(ROOT, None, BoxKind::System, "x");
        root.tech = Some(Field::ig("Rust".to_string()));
        let mut api = plain("api", Some(ROOT), BoxKind::Service, "API");
        api.made_of = vec!["api/**".into()];
        let db = plain("api/db", Some("api"), BoxKind::Store, "DB");
        let arrow = |from: &str, to: &str, parts: Vec<MapArrow>| MapArrow {
            id: format!("{from}->{to}"),
            parent: ROOT.into(),
            from: from.into(),
            to: to.into(),
            kind: ArrowKind::Calls,
            label: None,
            count: 1,
            kinds: vec![],
            in_loop: false,
            origin: Origin::Built,
            parts,
        };
        let mut placed = Placed::default();
        placed.frames.insert(
            "api".into(),
            Frame {
                x: 1.0,
                y: 2.0,
                w: 3.0,
                h: 4.0,
            },
        );
        placed.inner.insert(ROOT.into(), (10.0, 20.0));
        placed.regions.push(Region {
            parent: "api".into(),
            kind: RegionKind::Ring,
            label: "r".into(),
            frame: Frame {
                x: 0.0,
                y: 0.0,
                w: 1.0,
                h: 1.0,
            },
        });
        View {
            version: 1,
            boxes: vec![root, api, db],
            arrows: vec![arrow("api", "api/db", vec![arrow("api", "api/db", vec![])])],
            marks: vec![Mark {
                layer: "rules".into(),
                target: "api->api/db".into(),
                state: crate::map::MarkState::Bad,
                badge: None,
                value: None,
                note: None,
            }],
            placed,
            sources: Default::default(),
            parts: Default::default(),
            tables: Default::default(),
            notices: vec!["hello".into()],
            warnings: vec![],
            kinds: vec![],
            proposals: vec![],
            read: Default::default(),
            broken: false,
            landscape: None,
        }
    }
    fn project(dir: &str, ground: bool, view: Option<View>) -> Project {
        Project {
            dir: dir.into(),
            why: "Git repo".into(),
            ground,
            view,
        }
    }

    #[test]
    fn a_project_s_map_sits_inside_its_box_under_its_own_ids() {
        let v = combine(
            "acme",
            vec![project("shop", false, Some(inner()))],
            Landscape::default(),
        );
        let b = |id: &str| v.boxes.iter().find(|b| b.id == id).unwrap();
        assert_eq!(b(ROOT).title.value, "acme");
        assert_eq!(b("shop").title.value, "Shop");
        assert_eq!(b("shop").tech.as_ref().unwrap().value, "Rust");
        assert!(!b("shop").needs_reading);
        assert_eq!(b("shop/api").parent.as_deref(), Some("shop"));
        assert_eq!(b("shop/api/db").parent.as_deref(), Some("shop/api"));
        assert_eq!(b("shop/api").made_of, ["shop/api/**"]);
        assert_eq!(v.boxes.iter().filter(|x| x.id == ROOT).count(), 1);
        let a = &v.arrows[0];
        assert_eq!(
            (a.id.as_str(), a.parent.as_str()),
            ("shop/api->shop/api/db", "shop")
        );
        assert_eq!(a.parts[0].from, "shop/api");
        assert_eq!(v.marks[0].target, "shop/api->shop/api/db");
        assert_eq!(v.placed.inner["shop"], (10.0, 20.0));
        assert_eq!(v.placed.frames["shop/api"].x, 1.0);
        assert_eq!(v.placed.regions[0].parent, "shop/api");
        assert_eq!(v.notices, ["shop: hello"]);
    }

    #[test]
    fn an_unread_project_is_an_empty_box_that_says_so() {
        let v = combine("f", vec![project("a", false, None)], Landscape::default());
        let a = v.boxes.iter().find(|b| b.id == "a").unwrap();
        assert!(a.needs_reading);
        assert_eq!(a.title.value, "a");
        assert_eq!(a.made_of, ["a/**"]);
    }

    #[test]
    fn what_runs_the_others_lies_on_a_band_below_them() {
        let v = combine(
            "f",
            vec![
                project("infra", true, None),
                project("a", false, None),
                project("b", false, None),
                project("vpc", true, None),
            ],
            Landscape::default(),
        );
        let f = |id: &str| v.placed.frames[id];
        let band = v.placed.regions.iter().find(|r| r.parent == ROOT).unwrap();
        assert_eq!(band.label, "Where it runs");
        assert!(f("infra").y > f("a").y + f("a").h && f("infra").y >= band.frame.y);
        assert!(f("vpc").x + f("vpc").w <= band.frame.x + band.frame.w);
        assert!(f("infra").y + f("infra").h <= band.frame.y + band.frame.h);
        let (w, h) = v.placed.inner[ROOT];
        assert!(band.frame.x + band.frame.w < w && band.frame.y + band.frame.h < h);
    }

    #[test]
    fn with_nothing_above_it_there_is_no_band() {
        let v = combine(
            "f",
            vec![project("infra", true, None), project("vpc", true, None)],
            Landscape::default(),
        );
        assert!(v.placed.regions.is_empty());
        assert_eq!(v.placed.frames.len(), 2);
    }

    #[test]
    fn links_are_proposals_that_say_how_many_files_suggest_them() {
        let info = Landscape {
            links: vec![Link {
                from: "a".into(),
                to: "b".into(),
                files: vec!["x".into(), "y".into()],
            }],
            ..Default::default()
        };
        let v = combine(
            "f",
            vec![project("a", false, None), project("b", false, None)],
            info,
        );
        let a = &v.arrows[0];
        assert_eq!(
            (a.id.as_str(), a.parent.as_str(), a.label.as_deref()),
            ("a->b", ROOT, Some("2 files"))
        );
        assert_eq!(a.origin, Origin::Build);
        let l = v.landscape.unwrap();
        assert_eq!(l.links[0].files, ["x", "y"]);
        assert_eq!(
            l.projects
                .iter()
                .map(|p| p.dir.as_str())
                .collect::<Vec<_>>(),
            ["a", "b"]
        );
    }
}
