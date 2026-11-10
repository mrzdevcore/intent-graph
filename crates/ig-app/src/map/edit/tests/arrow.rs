use super::*;

fn set(from: &str, to: &str, key: &str, v: Option<&str>) -> Edit {
    Edit::ArrowSet {
        from: from.into(),
        to: to.into(),
        key: key.into(),
        value: v.map(String::from),
        before: None,
        restore: vec![],
    }
}

fn back(from: &str, to: &str, key: &str, v: Option<&str>) -> Edit {
    match set(from, to, key, v) {
        Edit::ArrowSet {
            from,
            to,
            key,
            value,
            ..
        } => Edit::ArrowSet {
            from,
            to,
            key,
            value,
            before: Some("api->stripe".into()),
            restore: vec![],
        },
        e => e,
    }
}

fn restoring(key: &str, v: &str, i: usize, text: &str) -> Edit {
    Edit::ArrowSet {
        from: "api".into(),
        to: "db".into(),
        key: key.into(),
        value: Some(v.into()),
        before: None,
        restore: vec![(i, text.into())],
    }
}

fn api(from: &str, to: &str) -> String {
    API.replace(from, to)
}

#[test]
fn arrowset_kind() {
    let want = api(
        "data \"db\" label=\"orders\"",
        "reads \"db\" label=\"orders\"",
    );
    let inv = check(
        set("api", "db", "kind", Some("reads")),
        &[("ig/api.kdl", &want)],
    );
    assert_eq!(inv, set("api", "db", "kind", Some("data")));
}

#[test]
fn arrowset_label() {
    let want = api("label=\"orders\"", "label=\"the orders\"");
    let inv = check(
        set("api", "db", "label", Some("the orders")),
        &[("ig/api.kdl", &want)],
    );
    assert_eq!(inv, restoring("label", "orders", 1, " label=\"orders\""));
    let want = api("data \"db\" label=\"orders\"", "data \"db\"");
    let inv = check(set("api", "db", "label", None), &[("ig/api.kdl", &want)]);
    assert_eq!(inv, restoring("label", "orders", 1, " label=\"orders\""));
    let want = api("network \"stripe\"", "network \"stripe\" label=\"charges\"");
    let inv = check(
        set("api", "stripe", "label", Some("charges")),
        &[("ig/api.kdl", &want)],
    );
    assert_eq!(inv, set("api", "stripe", "label", None));
}

#[test]
fn arrowset_note() {
    let want = api(
        "data \"db\" label=\"orders\"",
        "data \"db\" label=\"orders\" note=\"hot path\"",
    );
    let inv = check(
        set("api", "db", "note", Some("hot path")),
        &[("ig/api.kdl", &want)],
    );
    assert_eq!(inv, set("api", "db", "note", None));
    let f = MemFiles::with(&[("ig/system.kdl", SYS), ("ig/api.kdl", &want)]);
    let inv = check_on(
        f,
        set("api", "db", "note", Some("cold")),
        &[("ig/api.kdl", &want.replace("hot path", "cold"))],
    );
    assert_eq!(inv, restoring("note", "hot path", 2, " note=\"hot path\""));
}

#[test]
fn arrowset_context() {
    let one = api(
        "network \"stripe\"",
        "network \"stripe\" context=\"retry twice\"",
    );
    check(
        set("api", "stripe", "context", Some("retry twice")),
        &[("ig/api.kdl", &one)],
    );
    let multi = api(
        "network \"stripe\"",
        "network \"stripe\" context=#\"\"\"\n        a\n          b\n        \"\"\"#",
    );
    let inv = check(
        set("api", "stripe", "context", Some("a\n  b")),
        &[("ig/api.kdl", &multi)],
    );
    assert_eq!(inv, set("api", "stripe", "context", None));
    let f = MemFiles::with(&[("ig/system.kdl", SYS), ("ig/api.kdl", &multi)]);
    check_on(
        f,
        set("api", "stripe", "context", None),
        &[("ig/api.kdl", API)],
    );
    let m = spec::read(&MemFiles::with(&[
        ("ig/system.kdl", SYS),
        ("ig/api.kdl", &multi),
    ]))
    .unwrap()
    .unwrap();
    let a = m.arrows.iter().find(|a| a.id == "api->stripe").unwrap();
    assert_eq!(a.context.as_deref(), Some("a\n  b"));
}

#[test]
fn arrowset_to_reconnects_keeping_props() {
    let want = api(
        "data \"db\" label=\"orders\"",
        "data \"stripe\" label=\"orders\"",
    )
    .replace("    network \"stripe\"\n", "");
    let f = MemFiles::with(&[
        ("ig/system.kdl", SYS),
        ("ig/api.kdl", &api("    network \"stripe\"\n", "")),
    ]);
    let before = texts(&f);
    let inv = apply(&f, &set("api", "db", "to", Some("stripe"))).unwrap();
    assert_eq!(texts(&f)["ig/api.kdl"], want);
    assert_eq!(inv, set("api", "stripe", "to", Some("db")));
    apply(&f, &inv).unwrap();
    assert_eq!(texts(&f), before);
}

