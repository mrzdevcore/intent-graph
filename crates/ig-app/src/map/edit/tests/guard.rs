//! Only the system claims `**`, the whole repo.

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
    };
    fails(set("web"), "claims `**`");
    // the system's own `**` stays settable
    let f = spec();
    apply(&f, &set("shop")).unwrap();
}
