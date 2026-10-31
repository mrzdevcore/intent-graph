//! `set kind`: the node renamed in place, refused where the kind may not sit; team kinds.

use super::*;

const SHOP: &str = "system \"shop\" title=\"Shop\" {\n    at \"**\"\n    app \"api\" title=\"API\" { // serves the shop\n        usecase \"api/x\" title=\"X\"\n    }\n}\n";

fn kind(id: &str, to: &str) -> Edit {
    Edit::Set {
        id: id.into(),
        key: "kind".into(),
        value: Some(to.into()),
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
