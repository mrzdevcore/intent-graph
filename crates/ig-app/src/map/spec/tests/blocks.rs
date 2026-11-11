//! `ig/` read by the kinds table: old names, groups, properties, team kinds.

use super::*;

/// The `ig/` read from `ig/system.kdl` alone.
fn parse(system: &str) -> Result<MapFile, Vec<String>> {
    crate::map::spec::parse(vec![("ig/system.kdl".into(), Some(system.into()))])
}

fn sys(s: &str) -> String {
    format!("system \"system\" title=\"T\" {{\n{s}\n}}\n")
}

/// An `ig/` as written before the building blocks, with the old kind and arrow names.
const OLD_MAP: &str = r#"system "system" title="Shop" {
  api "api" title="API" {
    network "worker"
    event "q"
    data "db"
  }
  worker "worker" title="Worker"
  library "lib" title="Lib" {
    depends "api"
  }
  folder "f" title="F" {
    component "f/c" title="C"
  }
  queue "q" title="Q"
  store "db" title="DB"
}
"#;

#[test]
fn a_map_from_before_reads_with_the_new_kinds() {
    let m = parse(OLD_MAP).unwrap();
    let kind = |id: &str| m.boxes.iter().find(|b| b.id == id).unwrap().kind.value;
    use crate::map::facts::BoxKind;
    assert_eq!(
        [
            kind("api"),
            kind("worker"),
            kind("lib"),
            kind("f"),
            kind("f/c")
        ],
        [
            BoxKind::Service,
            BoxKind::Service,
            BoxKind::Module,
            BoxKind::Group,
            BoxKind::Module
        ]
    );
    assert_eq!(m.arrows.len(), 4);
    assert!(m.arrows.iter().all(|a| matches!(
        a.kind,
        ArrowKind::Uses | ArrowKind::Calls | ArrowKind::Sends | ArrowKind::Reads
    )));
}

#[test]
fn groups_hold_what_may_sit_where_they_sit() {
    assert!(
        parse(&sys(
            "group \"g\" title=\"G\" { group \"h\" title=\"H\" { service \"s\" title=\"S\" } }"
        ))
        .is_ok()
    );
    // a run block in an inner group reads: the table gives a warning (`ig check`), not an error
    assert!(
        parse(&sys(
            "service \"s\" title=\"S\" { group \"s/g\" title=\"G\" { service \"t\" title=\"T\" } }",
        ))
        .is_ok()
    );
    let m = parse(&sys(
        "group \"g\" title=\"G\" { service \"s\" title=\"S\" { usecase \"s/u\" title=\"U\" } }",
    ))
    .unwrap();
    let lvl = |id: &str| m.boxes.iter().find(|b| b.id == id).unwrap().level;
    assert_eq!(
        [lvl("g"), lvl("s"), lvl("s/u")],
        [Level::Container, Level::Container, Level::Component]
    );
}

#[test]
fn properties_follow_the_kind() {
    let m = parse(&sys(
        "model \"llama\" title=\"Llama\" provider=\"ollama\" hosted=\"local\" owner=\"me\"",
    ))
    .unwrap();
    let b = m.boxes.iter().find(|b| b.id == "llama").unwrap();
    assert_eq!(b.props.get("provider").map(String::as_str), Some("ollama"));
    assert_eq!(b.props.get("owner").map(String::as_str), Some("me"));
    for (text, says) in [
        (
            "model \"m\" title=\"M\" hosted=\"cloud\"",
            "`hosted` is one of local, hosted",
        ),
        (
            "service \"s\" title=\"S\" provider=\"x\"",
            "unknown property `provider`",
        ),
        (
            "service \"s\" title=\"S\" runs=\"nonsense\"",
            "`s`: `runs` is <kind>:<remote>/<name>",
        ),
    ] {
        assert!(
            parse(&sys(text)).unwrap_err().join("\n").contains(says),
            "{text}"
        );
    }
}

#[test]
fn runs_is_written_back_as_it_was_read() {
    let m = parse(&sys("service \"s\" title=\"S\" runs=\"incus:a/b\"")).unwrap();
    let files = crate::map::spec::write(&m);
    assert!(files["ig/system.kdl"].contains("runs=\"incus:a/b\""));
    let texts = files.into_iter().map(|(p, t)| (p, Some(t))).collect();
    let back = crate::map::spec::parse(texts).unwrap();
    let s = back.boxes.iter().find(|b| b.id == "s").unwrap();
    assert_eq!(s.props.get("runs").map(String::as_str), Some("incus:a/b"));
}

