//! `set kind`: the node renamed in place, refused where the kind may not sit; team kinds.

use super::*;

const SHOP: &str = "system \"shop\" title=\"Shop\" {\n    at \"**\"\n    app \"api\" title=\"API\" { // serves the shop\n        usecase \"api/x\" title=\"X\"\n    }\n}\n";

fn kind(id: &str, to: &str) -> Edit {
    Edit::Set {
        id: id.into(),
        key: "kind".into(),
        value: Some(to.into()),
        restore: vec![],
    }
}

#[test]
fn set_kind_renames_the_node_in_place() {
    let f = MemFiles::with(&[("ig/system.kdl", SHOP)]);
    let want = SHOP.replace("app \"api\"", "service \"api\"");
    let inv = check_on(f, kind("api", "service"), &[("ig/system.kdl", &want)]);
    assert_eq!(inv, kind("api", "app"));
}

#[test]
fn set_kind_refuses_a_kind_that_may_not_sit_there() {
    for (to, says) in [
        ("usecase", "a usecase sits inside a running block"),
        ("nonsense", "`nonsense` is not a kind"),
        ("system", "`system`"),
    ] {
        let f = MemFiles::with(&[("ig/system.kdl", SHOP)]);
        let errs = apply(&f, &kind("api", to)).unwrap_err();
        assert!(errs.iter().any(|e| e.contains(says)), "{to}: {errs:?}");
        assert_eq!(texts(&f)["ig/system.kdl"], SHOP, "nothing written");
    }
}

const IOT: &str =
    "kind \"iot-device\" is=\"service\" title=\"IoT device\" means=\"A device in the field\"\n";

#[test]
fn an_ig_with_a_team_kind_takes_edits_and_fmt() {
    let sys = "system \"shop\" title=\"Shop\" {\n    at \"**\"\n    iot-device \"probe\" title=\"Probe\"\n    app \"api\" title=\"API\"\n}\n";
    let f = || MemFiles::with(&[("ig/system.kdl", sys), ("ig/kinds/iot-device.kdl", IOT)]);
    let title = Edit::Set {
        id: "probe".into(),
        key: "title".into(),
        value: Some("Sensor".into()),
        restore: vec![],
    };
    check_on(
        f(),
        title,
        &[("ig/system.kdl", &sys.replace("Probe", "Sensor"))],
    );
    let want = sys.replace("app \"api\"", "iot-device \"api\"");
    check_on(f(), kind("api", "iot-device"), &[("ig/system.kdl", &want)]);
    assert!(fmt(&f(), true).unwrap().is_empty());
}

#[test]
fn add_writes_a_team_kind_and_the_kinds_own_properties() {
    let f = MemFiles::with(&[("ig/system.kdl", SHOP), ("ig/kinds/iot-device.kdl", IOT)]);
    let add = |kind: &str, id: &str, props: &[(&str, &str)]| Edit::Add {
        parent: "shop".into(),
        kind: kind.into(),
        id: id.into(),
        title: "T".into(),
        at: vec![],
        before: None,
        props: props
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect(),
    };
    apply(&f, &add("iot-device", "probe", &[])).unwrap();
    apply(
        &f,
        &add(
            "model",
            "llm",
            &[("provider", "ollama"), ("hosted", "local")],
        ),
    )
    .unwrap();
    let sys = &texts(&f)["ig/system.kdl"];
    assert!(sys.contains("iot-device \"probe\" title=\"T\"\n"), "{sys}");
    assert!(
        sys.contains("model \"llm\" title=\"T\" hosted=\"local\" provider=\"ollama\"\n"),
        "{sys}"
    );
    let bad = add("model", "x", &[("hosted", "cloud")]);
    let errs = apply(&f, &bad).unwrap_err();
    assert!(
        errs.iter().any(|e| e.contains("`hosted` is one of")),
        "{errs:?}"
    );
}

fn prop(id: &str, key: &str, v: Option<&str>) -> Edit {
    Edit::Set {
        id: id.into(),
        key: key.into(),
        value: v.map(String::from),
        restore: vec![],
    }
}

const LLM: &str = "system \"shop\" title=\"Shop\" {\n    at \"**\"\n    model \"llm\" title=\"LLM\" provider=\"ollama\" tech=\"llama\"\n}\n";

#[test]
fn set_takes_owner_and_the_kinds_own_properties() {
    let f = || MemFiles::with(&[("ig/system.kdl", LLM)]);
    let sys = |a: &str, b: &str| LLM.replace(a, b);
    check_on(
        f(),
        prop("llm", "provider", Some("anthropic")),
        &[("ig/system.kdl", &sys("ollama", "anthropic"))],
    );
    check_on(
        f(),
        prop("llm", "provider", None),
        &[("ig/system.kdl", &sys(" provider=\"ollama\"", ""))],
    );
    check_on(
        f(),
        prop("llm", "owner", Some("ai-team")),
        &[(
            "ig/system.kdl",
            &sys("tech=\"llama\"", "tech=\"llama\" owner=\"ai-team\""),
        )],
    );
    check_on(
        f(),
        prop("llm", "runs", Some("incus:a/b")),
        &[(
            "ig/system.kdl",
            &sys("tech=\"llama\"", "tech=\"llama\" runs=\"incus:a/b\""),
        )],
    );
    let errs = apply(&f(), &prop("llm", "prompt", Some("x"))).unwrap_err();
    assert!(
        errs.iter()
            .any(|e| e.contains("`prompt` is not a property of a model")),
        "{errs:?}"
    );
}

#[test]
fn set_kind_drops_the_properties_the_new_kind_does_not_take_and_undo_restores_them() {
    let f = MemFiles::with(&[("ig/system.kdl", LLM)]);
    let want = LLM.replace(
        "model \"llm\" title=\"LLM\" provider=\"ollama\"",
        "service \"llm\" title=\"LLM\"",
    );
    let inv = check_on(f, kind("llm", "service"), &[("ig/system.kdl", &want)]);
    assert_eq!(
        inv,
        Edit::Set {
            id: "llm".into(),
            key: "kind".into(),
            value: Some("model".into()),
            restore: vec![(2, " provider=\"ollama\"".into())],
        }
    );
    // through the journal too
    let f = MemFiles::with(&[("ig/system.kdl", LLM)]);
    crate::map::edit::journal::record(
        &f,
        crate::map::edit::journal::By::Person,
        &kind("llm", "service"),
    )
    .unwrap();
    crate::map::edit::journal::undo(&f).unwrap().unwrap();
    assert_eq!(texts(&f)["ig/system.kdl"], LLM);
}

#[test]
fn dropping_a_kind_change_puts_a_box_back_where_it_was_out_of_place() {
    // from before groups: a usecase in a folder straight in the system
    let sys = "system \"shop\" title=\"Shop\" {\n    at \"**\"\n    folder \"f\" title=\"F\" {\n        usecase \"u\" title=\"U\"\n    }\n}\n";
    use crate::map::edit::journal::{By, drop, record};
    let f = MemFiles::with(&[("ig/system.kdl", sys)]);
    let e = record(&f, By::Claude, &kind("u", "service")).unwrap();
    drop(&f, e.n).unwrap();
    assert_eq!(texts(&f)["ig/system.kdl"], sys);
    // and a box out of place may stay so: never worse
    let f = MemFiles::with(&[("ig/system.kdl", sys)]);
    apply(&f, &kind("u", "domain")).unwrap();
}
