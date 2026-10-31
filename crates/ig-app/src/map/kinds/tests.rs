use super::*;
use crate::map::facts::Role;
use BoxKind::*;

const ALL_BOX_KINDS: [BoxKind; 25] = [
    Person, System, External, App, Service, Job, Store, Queue, Model, Module, Entry, Usecase, Port,
    Adapter, Agent, Domain, Group, Folder, File, Component, Parts, Task, Function, Class, Method,
];

fn mem(files: &[(&str, &str)]) -> ig_core::testing::MemFiles {
    ig_core::testing::MemFiles::with(files)
}

#[test]
fn every_spec_kind_is_defined_exactly_once() {
    let k = Kinds::builtin();
    for b in ALL_BOX_KINDS.iter().filter(|b| !b.code_level()) {
        let name = serde_json::to_value(b).unwrap();
        assert_eq!(
            k.names().iter().filter(|n| **n == name).count(),
            1,
            "{name}"
        );
    }
    assert!(
        k.names()
            .iter()
            .all(|n| k.get(n).unwrap().base.role() != Role::Code)
    );
}

#[test]
fn a_team_kind_extends_a_built_in_one() {
    let files = mem(&[(
        "ig/kinds/iot-device.kdl",
        "kind \"iot-device\" is=\"service\" title=\"IoT device\" { property \"firmware\" required=#true }",
    )]);
    let k = Kinds::load(&files).unwrap();
    let d = k.get("iot-device").unwrap();
    assert_eq!((d.base, d.custom), (BoxKind::Service, true));
    assert!(d.props.iter().any(|p| p.name == "firmware" && p.required));
}

#[test]
fn bad_team_kinds_name_their_file() {
    for (text, says) in [
        ("kind \"x\" is=\"nope\"", "`nope` is not a built-in kind"),
        (
            "kind \"x\" is=\"function\"",
            "`function` is a code-level kind",
        ),
        (
            "kind \"service\" is=\"service\"",
            "`service` is a built-in kind",
        ),
        ("kind \"x\" is=\"service\" {", "ig/kinds/x.kdl:1"),
        (
            "kind \"x\" is=\"service\" { arrow \"pings\" }",
            "a kind cannot add arrows",
        ),
        ("kind \"calls\" is=\"service\"", "`calls` is an arrow kind"),
        (
            "kind \"depends\" is=\"service\"",
            "`depends` is an arrow kind",
        ),
        (
            "kind \"badges\" is=\"service\"",
            "`badges` is a word of ig/",
        ),
    ] {
        let e = Kinds::load(&mem(&[("ig/kinds/x.kdl", text)]))
            .unwrap_err()
            .join("\n");
        assert!(
            e.contains("ig/kinds/x.kdl") && e.contains(says),
            "{text}: {e}"
        );
    }
}

#[test]
fn a_team_kind_keeps_its_base_props_and_arrow_rules() {
    let files = mem(&[(
        "ig/kinds/llm.kdl",
        "kind \"llm\" is=\"model\" title=\"LLM\" { property \"context\" }",
    )]);
    let k = Kinds::load(&files).unwrap();
    let llm = k.get("llm").unwrap();
    let names: Vec<_> = llm.props.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names, ["provider", "model", "hosted", "context"]);
    assert_eq!(llm.props[2].one_of, ["local", "hosted"]);
    assert_eq!(k.get("entry").unwrap().props[0].one_of.len(), 4);
    let (svc, store, job) = (
        k.get("service").unwrap(),
        k.get("store").unwrap(),
        k.get("job").unwrap(),
    );
    assert!(k.arrow_ok(ArrowKind::Reads, svc, store));
    assert!(!k.arrow_ok(ArrowKind::Reads, svc, job));
    assert!(k.arrow_ok(ArrowKind::Triggers, svc, job));
    assert!(k.arrow_ok(ArrowKind::Triggers, svc, k.get("entry").unwrap()));
    assert!(k.arrow_ok(ArrowKind::Calls, svc, job));
    assert!(!k.arrow_ok(ArrowKind::Implements, svc, k.get("port").unwrap()));
    assert_eq!(k.names().last(), Some(&"llm"));
}

#[test]
fn where_a_block_may_sit() {
    use Role::{Design as D, Group as G, Root as R, Run as U};
    assert!(may_sit(U, R, true));
    assert!(may_sit(U, G, true));
    assert!(!may_sit(U, G, false));
    assert!(may_sit(D, U, false));
    assert!(!may_sit(D, R, true));
    assert!(kind_may_sit(Module, R, true) && kind_may_sit(Module, G, true));
    assert!(!kind_may_sit(Usecase, R, true));
    for (parent, outer) in [(R, true), (U, false), (D, false), (G, true), (G, false)] {
        assert!(may_sit(G, parent, outer), "{parent:?}");
    }
}

#[test]
fn only_person_external_and_group_are_not_code_and_a_team_kind_takes_its_bases() {
    let k = Kinds::builtin();
    let not: Vec<&str> = (k.names().into_iter())
        .filter(|n| !k.get(n).unwrap().code)
        .collect();
    assert_eq!(not, ["person", "external", "group"]);
    let files = mem(&[("ig/kinds/partner.kdl", "kind \"partner\" is=\"external\"")]);
    let k = Kinds::load(&files).unwrap();
    assert!(!k.get("partner").unwrap().code);
    let bad = mem(&[("ig/kinds/p.kdl", "kind \"p\" is=\"service\" code=#false")]);
    assert!(Kinds::load(&bad).is_err(), "a team kind takes its base's");
}