#[test]
fn thirteen_children_read() {
    let kids: String = (0..13)
        .map(|i| format!("service \"s{i}\" title=\"S\"\n"))
        .collect();
    assert!(parse(&sys(&kids)).is_ok());
}

#[test]
fn a_team_kind_reads_and_writes_by_its_own_name() {
    let kind = "kind \"iot-device\" is=\"service\" { property \"firmware\" required=#true }";
    let read = |box_: &str, with_kind: bool| {
        let mut files = vec![("ig/system.kdl".to_string(), Some(sys(box_)))];
        if with_kind {
            files.push(("ig/kinds/iot-device.kdl".into(), Some(kind.into())));
        }
        crate::map::spec::parse(files)
    };
    let m = read(
        "iot-device \"sensor\" title=\"Sensor\" firmware=\"1.2\"",
        true,
    )
    .unwrap();
    let b = m.boxes.iter().find(|b| b.id == "sensor").unwrap();
    assert_eq!(b.kind.value, crate::map::facts::BoxKind::Service);
    assert_eq!(b.custom.as_deref(), Some("iot-device"));
    let sys_kdl = &write(&m)["ig/system.kdl"];
    assert!(
        sys_kdl.contains("iot-device \"sensor\" title=\"Sensor\" firmware=\"1.2\""),
        "{sys_kdl}"
    );
    let e = read("iot-device \"sensor\" title=\"Sensor\"", true).unwrap_err();
    assert!(
        e.join("\n").contains("`sensor`: `firmware` is required"),
        "{e:?}"
    );
    let e = read(
        "iot-device \"sensor\" title=\"Sensor\" firmware=\"1.2\"",
        false,
    )
    .unwrap_err();
    assert!(
        e.join("\n")
            .contains("`iot-device` is not a kind (no ig/kinds/iot-device.kdl)"),
        "{e:?}"
    );
}

#[test]
fn spec_read_takes_the_team_kinds_from_ig_kinds() {
    let files = ig_core::testing::MemFiles::with(&[
        (
            "ig/system.kdl",
            &sys("iot-device \"sensor\" title=\"Sensor\""),
        ),
        (
            "ig/kinds/iot-device.kdl",
            "kind \"iot-device\" is=\"service\"",
        ),
    ]);
    let m = read(&files).unwrap().unwrap();
    assert_eq!(m.boxes.iter().filter(|b| b.custom.is_some()).count(), 1);
}

#[test]
fn reads_a_box_with_no_kind_yet() {
    use crate::map::facts::BoxKind;
    let m = parse(&sys("  box \"billing\" title=\"billing\"")).unwrap();
    let b = m.boxes.iter().find(|b| b.id == "billing").unwrap();
    assert_eq!(b.kind.value, BoxKind::Box);
    assert!(crate::map::warn::warnings(&m, &crate::map::kinds::Kinds::builtin()).is_empty());
}

#[test]
fn a_box_holds_run_and_design_blocks() {
    let m = parse(&sys(
        "  box \"b\" title=\"B\" {\n    service \"s\" title=\"S\"\n    module \"b/m\" title=\"M\"\n    usecase \"b/u\" title=\"U\"\n  }\n  group \"g\" title=\"G\" {\n    box \"gb\" title=\"GB\" {\n      store \"st\" title=\"St\"\n    }\n  }\n  service \"api\" title=\"Api\" {\n    box \"ab\" title=\"AB\" {\n      usecase \"ab/u\" title=\"U\"\n    }\n  }",
    ))
    .unwrap();
    assert_eq!(
        crate::map::warn::warnings(&m, &crate::map::kinds::Kinds::builtin()),
        Vec::<String>::new()
    );
}

fn box_of<'a>(m: &'a MapFile, id: &str) -> &'a crate::map::file::MapBox {
    m.boxes.iter().find(|b| b.id == id).unwrap()
}

#[test]
fn reads_box_context_multiline() {
    let m = parse(&sys(
        "  service \"api\" title=\"Api\" {\n    context #\"\"\"\n      Build it \"\"\" like\n      this\n\n        indented\n      \"\"\"#\n  }",
    ))
    .unwrap();
    let c = box_of(&m, "api").context.as_ref().unwrap();
    assert_eq!(c.value, "Build it \"\"\" like\nthis\n\n  indented");
    assert_eq!(c.source, crate::map::file::Source::Person);
    let one = parse(&sys(
        "  service \"api\" title=\"Api\" {\n    context \"short\"\n  }",
    ))
    .unwrap();
    assert_eq!(box_of(&one, "api").context.as_ref().unwrap().value, "short");
}

