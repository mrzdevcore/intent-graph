use super::super::origin::Origin;
use super::*;
use crate::testing::{Fake, env};
use ig_core::testing::MemFiles;

#[test]
fn notices_reach_the_view_and_a_rejected_map_is_kept() {
    let files = MemFiles::with(&[
        ("ig.yaml", "complexityMax: 9\n"),
        ("src/a.ts", "export function f() {}\n"),
        (file::MAP, "{ not json"),
    ]);
    let fe = Fake;
    let v = run(&env(&files, None, &fe)).unwrap();
    assert!(v.notices[0].contains("complexityMax"), "{:?}", v.notices);
    assert!(
        v.notices.iter().any(|n| n.contains("left as is")),
        "{:?}",
        v.notices
    );
    assert_eq!(files.read(file::MAP).as_deref(), Some("{ not json"));
    assert!(
        files.read("ig/system.kdl").is_none(),
        "its reading is not lost"
    );
    assert!(v.broken, "no spec, none written");
}

#[test]
fn the_first_run_writes_ig_and_later_runs_never_rewrite_it() {
    let files = MemFiles::with(&[("src/a.ts", "export function f() {}\n")]);
    let v = run(&env(&files, None, &Fake)).unwrap();
    assert!(
        v.notices.iter().any(|n| n.starts_with("wrote ig/")),
        "{:?}",
        v.notices
    );
    // a root package is written with its own folders, not `**`
    let pkg = files.read("ig/pkg.kdl").unwrap();
    assert!(pkg.contains("at \"src/**\"\n"), "{pkg}");
    let before = files.read("ig/system.kdl").unwrap();
    files
        .write_atomic("lib/c.ts", b"export const y = 2;\n")
        .unwrap(); // outside every `at`: drift
    let v = run(&env(&files, None, &Fake)).unwrap();
    assert_eq!(files.read("ig/system.kdl").unwrap(), before);
    assert_eq!(files.read("ig/pkg.kdl").unwrap(), pkg);
    assert!(v.boxes.iter().any(|b| b.origin == Origin::Drift));
    assert!(!v.notices.iter().any(|n| n.starts_with("wrote ig/")));
    assert!(v.boxes.iter().all(|b| !b.needs_reading));
    assert!(
        files.read(file::MAP).is_none(),
        "the map file is never written"
    );
}

#[test]
fn a_map_json_is_migrated_with_its_readings() {
    let files = MemFiles::with(&[("src/a.ts", "export function f() {}\n")]);
    let e = env(&files, None, &Fake);
    let cfg = crate::config::load(&files).unwrap();
    let built = crate::graph(&e, &files, &cfg, None).unwrap();
    let b = super::super::facts(&built.workspace, &built.graph, &cfg);
    let mut m = refresh(None, &b, arrows(&b, &built.graph), &[]).0;
    let sys = (m.boxes.iter_mut())
        .find(|b| b.level == super::super::Level::System)
        .unwrap();
    sys.title = Field::person("Core".into());
    sys.pinned.push("title".into());
    file::save(&files, &m).unwrap();
    let map = files.read(file::MAP).unwrap();
    let v = run(&e).unwrap();
    assert!(
        files
            .read("ig/system.kdl")
            .unwrap()
            .contains("title=\"Core\"")
    );
    assert!(
        v.notices.iter().any(|n| n.contains("from .ig/map.json")),
        "{:?}",
        v.notices
    );
    assert_eq!(files.read(file::MAP).unwrap(), map, "left where it is");
}

#[test]
fn a_broken_spec_is_reported_and_kept() {
    let files = MemFiles::with(&[
        ("src/a.ts", "export function f() {}\n"),
        ("ig/system.kdl", "system {"),
    ]);
    let v = run(&env(&files, None, &Fake)).unwrap();
    assert!(
        v.notices.iter().any(|n| n.starts_with("ig/system.kdl:")),
        "{:?}",
        v.notices
    );
    assert!(v.notices.iter().any(|n| n == "ig/ left as is; fix it"));
    assert!(!v.boxes.is_empty(), "the code is still drawn");
    assert_eq!(files.read("ig/system.kdl").as_deref(), Some("system {"));
}

#[test]
fn a_function_box_carries_its_code() {
    let (ws, g, cfg) = crate::map::shop::load();
    let b = super::super::facts(&ws, &g, &cfg);
    let m = refresh(None, &b, arrows(&b, &g), &[]).0;
    let f = g.nodes.iter().find(|n| n.kind == "function").unwrap();
    let (a, z) = (f.range.start_line, f.range.end_line);
    let src: String = (1..=z + 2).map(|i| format!("line {i}\n")).collect();
    let v = view(
        &MemFiles::with(&[(f.file.as_str(), src.as_str())]),
        &m,
        &g,
        &[],
    );
    let s = &v.sources[&f.id];
    assert_eq!((s.file.as_str(), s.line), (f.file.as_str(), a));
    let want: Vec<String> = (a..=z).map(|i| format!("line {i}")).collect();
    assert_eq!(s.text, want.join("\n"));
    assert!(
        v.sources.values().all(|s| s.file == f.file),
        "unreadable files have none"
    );
}

