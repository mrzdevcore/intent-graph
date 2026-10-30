use super::*;
use ig_core::testing::MemFiles;

const H: &str = "// ig's intended architecture: edit freely; ig never rewrites this file.\n";

const SYS: &str = r#"// ig's intended architecture: edit freely; ig never rewrites this file.
// the shop
system "shop" title="Shop" {
    // where the code is
    at "**"

    // the database
    store "db" title="Postgres" tech="postgres"
    // payments, outside
    external "stripe" title="Stripe" // the PSP
    app "web" title="Web" {
        at "apps/web/**"
        uses "api"
    }
    // end of system
}
// eof
"#;

const API: &str = r#"// ig's intended architecture: edit freely; ig never rewrites this file.
// the backend
app "api" title="Api" {
    at "apps/api/**"
    data "db" label="orders"
    network "stripe"

    // checkout
    usecase "api/checkout" title="Checkout" {
        at "apps/api/src/checkout/**"
        uses "api/orders"
    }
    usecase "api/orders" title="Orders" // the orders
    // more to come
}
"#;

fn spec() -> MemFiles {
    MemFiles::with(&[("ig/system.kdl", SYS), ("ig/api.kdl", API)])
}

fn texts(f: &MemFiles) -> BTreeMap<String, String> {
    let files = f.files.lock().unwrap();
    files
        .iter()
        .map(|(p, b)| (p.clone(), String::from_utf8(b.clone()).unwrap()))
        .collect()
}

/// Applies `e` to the fixture, checks the files after it equal `want` (the paths named; every
/// other file untouched), then that the inverse gives the fixture back byte for byte.
fn check(e: Edit, want: &[(&str, &str)]) -> Edit {
    let f = spec();
    let before = texts(&f);
    let inv = apply(&f, &e).unwrap();
    let mut expect = before.clone();
    for (p, t) in want {
        match *t {
            "" => expect.remove(*p),
            t => expect.insert(p.to_string(), t.to_string()),
        };
    }
    let after = texts(&f);
    for (p, t) in &expect {
        assert_eq!(after.get(p), Some(t), "{p}");
    }
    assert_eq!(
        after.keys().collect::<Vec<_>>(),
        expect.keys().collect::<Vec<_>>()
    );
    let back = apply(&f, &inv).unwrap();
    assert_eq!(texts(&f), before, "the inverse {inv:?} gives the spec back");
    assert_eq!(apply(&f, &back).unwrap(), inv, "and redoes it");
    assert_eq!(texts(&f), after);
    inv
}

fn fails(e: Edit, says: &str) {
    let f = spec();
    let before = texts(&f);
    let errs = apply(&f, &e).unwrap_err();
    assert!(
        errs.iter().any(|x| x.contains(says)),
        "{errs:?} says {says}"
    );
    assert_eq!(texts(&f), before, "nothing written");
}

#[test]
fn add_a_component_before_a_sibling() {
    let inv = check(
        Edit::Add {
            parent: "api".into(),
            kind: BoxKind::Usecase,
            id: "api/refund".into(),
            title: "Refund".into(),
            at: vec!["apps/api/src/refund/**".into()],
            before: Some("api/orders".into()),
        },
        &[(
            "ig/api.kdl",
            r#"// ig's intended architecture: edit freely; ig never rewrites this file.
// the backend
app "api" title="Api" {
    at "apps/api/**"
    data "db" label="orders"
    network "stripe"

    // checkout
    usecase "api/checkout" title="Checkout" {
        at "apps/api/src/checkout/**"
        uses "api/orders"
    }
    usecase "api/refund" title="Refund" {
        at "apps/api/src/refund/**"
    }
    usecase "api/orders" title="Orders" // the orders
    // more to come
}
"#,
        )],
    );
    assert_eq!(
        inv,
        Edit::Remove {
            id: "api/refund".into()
        }
    );
}

#[test]
fn add_a_container_goes_before_the_blocks_trailing_comment() {
    check(
        Edit::Add {
            parent: "shop".into(),
            kind: BoxKind::Store,
            id: "cache".into(),
            title: "Cache".into(),
            at: vec![],
            before: None,
        },
        &[(
            "ig/system.kdl",
            r#"// ig's intended architecture: edit freely; ig never rewrites this file.
// the shop
system "shop" title="Shop" {
    // where the code is
    at "**"

    // the database
    store "db" title="Postgres" tech="postgres"
    // payments, outside
    external "stripe" title="Stripe" // the PSP
    app "web" title="Web" {
        at "apps/web/**"
        uses "api"
    }
    store "cache" title="Cache"
    // end of system
}
// eof
"#,
        )],
    );
}

