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
            "service \"s\" title=\"S\" runs=\"compose:s\"",
            "`runs` comes with live status; not yet",
        ),
    ] {
        assert!(
            parse(&sys(text)).unwrap_err().join("\n").contains(says),
            "{text}"
        );
    }
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