#[test]
fn a_box_context_is_written_once_and_as_a_string() {
    for body in ["context \"a\"\n    context \"b\"", "context", "context 1"] {
        let s = sys(&format!(
            "  service \"api\" title=\"Api\" {{\n    {body}\n  }}"
        ));
        assert!(parse(&s).is_err(), "{body}");
    }
}

#[test]
fn reads_arrow_note_and_context() {
    let m = parse(&sys(
        "  service \"api\" title=\"Api\" {\n    calls \"web\" label=\"orders\" note=\"every minute\" context=#\"\"\"\n      why\n      and how\n      \"\"\"#\n  }\n  app \"web\" title=\"Web\"",
    ))
    .unwrap();
    let a = &m.arrows[0];
    assert_eq!(a.label.as_deref(), Some("orders"));
    assert_eq!(a.note.as_deref(), Some("every minute"));
    assert_eq!(a.context.as_deref(), Some("why\nand how"));
}

#[test]
fn rejects_a_note_over_80_or_with_a_newline() {
    let arrow = |note: &str| {
        parse(&sys(&format!(
            "  service \"api\" title=\"Api\" {{\n    calls \"web\" note=\"{note}\"\n  }}\n  app \"web\" title=\"Web\""
        )))
    };
    assert!(arrow(&"x".repeat(80)).is_ok());
    let long = arrow(&"x".repeat(81)).unwrap_err();
    assert!(
        long.iter().any(|e| e.contains("note") && e.contains("80")),
        "{long:?}"
    );
    let nl = arrow("a\\nb").unwrap_err();
    assert!(
        nl.iter()
            .any(|e| e.contains("note") && e.contains("one line")),
        "{nl:?}"
    );
}

#[test]
fn unknown_arrow_property_is_a_warning() {
    let s = sys(
        "  service \"api\" title=\"Api\" {\n    calls \"web\" colour=\"red\"\n  }\n  app \"web\" title=\"Web\"",
    );
    let texts = vec![("ig/system.kdl".to_string(), Some(s))];
    assert_eq!(
        crate::map::spec::parse(texts.clone()).unwrap().arrows.len(),
        1
    );
    let w = crate::map::spec::read::arrow_warnings(&texts);
    assert_eq!(w.len(), 1, "{w:?}");
    assert!(
        w[0].contains("colour") && w[0].starts_with("ig/system.kdl:"),
        "{w:?}"
    );
}

#[test]
fn context_and_notes_survive_a_write_and_a_read() {
    let m = parse(&sys(
        "  service \"api\" title=\"Api\" {\n    context \"for the builder\"\n    calls \"web\" note=\"hourly\" context=\"why\"\n  }\n  app \"web\" title=\"Web\"",
    ))
    .unwrap();
    let again = crate::map::spec::parse(
        crate::map::spec::write(&m)
            .into_iter()
            .map(|(p, t)| (p, Some(t)))
            .collect(),
    )
    .unwrap();
    assert_eq!(again.boxes, m.boxes);
    assert_eq!(again.arrows, m.arrows);
}

#[test]
fn reads_each_arrow_shape_and_rejects_another() {
    let arrow = |shape: &str| {
        parse(&sys(&format!(
            "  service \"api\" title=\"Api\" {{\n    calls \"web\" shape=\"{shape}\"\n  }}\n  app \"web\" title=\"Web\""
        )))
    };
    for shape in ["circuit", "elbow", "curved", "straight"] {
        let m = arrow(shape).unwrap();
        let v = serde_json::to_value(&m.arrows[0]).unwrap();
        assert_eq!(v["shape"], shape);
        let again = crate::map::spec::parse(
            crate::map::spec::write(&m)
                .into_iter()
                .map(|(p, t)| (p, Some(t)))
                .collect(),
        )
        .unwrap();
        assert_eq!(again.arrows, m.arrows);
    }
    let bad = arrow("zigzag").unwrap_err();
    assert!(
        bad.iter()
            .any(|e| e.contains("shape") && e.contains("circuit, elbow, curved, straight")),
        "{bad:?}"
    );
    let m = parse(&sys(
        "  service \"api\" title=\"Api\" {\n    calls \"web\"\n  }\n  app \"web\" title=\"Web\"",
    ))
    .unwrap();
    assert!(
        serde_json::to_value(&m.arrows[0])
            .unwrap()
            .get("shape")
            .is_none()
    );
}

#[test]
fn shape_is_not_an_unknown_arrow_property() {
    let s = sys(
        "  service \"api\" title=\"Api\" {\n    calls \"web\" shape=\"elbow\"\n  }\n  app \"web\" title=\"Web\"",
    );
    let texts = vec![("ig/system.kdl".to_string(), Some(s))];
    assert!(crate::map::spec::read::arrow_warnings(&texts).is_empty());
}