#[test]
fn arrowset_from_moves_the_node_to_the_new_source_box() {
    let want_sys = SYS.replace(
        "        uses \"api\"\n",
        "        uses \"api\"\n        data \"db\" label=\"orders\"\n",
    );
    let want_api = api("    data \"db\" label=\"orders\"\n", "");
    let inv = check(
        set("api", "db", "from", Some("web")),
        &[("ig/system.kdl", &want_sys), ("ig/api.kdl", &want_api)],
    );
    assert_eq!(inv, back("web", "db", "from", Some("api")));
}

#[test]
fn arrowset_flip_swaps_ends_keeping_props() {
    let want_sys = SYS.replace(
        "    store \"db\" title=\"Postgres\" tech=\"postgres\"\n",
        "    store \"db\" title=\"Postgres\" tech=\"postgres\" {\n        data \"api\" label=\"orders\"\n    }\n",
    );
    let want_api = api("    data \"db\" label=\"orders\"\n", "");
    let inv = check(
        set("api", "db", "flip", None),
        &[("ig/system.kdl", &want_sys), ("ig/api.kdl", &want_api)],
    );
    assert_eq!(inv, back("db", "api", "flip", None));
}

#[test]
fn arrowset_refuses_a_duplicate_pair() {
    fails(set("api", "stripe", "to", Some("db")), "written twice");
}

#[test]
fn arrowset_refuses_a_self_arrow() {
    fails(set("api", "db", "to", Some("api")), "its own box");
    fails(set("api", "db", "from", Some("db")), "its own box");
}

#[test]
fn arrowset_refuses_an_unknown_arrow() {
    fails(set("api", "web", "label", Some("x")), "no arrow `api->web`");
}

#[test]
fn arrowset_refuses_a_note_over_80() {
    fails(
        set("api", "db", "note", Some(&"x".repeat(81))),
        "at most 80",
    );
}

fn kept(text: &str, e: Edit) {
    let f = MemFiles::with(&[("ig/system.kdl", SYS), ("ig/api.kdl", text)]);
    let before = texts(&f);
    let inv = apply(&f, &e).unwrap();
    assert_ne!(texts(&f), before);
    apply(&f, &inv).unwrap();
    assert_eq!(texts(&f), before);
}

#[test]
fn arrowset_undoes_a_raw_string_label_exactly() {
    let t = api("label=\"orders\"", "label=#\"or\"ders\"#");
    kept(&t, set("api", "db", "label", Some("x")));
    kept(&t, set("api", "db", "label", None));
}

#[test]
fn arrowset_undoes_a_multiline_context_exactly() {
    let t = api(
        "network \"stripe\"",
        "network \"stripe\" context=#\"\"\"\n        a\n          b\n        \"\"\"#",
    );
    kept(&t, set("api", "stripe", "context", None));
    kept(&t, set("api", "stripe", "context", Some("one")));
}

#[test]
fn arrowset_undoes_a_one_line_context_and_props_out_of_order() {
    let t = api(
        "network \"stripe\"",
        "network \"stripe\" context=\"c\"   note=\"n\" label=\"l\"",
    );
    kept(&t, set("api", "stripe", "context", Some("d")));
    kept(&t, set("api", "stripe", "context", None));
    kept(&t, set("api", "stripe", "label", None));
    kept(&t, set("api", "stripe", "note", Some("m")));
}

#[test]
fn arrowset_from_and_flip_undo_when_the_arrow_is_last_before_the_boxes() {
    let t = api("    network \"stripe\"\n", "");
    kept(&t, set("api", "db", "from", Some("web")));
    kept(&t, set("api", "db", "flip", None));
}

fn arrow_from(id: &str) -> Edit {
    Edit::Arrow {
        from: id.into(),
        to: "db".into(),
        kind: ArrowKind::Reads,
        label: None,
    }
}

#[test]
fn an_arrow_from_a_box_with_a_trailing_comment_and_no_body() {
    let want = SYS.replace(
        "external \"stripe\" title=\"Stripe\" // the PSP",
        "external \"stripe\" title=\"Stripe\" { // the PSP\n        reads \"db\"\n    }",
    );
    check(arrow_from("stripe"), &[("ig/system.kdl", &want)]);
}

#[test]
fn an_arrow_from_a_box_whose_brace_line_has_a_comment() {
    let sys = SYS.replace(
        "app \"web\" title=\"Web\" {",
        "app \"web\" title=\"Web\" { // the site",
    );
    let want = sys.replace(
        "        uses \"api\"\n",
        "        uses \"api\"\n        reads \"db\"\n",
    );
    let f = MemFiles::with(&[("ig/system.kdl", &sys), ("ig/api.kdl", API)]);
    check_on(f, arrow_from("web"), &[("ig/system.kdl", &want)]);
}
