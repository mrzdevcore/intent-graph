//! Only the system claims `**`, the whole repo; edits read and write as tagged JSON.

use super::*;

#[test]
fn no_edit_gives_a_box_the_whole_repo() {
    let add = Edit::Add {
        parent: "shop".into(),
        kind: "app".into(),
        id: "all".into(),
        title: "All".into(),
        at: vec!["**".into()],
        before: None,
        props: Default::default(),
    };
    fails(add, "claims `**`");
    let set = |id: &str| Edit::Set {
        id: id.into(),
        key: "at".into(),
        value: Some("apps/**\n**".into()),
        restore: vec![],
    };
    fails(set("web"), "claims `**`");
    // the system's own `**` stays settable
    let f = spec();
    apply(&f, &set("shop")).unwrap();
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
