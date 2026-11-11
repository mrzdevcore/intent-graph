//! `set context`: a single-line string, or a raw multi-line one when the text has line breaks.

use super::*;

const SHOP: &str = "system \"shop\" title=\"Shop\" {\n    at \"**\"\n    app \"api\" title=\"API\" {\n        at \"apps/api/**\"\n        usecase \"api/x\" title=\"X\"\n    }\n}\n";

fn context(id: &str, v: Option<&str>) -> Edit {
    Edit::Set {
        id: id.into(),
        key: "context".into(),
        value: v.map(String::from),
        restore: vec![],
    }
}

#[test]
fn set_context_writes_a_raw_string_when_multiline() {
    let f = MemFiles::with(&[("ig/system.kdl", SHOP)]);
    let want = SHOP.replace(
        "        at \"apps/api/**\"\n",
        "        at \"apps/api/**\"\n        context #\"\"\"\n            Build \"\"\" it.\n\n              Twice.\n            \"\"\"#\n",
    );
    let text = "Build \"\"\" it.\n\n  Twice.";
    let inv = check_on(f, context("api", Some(text)), &[("ig/system.kdl", &want)]);
    assert_eq!(inv, context("api", None));
    let m = spec::read(&MemFiles::with(&[("ig/system.kdl", &want)]))
        .unwrap()
        .unwrap();
    let b = m.boxes.iter().find(|b| b.id == "api").unwrap();
    assert_eq!(b.context.as_ref().unwrap().value, text);
}

#[test]
fn set_context_on_one_line_is_a_plain_string_and_replaces_in_place() {
    let f = MemFiles::with(&[("ig/system.kdl", SHOP)]);
    let one = SHOP.replace(
        "        at \"apps/api/**\"\n",
        "        at \"apps/api/**\"\n        context \"for the builder\"\n",
    );
    let inv = check_on(
        f,
        context("api", Some("for the builder")),
        &[("ig/system.kdl", &one)],
    );
    let f = MemFiles::with(&[("ig/system.kdl", &one)]);
    let multi = one.replace(
        "context \"for the builder\"",
        "context #\"\"\"\n            a\n            b\n            \"\"\"#",
    );
    let back = check_on(
        f,
        context("api", Some("a\nb")),
        &[("ig/system.kdl", &multi)],
    );
    let entry = " \"for the builder\"".to_string();
    let want = Edit::Set {
        id: "api".into(),
        key: "context".into(),
        value: Some("for the builder".into()),
        restore: vec![(0, entry)],
    };
    assert_eq!(back, want);
    assert_eq!(inv, context("api", None));
}

#[test]
fn removing_a_context_puts_it_back_byte_for_byte() {
    let with = SHOP.replace(
        "        at \"apps/api/**\"\n",
        "        at \"apps/api/**\"\n\n        context   \"hand  written\" // keep\n",
    );
    let f = MemFiles::with(&[("ig/system.kdl", &with)]);
    check_on(f, context("api", None), &[("ig/system.kdl", SHOP)]);
}

#[test]
fn context_replaced_by_hand_formatting_is_restored_exactly() {
    let with = SHOP.replace(
        "        at \"apps/api/**\"\n",
        "        at \"apps/api/**\"\n        context   \"hand  written\"\n",
    );
    let f = MemFiles::with(&[("ig/system.kdl", &with)]);
    let after = with.replace("context   \"hand  written\"", "context \"new\"");
    check_on(f, context("api", Some("new")), &[("ig/system.kdl", &after)]);
}

#[test]
fn a_context_without_text_is_refused_not_a_panic() {
    let with = SHOP.replace(
        "        at \"apps/api/**\"\n",
        "        at \"apps/api/**\"\n        context\n",
    );
    let f = MemFiles::with(&[("ig/system.kdl", &with)]);
    let e = apply(&f, &context("api", Some("new"))).unwrap_err();
    assert!(e.iter().any(|m| m.contains("has no text")), "{e:?}");
    assert_eq!(texts(&f)["ig/system.kdl"], with);
}