const SYS_WITHOUT_WEB: &str = r#"// ig's intended architecture: edit freely; ig never rewrites this file.
// the shop
system "shop" title="Shop" {
    // where the code is
    at "**"

    // the database
    store "db" title="Postgres" tech="postgres"
    // payments, outside
    external "stripe" title="Stripe" // the PSP
    // end of system
}
// eof
"#;

#[test]
fn a_first_child_moves_its_container_to_a_file_of_its_own() {
    check(
        Edit::Add {
            parent: "web".into(),
            kind: BoxKind::Component,
            id: "web/cart".into(),
            title: "Cart".into(),
            at: vec![],
            before: None,
        },
        &[
            ("ig/system.kdl", SYS_WITHOUT_WEB),
            (
                "ig/web.kdl",
                &format!(
                    "{H}{}",
                    r#"app "web" title="Web" {
    at "apps/web/**"
    uses "api"
    component "web/cart" title="Cart"
}
"#
                ),
            ),
        ],
    );
}

#[test]
fn remove_takes_the_comments_above_and_restores_them() {
    let inv = check(
        Edit::Remove {
            id: "api/checkout".into(),
        },
        &[(
            "ig/api.kdl",
            r#"// ig's intended architecture: edit freely; ig never rewrites this file.
// the backend
app "api" title="Api" {
    at "apps/api/**"
    data "db" label="orders"
    network "stripe"
    usecase "api/orders" title="Orders" // the orders
    // more to come
}
"#,
        )],
    );
    assert_eq!(
        inv,
        Edit::Restore {
            parent: "api".into(),
            before: Some("api/orders".into()),
            kdl: "\n    // checkout\n    usecase \"api/checkout\" title=\"Checkout\" {\n        at \"apps/api/src/checkout/**\"\n        uses \"api/orders\"\n    }\n".into(),
        }
    );
}

#[test]
fn removing_a_first_child_keeps_the_block_shaped() {
    check(
        Edit::Set {
            id: "web".into(),
            key: "at".into(),
            value: None,
        },
        &[(
            "ig/system.kdl",
            &SYS.replace("        at \"apps/web/**\"\n", ""),
        )],
    );
}

#[test]
fn move_a_component_to_another_container_rekeys_it_and_its_arrows() {
    let inv = check(
        Edit::Move {
            id: "api/orders".into(),
            parent: "web".into(),
            before: None,
        },
        &[
            (
                "ig/api.kdl",
                r#"// ig's intended architecture: edit freely; ig never rewrites this file.
// the backend
app "api" title="Api" {
    at "apps/api/**"
    data "db" label="orders"
    network "stripe"

    // checkout
    usecase "api/checkout" title="Checkout" {
        at "apps/api/src/checkout/**"
        uses "web/orders"
    }
    // more to come
}
"#,
            ),
            ("ig/system.kdl", SYS_WITHOUT_WEB),
            (
                "ig/web.kdl",
                &format!(
                    "{H}{}",
                    r#"app "web" title="Web" {
    at "apps/web/**"
    uses "api"
    usecase "web/orders" title="Orders" // the orders
}
"#
                ),
            ),
        ],
    );
    assert_eq!(
        inv,
        Edit::Move {
            id: "web/orders".into(),
            parent: "api".into(),
            before: None,
        }
    );
}

#[test]
fn move_reorders_a_subtree_with_its_block() {
    check(
        Edit::Move {
            id: "web".into(),
            parent: "shop".into(),
            before: Some("db".into()),
        },
        &[(
            "ig/system.kdl",
            r#"// ig's intended architecture: edit freely; ig never rewrites this file.
// the shop
system "shop" title="Shop" {
    // where the code is
    at "**"
    app "web" title="Web" {
        at "apps/web/**"
        uses "api"
    }

    // the database
    store "db" title="Postgres" tech="postgres"
    // payments, outside
    external "stripe" title="Stripe" // the PSP
    // end of system
}
// eof
"#,
        )],
    );
}

#[test]
fn rename_takes_the_arrows_pointing_at_it() {
    check(
        Edit::Rename {
            id: "api/orders".into(),
            to: "api/sales".into(),
        },
        &[(
            "ig/api.kdl",
            &API.replace("\"api/orders\"", "\"api/sales\""),
        )],
    );
}

#[test]
fn renaming_a_container_takes_its_components_and_its_file() {
    check(
        Edit::Rename {
            id: "api".into(),
            to: "core".into(),
        },
        &[
            ("ig/api.kdl", ""),
            ("ig/core.kdl", &API.replace("\"api", "\"core")),
            (
                "ig/system.kdl",
                &SYS.replace("uses \"api\"", "uses \"core\""),
            ),
        ],
    );
}