#[test]
fn arrows_end_at_the_components_an_agent_drew() {
    let (ws, g, cfg) = crate::map::shop::load();
    let b = super::super::facts(&ws, &g, &cfg);
    let mut old = refresh(None, &b, arrows(&b, &g), &[]).0;
    // an agent renamed every component: same files, its own ids
    for x in old
        .boxes
        .iter_mut()
        .filter(|x| x.level == super::super::Level::Component)
    {
        x.id = format!("{}-read", x.id);
        x.made_of_source = file::Source::Agent;
    }
    let (mut m, _) = refresh(Some(&old), &b, arrows(&b, &g), &[]);
    relift(&mut m, &g, &[]);
    let ids: HashSet<&str> = m.boxes.iter().map(|x| x.id.as_str()).collect();
    assert!(m.arrows.iter().any(|a| a.from.ends_with("-read")));
    assert!(
        (m.arrows.iter()).all(|a| ids.contains(a.from.as_str()) && ids.contains(a.to.as_str())),
        "{:?}",
        m.arrows
    );
}

#[test]
fn an_invalid_map_is_never_written() {
    let files = MemFiles::with(&[]);
    let mut m = MapFile {
        version: 2,
        boxes: vec![],
        arrows: vec![],
        marks: vec![],
    };
    let (n, wrote) = write_spec(&files, &m, "the code").unwrap();
    assert!(n.contains("not written") && n.contains("version 2"), "{n}");
    assert!(!wrote);
    assert!(files.glob(&["ig/*".into()]).unwrap().is_empty());
    let (ws, g, cfg) = crate::map::shop::load();
    let b = super::super::facts(&ws, &g, &cfg);
    m = refresh(None, &b, arrows(&b, &g), &[]).0;
    let (n, wrote) = write_spec(&files, &m, "the code").unwrap();
    assert!(n.starts_with("wrote ig/") && wrote, "{n}");
}

#[test]
fn a_spec_ig_would_reject_is_never_written() {
    let (ws, g, cfg) = crate::map::shop::load();
    let b = super::super::facts(&ws, &g, &cfg);
    let mut m = refresh(None, &b, arrows(&b, &g), &[]).0;
    // an arrow to no box passes the map's checks but not the reader's
    let mut a = m.arrows[0].clone();
    (a.id, a.to) = (format!("{}->nowhere", a.from), "nowhere".into());
    m.arrows.push(a);
    assert!(file::validate(&m).is_ok());
    let files = MemFiles::with(&[]);
    let (n, wrote) = write_spec(&files, &m, "the code").unwrap();
    assert!(n.starts_with("ig/ not written") && !wrote, "{n}");
    assert!(files.glob(&["ig/*".into()]).unwrap().is_empty());
}

#[test]
fn the_view_carries_the_kinds_and_its_own_warnings() {
    let files = MemFiles::with(&[("src/a.ts", "export function f() {}\n")]);
    run(&env(&files, None, &Fake)).unwrap();
    let sys = files.read("ig/system.kdl").unwrap();
    let sys = sys.replacen("{\n", "{\n  usecase \"lonely\" title=\"Lonely\"\n", 1);
    files.write_atomic("ig/system.kdl", sys.as_bytes()).unwrap();
    files
        .write_atomic(
            "ig/kinds/iot-device.kdl",
            b"kind \"iot-device\" is=\"service\" title=\"IoT device\" means=\"A device in the field\" { property \"firmware\" required=#true }\n",
        )
        .unwrap();
    let v = run(&env(&files, None, &Fake)).unwrap();
    let iot = v.kinds.iter().find(|k| k.name == "iot-device").unwrap();
    assert_eq!((iot.base.as_str(), iot.role), ("service", "run"));
    assert!(iot.custom && iot.props.iter().any(|p| p.name == "firmware" && p.required));
    let model = v.kinds.iter().find(|k| k.name == "model").unwrap();
    let hosted = model.props.iter().find(|p| p.name == "hosted").unwrap();
    assert_eq!(hosted.one_of, ["local", "hosted"]);
    assert!(
        v.kinds.iter().all(|k| k.name != "file"),
        "no code-level kinds"
    );
    assert_eq!(
        v.warnings,
        ["`lonely`: a usecase sits inside a running block"]
    );
    assert!(
        v.notices.iter().all(|n| !n.contains("lonely")),
        "{:?}",
        v.notices
    );
    let json = serde_json::to_value(&v).unwrap();
    assert_eq!(json["kinds"][0]["name"], "system");
    assert!(
        json["kinds"]
            .as_array()
            .unwrap()
            .iter()
            .any(|k| k["props"][0]["oneOf"].is_array())
    );
    assert_eq!(json["warnings"][0], v.warnings[0]);
}

#[test]
fn the_spec_at_head_reads_with_its_team_kinds() {
    let files = MemFiles::with(&[]);
    let show = |p: &str, t: &str| (("h".to_string(), p.to_string()), t.to_string());
    let vcs = ig_core::testing::FakeVcs {
        head: Some("h".into()),
        shows: [
            show(
                "ig/system.kdl",
                "system \"shop\" title=\"Shop\" {\n    at \"**\"\n    iot-device \"probe\" title=\"Probe\"\n}\n",
            ),
            show(
                "ig/kinds/iot-device.kdl",
                "kind \"iot-device\" is=\"service\" title=\"IoT device\" means=\"A device\"\n",
            ),
        ]
        .into(),
        ..Default::default()
    };
    let m = prior(&env(&files, Some(&vcs), &Fake)).expect("HEAD's spec reads");
    assert!(m.boxes.iter().any(|b| b.id == "probe"));
}
