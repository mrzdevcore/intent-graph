use super::*;
use crate::map::file::{Field, MapBox, Source as By};
use crate::map::{ArrowKind, BoxKind, Level, MapArrow, Mark, MarkState};

fn bx(id: &str, parent: Option<&str>, level: Level, at: &[&str]) -> MapBox {
    MapBox {
        id: id.into(),
        parent: parent.map(Into::into),
        level,
        made_of: at.iter().map(|s| s.to_string()).collect(),
        made_of_source: By::Ig,
        package: None,
        files: 0,
        kind: Field::ig(BoxKind::Module),
        custom: None,
        props: Default::default(),
        title: Field::ig(title(id)),
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

fn title(id: &str) -> String {
    let mut c = id.chars();
    c.next()
        .map(|f| f.to_uppercase().chain(c).collect())
        .unwrap_or_default()
}

fn arrow(from: &str, to: &str) -> MapArrow {
    MapArrow {
        id: format!("{from}->{to}"),
        parent: "system".into(),
        from: from.into(),
        to: to.into(),
        kind: ArrowKind::Uses,
        label: None,
        count: 1,
        kinds: vec![ArrowKind::Uses],
        in_loop: false,
        origin: Origin::Built,
        parts: vec![],
    }
}

fn map(boxes: Vec<MapBox>, arrows: Vec<MapArrow>) -> MapFile {
    MapFile {
        version: 1,
        boxes,
        arrows,
        marks: vec![],
    }
}

fn shop() -> MapFile {
    map(
        vec![
            bx("system", None, Level::System, &["**"]),
            bx("web", Some("system"), Level::Container, &["apps/web/**"]),
            bx("db", Some("system"), Level::Container, &["apps/db/**"]),
        ],
        vec![],
    )
}

fn spec_side(m: MapFile) -> Side {
    Side {
        arch: m.clone(),
        spec: SpecSide::Read(m),
    }
}

fn run(base: &Side, head: &Side, changed: &[&str]) -> Diff {
    let changed: Vec<String> = changed.iter().map(|s| s.to_string()).collect();
    diff(base, head, &changed, "abc1234", None)
}

#[test]
fn same_sides_are_no_change() {
    let d = run(&spec_side(shop()), &spec_side(shop()), &[]);
    assert_eq!(d.summary, "no architecture change");
    assert!(matches!(d.source, Source::Spec));
    assert!(matches!(d.intent.status, IntentStatus::Same));
    assert!(d.boxes.added.is_empty() && d.boxes.removed.is_empty());
    assert!(d.boxes.changed.is_empty() && d.arrows.added.is_empty());
    assert!(d.problems.new.is_empty() && d.problems.fixed.is_empty());
    assert_eq!(d.base, "abc1234");
    assert_eq!(d.head, None);
}

#[test]
fn a_changed_file_marks_its_box_and_ancestors() {
    let m = map(
        vec![
            bx("system", None, Level::System, &["**"]),
            bx("web", Some("system"), Level::Container, &["apps/web/**"]),
            bx("ui", Some("web"), Level::Component, &["apps/web/ui/**"]),
            bx("api", Some("web"), Level::Component, &["apps/web/api/**"]),
        ],
        vec![],
    );
    let d = run(
        &spec_side(m.clone()),
        &spec_side(m),
        &["apps/web/ui/a.ts", "apps/web/api/b.ts", "README.md"],
    );
    let got: Vec<(&str, u32)> = (d.boxes.changed.iter())
        .map(|c| (c.id.as_str(), c.files_changed))
        .collect();
    assert_eq!(got, [("system", 2), ("web", 2), ("api", 1), ("ui", 1)]);
    let ui = d.boxes.changed.iter().find(|c| c.id == "ui").unwrap();
    assert_eq!(ui.files, ["apps/web/ui/a.ts"]);
    assert_eq!(ui.parent.as_deref(), Some("web"));
    let web = d.boxes.changed.iter().find(|c| c.id == "web").unwrap();
    assert_eq!(web.files, ["apps/web/api/b.ts", "apps/web/ui/a.ts"]);
    assert_eq!(d.summary, "2 boxes changed, ig/ unchanged");
}

#[test]
fn added_and_removed_boxes_and_arrows() {
    let mut base = shop();
    base.boxes
        .push(bx("old", Some("system"), Level::Container, &[]));
    base.arrows.push(arrow("web", "old"));
    base.arrows.push(arrow("old", "db"));
    let mut head = shop();
    head.boxes
        .push(bx("new", Some("system"), Level::Container, &[]));
    head.arrows.push(arrow("web", "new"));
    head.arrows.push(arrow("new", "db"));
    let d = run(&spec_side(base), &spec_side(head), &[]);
    let ids = |v: &[BoxRef]| v.iter().map(|b| b.id.clone()).collect::<Vec<_>>();
    assert_eq!(ids(&d.boxes.added), ["new"]);
    assert_eq!(ids(&d.boxes.removed), ["old"]);
    assert_eq!(d.boxes.added[0].title, "New");
    let ends = |v: &[ArrowRef]| {
        (v.iter())
            .map(|a| format!("{}>{}:{}", a.from, a.to, a.kind))
            .collect::<Vec<_>>()
    };
    assert_eq!(ends(&d.arrows.added), ["new>db:uses", "web>new:uses"]);
    assert_eq!(ends(&d.arrows.removed), ["old>db:uses", "web>old:uses"]);
    assert!(matches!(d.intent.status, IntentStatus::Changed));
    assert_eq!(ids(&d.intent.added), ["new"]);
    assert_eq!(
        ends(&d.intent.arrows_added),
        ["new>db:uses", "web>new:uses"]
    );
    assert_eq!(
        d.summary,
        "1 box added, 1 box removed, 2 new arrows, 2 arrows removed, ig/ changed"
    );
}

#[test]
fn one_new_arrow_names_it() {
    let mut head = shop();
    head.arrows.push(arrow("web", "db"));
    let d = run(&spec_side(shop()), &spec_side(head), &[]);
    assert_eq!(d.summary, "1 new arrow Web → Db, ig/ changed");
}

#[test]
fn ig_added_is_not_churn() {
    let base = Side {
        arch: shop(),
        spec: SpecSide::Missing,
    };
    let head = Side {
        arch: shop(),
        spec: SpecSide::Read(shop()),
    };
    let d = run(&base, &head, &[]);
    assert!(matches!(d.source, Source::Code));
    assert!(d.boxes.added.is_empty() && d.boxes.removed.is_empty());
    assert!(matches!(d.intent.status, IntentStatus::Added));
    assert_eq!(d.intent.note.as_deref(), Some("ig/ added (3 boxes)"));
    assert_eq!(d.summary, "ig/ added (3 boxes)");
}

#[test]
fn ig_removed_and_none_on_either_side() {
    let base = Side {
        arch: shop(),
        spec: SpecSide::Read(shop()),
    };
    let head = Side {
        arch: shop(),
        spec: SpecSide::Missing,
    };
    let d = run(&base, &head, &[]);
    assert!(matches!(d.intent.status, IntentStatus::Removed));
    assert_eq!(d.summary, "ig/ removed");
    let none = Side {
        arch: shop(),
        spec: SpecSide::Missing,
    };
    let d = run(&none, &none, &["apps/db/x.ts"]);
    assert!(matches!(d.intent.status, IntentStatus::Same));
    assert_eq!(d.summary, "1 box changed");
}

#[test]
fn base_ig_broken_is_noted() {
    let mut drifting = shop();
    drifting.boxes.push(drift("lib-drift", &["lib/**"]));
    let base = Side {
        arch: shop(),
        spec: SpecSide::Broken("ig/web.kdl:3: unknown node `tecc`".into()),
    };
    let head = Side {
        arch: drifting.clone(),
        spec: SpecSide::Read(drifting),
    };
    let d = run(&base, &head, &[]);
    assert!(matches!(d.source, Source::Code));
    assert!(matches!(d.intent.status, IntentStatus::BaseBroken));
    let note = d.intent.note.as_deref().unwrap();
    assert!(note.starts_with("base ig/ does not read: "), "{note}");
    assert!(note.ends_with("unknown node `tecc`"), "{note}");
    assert!(d.problems.new.is_empty() && d.problems.fixed.is_empty());
}

#[test]
fn intent_fields_and_moves() {
    let base = shop();
    let mut head = shop();
    let web = head.boxes.iter_mut().find(|b| b.id == "web").unwrap();
    web.parent = Some("db".into());
    web.tech = Some(Field::ig("Next.js".into()));
    let db = head.boxes.iter_mut().find(|b| b.id == "db").unwrap();
    db.made_of = vec!["db/**".into()];
    db.props.insert("runs".into(), "postgres".into());
    db.title = Field::agent("Database".into(), None);
    let d = run(&spec_side(base), &spec_side(head), &[]);
    assert!(matches!(d.intent.status, IntentStatus::Changed));
    assert_eq!(d.intent.moved.len(), 1);
    let m = &d.intent.moved[0];
    assert_eq!(
        (m.id.as_str(), m.from.as_deref(), m.to.as_deref()),
        ("web", Some("system"), Some("db"))
    );
    let fields: Vec<(&str, Vec<&str>)> = (d.intent.changed.iter())
        .map(|c| (c.id.as_str(), c.fields.iter().map(String::as_str).collect()))
        .collect();
    assert_eq!(
        fields,
        [("db", vec!["title", "at", "runs"]), ("web", vec!["tech"])]
    );
}

#[test]
fn a_field_whose_source_alone_changed_is_no_change() {
    let mut head = shop();
    head.boxes[1].title = Field::person("Web".into());
    let d = run(&spec_side(shop()), &spec_side(head), &[]);
    assert_eq!(d.summary, "no architecture change");
}

fn drift(id: &str, at: &[&str]) -> MapBox {
    let mut b = bx(id, Some("system"), Level::Container, at);
    b.origin = Origin::Drift;
    b
}

fn with_arch(spec: MapFile, arch: MapFile) -> Side {
    Side {
        arch,
        spec: SpecSide::Read(spec),
    }
}

#[test]
fn drift_matched_by_globs_not_id() {
    let mut base = shop();
    base.boxes
        .push(drift("lib-drift", &["lib/b/**", "lib/a/**"]));
    base.boxes.push(drift("tools-drift", &["tools/**"]));
    base.arrows.push(arrow("web", "lib-drift"));
    let mut head = shop();
    head.boxes
        .push(drift("lib-drift-2", &["lib/a/**", "lib/b/**"]));
    head.boxes.push(drift("lib-drift", &["scripts/**"]));
    head.arrows.push(arrow("web", "lib-drift-2"));
    let d = run(&with_arch(shop(), base), &with_arch(shop(), head), &[]);
    let texts = |v: &[Problem]| v.iter().map(|p| p.text.clone()).collect::<Vec<_>>();
    assert_eq!(
        texts(&d.problems.new),
        ["code under scripts/** is in no box"]
    );
    assert_eq!(d.problems.new[0].boxes, ["lib-drift"]);
    assert!(matches!(d.problems.new[0].kind, ProblemKind::Drift));
    assert_eq!(
        texts(&d.problems.fixed),
        ["code under tools/** is in no box"]
    );
    assert!(d.arrows.added.is_empty() && d.arrows.removed.is_empty());
    assert!(d.boxes.added.is_empty() && d.boxes.removed.is_empty());
    assert_eq!(d.summary, "ig/ unchanged, drift +1, 1 problem fixed");
}

fn rule(target: &str, badge: &str) -> Mark {
    Mark {
        layer: "rules".into(),
        target: target.into(),
        state: MarkState::Bad,
        badge: Some(badge.into()),
        value: None,
        note: None,
    }
}

#[test]
fn new_and_fixed_rule_breaks() {
    let mut base = shop();
    base.boxes
        .push(bx("cli", Some("system"), Level::Container, &[]));
    base.arrows = vec![arrow("web", "db"), arrow("cli", "db")];
    let mut head = base.clone();
    base.marks = vec![rule("cli->db", "no-db-from-cli")];
    head.arrows = vec![arrow("web", "db"), arrow("cli", "db"), arrow("db", "web")];
    head.marks = vec![
        rule("web->db", "web-not-db"),
        rule("db->web", "web-not-db"),
        Mark {
            state: MarkState::Ok,
            ..rule("cli->db", "no-db-from-cli")
        },
    ];
    let spec = map(base.boxes.clone(), vec![]);
    let d = run(&with_arch(spec.clone(), base), &with_arch(spec, head), &[]);
    let got = |v: &[Problem]| {
        (v.iter())
            .map(|p| format!("{} {}", p.text, p.boxes.join(">")))
            .collect::<Vec<_>>()
    };
    assert_eq!(
        got(&d.problems.new),
        ["web-not-db db>web", "web-not-db web>db"]
    );
    assert!(matches!(d.problems.new[0].kind, ProblemKind::Rule));
    assert_eq!(got(&d.problems.fixed), ["no-db-from-cli cli>db"]);
    assert_eq!(
        d.summary,
        "1 new arrow Db → Web, ig/ unchanged, 2 new rule breaks, 1 problem fixed"
    );
}

#[test]
fn json_shape_is_camel_case() {
    let mut head = shop();
    head.arrows.push(arrow("web", "db"));
    let d = run(&spec_side(shop()), &spec_side(head), &["apps/web/a.ts"]);
    let j = serde_json::to_value(&d).unwrap();
    assert_eq!(j["source"], "spec");
    assert_eq!(j["head"], serde_json::Value::Null);
    assert_eq!(j["boxes"]["changed"][0]["filesChanged"], 1);
    assert_eq!(j["intent"]["status"], "changed");
    assert_eq!(j["intent"]["arrowsAdded"][0]["kind"], "uses");
    assert!(j["intent"]["arrowsRemoved"].as_array().unwrap().is_empty());
    assert!(j["intent"].get("note").is_none());
    assert!(j["problems"]["new"].as_array().unwrap().is_empty());
    let broken = Side {
        arch: shop(),
        spec: SpecSide::Broken("x".into()),
    };
    let j = serde_json::to_value(run(&broken, &spec_side(shop()), &[])).unwrap();
    assert_eq!(j["intent"]["status"], "baseBroken");
    assert_eq!(j["source"], "code");
}

#[test]
fn an_added_box_is_not_also_changed_and_an_old_at_still_counts() {
    let mut head = shop();
    head.boxes
        .push(bx("ui", Some("web"), Level::Component, &["apps/web/ui/**"]));
    let db = head.boxes.iter_mut().find(|b| b.id == "db").unwrap();
    db.made_of = vec!["db/**".into()];
    let d = run(
        &spec_side(shop()),
        &spec_side(head),
        &["apps/web/ui/a.ts", "apps/db/gone.ts"],
    );
    let got: Vec<(&str, u32)> = (d.boxes.changed.iter())
        .map(|c| (c.id.as_str(), c.files_changed))
        .collect();
    assert_eq!(got, [("system", 2), ("db", 1), ("web", 1)]);
    assert_eq!(d.summary, "1 box changed, 1 box added, ig/ changed");
}

mod runs {
    use super::super::build::run;
    use super::*;
    use crate::map::view::architecture;
    use crate::testing::{Fake, env};
    use ig_core::testing::{FakeVcs, MemFiles};
    use std::cell::RefCell;
    use std::path::{Path, PathBuf};

    const CODE: &[(&str, &str)] = &[("src/a.ts", "export function f() {}\n")];

    fn vcs(root: Option<&str>) -> FakeVcs {
        FakeVcs {
            commits: ["b".to_string(), "h".to_string()].into(),
            checkout_root: root.map(PathBuf::from),
            changed_between: vec!["src/a.ts".into()],
            ..FakeVcs::default()
        }
    }

    #[test]
    fn run_uses_code_ids_when_one_side_lacks_ig() {
        let files = MemFiles::with(CODE);
        let v = vcs(Some("wt"));
        let e = env(&files, Some(&v), &Fake);
        let calls = RefCell::new(vec![]);
        let at = |root: &Path, use_spec: bool| {
            calls.borrow_mut().push((root.to_path_buf(), use_spec));
            let spec = map(vec![bx("spec-only", None, Level::System, &["**"])], vec![]);
            let arch = match use_spec {
                true => spec.clone(),
                false => architecture(&env(&MemFiles::with(CODE), None, &Fake), false)?.arch,
            };
            Ok(Side {
                arch,
                spec: SpecSide::Read(spec),
            })
        };
        let d = run(&e, "b", None, &at).unwrap();
        assert_eq!(*calls.borrow(), [("wt".into(), true), ("wt".into(), false)]);
        assert!(matches!(d.source, Source::Code));
        assert!(matches!(d.intent.status, IntentStatus::Removed));
        assert!(d.boxes.added.is_empty(), "{:?}", d.boxes.added);
        assert!(d.boxes.removed.is_empty(), "{:?}", d.boxes.removed);
        assert!(!d.boxes.changed.is_empty(), "src/a.ts changed");
        assert_eq!((d.base.as_str(), d.head.as_deref()), ("b", None));
    }

    #[test]
    fn run_builds_an_empty_base_when_the_root_did_not_exist() {
        let files = MemFiles::with(CODE);
        let v = vcs(None);
        let e = env(&files, Some(&v), &Fake);
        let at = |_: &Path, _: bool| -> crate::Result<Side> { panic!("no checkout root") };
        let d = run(&e, "b", None, &at).unwrap();
        assert!(!d.boxes.added.is_empty());
        assert!(matches!(d.intent.status, IntentStatus::Same));
    }

    #[test]
    fn run_rejects_a_rev_that_is_not_a_commit() {
        let files = MemFiles::with(CODE);
        let v = vcs(Some("wt"));
        let e = env(&files, Some(&v), &Fake);
        let at = |_: &Path, _: bool| -> crate::Result<Side> { panic!("never built") };
        let err =
            |base: &str, head: Option<&str>| run(&e, base, head, &at).unwrap_err().to_string();
        assert_eq!(err("nope", None), "nope: not a commit");
        assert_eq!(err("b", Some("nope")), "nope: not a commit");

        let files = MemFiles::with(&[CODE[0], ("ig/system.kdl", "system {")]);
        let e = env(&files, Some(&v), &Fake);
        let at = |_: &Path, _: bool| {
            Ok(Side {
                arch: map(vec![], vec![]),
                spec: SpecSide::Missing,
            })
        };
        let msg = run(&e, "b", None, &at).unwrap_err().to_string();
        assert!(
            msg.starts_with("ig/ does not read: ig/system.kdl:"),
            "{msg}"
        );
    }
}