#[test]
fn set_changes_adds_and_removes_properties_in_order() {
    let db = "store \"db\" title=\"Postgres\" tech=\"postgres\"";
    let cases = [
        (
            "title",
            Some("Main DB"),
            "store \"db\" title=\"Main DB\" tech=\"postgres\"",
        ),
        (
            "description",
            Some("the \"orders\""),
            "store \"db\" title=\"Postgres\" tech=\"postgres\" description=\"the \\\"orders\\\"\"",
        ),
        ("tech", None, "store \"db\" title=\"Postgres\""),
        (
            "logo",
            Some("pg"),
            "store \"db\" title=\"Postgres\" tech=\"postgres\" logo=\"pg\"",
        ),
    ];
    for (key, value, line) in cases {
        check(
            Edit::Set {
                id: "db".into(),
                key: key.into(),
                value: value.map(String::from),
            },
            &[("ig/system.kdl", &SYS.replace(db, line))],
        );
    }
}

#[test]
fn set_at_replaces_the_globs() {
    check(
        Edit::Set {
            id: "api/checkout".into(),
            key: "at".into(),
            value: Some("apps/api/src/checkout/**\napps/api/src/pay/**".into()),
        },
        &[(
            "ig/api.kdl",
            &API.replace(
                "at \"apps/api/src/checkout/**\"",
                "at \"apps/api/src/checkout/**\" \"apps/api/src/pay/**\"",
            ),
        )],
    );
}

#[test]
fn set_at_on_a_box_without_one_puts_it_first() {
    check(
        Edit::Set {
            id: "api/orders".into(),
            key: "at".into(),
            value: Some("apps/api/src/orders/**".into()),
        },
        &[(
            "ig/api.kdl",
            &API.replace(
                "    usecase \"api/orders\" title=\"Orders\" // the orders\n",
                "    usecase \"api/orders\" title=\"Orders\" {\n        at \"apps/api/src/orders/**\"\n    } // the orders\n",
            ),
        )],
    );
}

#[test]
fn arrow_goes_after_the_arrows_and_unarrow_restores_it() {
    check(
        Edit::Arrow {
            from: "api".into(),
            to: "web".into(),
            kind: ArrowKind::Event,
            label: Some("paid".into()),
        },
        &[(
            "ig/api.kdl",
            &API.replace(
                "    network \"stripe\"\n",
                "    network \"stripe\"\n    event \"web\" label=\"paid\"\n",
            ),
        )],
    );
    let inv = check(
        Edit::Unarrow {
            from: "api".into(),
            to: "stripe".into(),
            kind: ArrowKind::Network,
        },
        &[("ig/api.kdl", &API.replace("    network \"stripe\"\n", ""))],
    );
    assert!(matches!(inv, Edit::Restore { before: Some(b), .. } if b == "api/checkout"));
}

#[test]
fn bad_edits_write_nothing() {
    fails(Edit::Remove { id: "nope".into() }, "no box `nope`");
    fails(
        Edit::Remove { id: "api".into() },
        "`api` has children: api/checkout, api/orders",
    );
    fails(
        Edit::Remove {
            id: "api/orders".into(),
        },
        "api/checkout->api/orders",
    );
    fails(
        Edit::Add {
            parent: "shop".into(),
            kind: BoxKind::Store,
            id: "db".into(),
            title: "Again".into(),
            at: vec![],
            before: None,
        },
        "`db` is already used",
    );
    fails(
        Edit::Add {
            parent: "nope".into(),
            kind: BoxKind::Store,
            id: "x".into(),
            title: "X".into(),
            at: vec![],
            before: None,
        },
        "no box `nope`",
    );
    fails(
        Edit::Rename {
            id: "api/orders".into(),
            to: "api/checkout".into(),
        },
        "`api/checkout` is already used",
    );
    fails(
        Edit::Move {
            id: "api".into(),
            parent: "api/checkout".into(),
            before: None,
        },
        "into itself",
    );
    fails(
        Edit::Move {
            id: "web".into(),
            parent: "web".into(),
            before: None,
        },
        "into itself",
    );
    // the reader rejects what the edit makes
    fails(
        Edit::Set {
            id: "db".into(),
            key: "title".into(),
            value: None,
        },
        "`db` has no title",
    );
    fails(
        Edit::Set {
            id: "db".into(),
            key: "colour".into(),
            value: None,
        },
        "`colour`",
    );
}

#[test]
fn edits_read_and_write_as_tagged_json() {
    let e = Edit::Unarrow {
        from: "a".into(),
        to: "b".into(),
        kind: ArrowKind::Uses,
    };
    let v = serde_json::to_value(&e).unwrap();
    assert_eq!(
        v,
        serde_json::json!({"op": "unarrow", "from": "a", "to": "b", "kind": "uses"})
    );
    assert_eq!(serde_json::from_value::<Edit>(v).unwrap(), e);
}
