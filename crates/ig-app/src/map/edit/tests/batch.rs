use super::*;

fn add(id: &str) -> Edit {
    Edit::Add {
        parent: "shop".into(),
        kind: "store".into(),
        id: id.into(),
        title: id.into(),
        at: vec![],
        before: None,
        props: Default::default(),
    }
}

fn arrow(from: &str, to: &str) -> Edit {
    Edit::Arrow {
        from: from.into(),
        to: to.into(),
        kind: ArrowKind::Reads,
        label: None,
    }
}

fn unarrow(from: &str, to: &str) -> Edit {
    Edit::Unarrow {
        from: from.into(),
        to: to.into(),
        kind: ArrowKind::Reads,
    }
}

fn batch(edits: Vec<Edit>) -> Edit {
    Edit::Batch { edits }
}

#[test]
fn batch_applies_in_order_and_saves_once() {
    let f = spec();
    let before = texts(&f);
    let inv = apply(&f, &batch(vec![add("cache"), arrow("web", "cache")])).unwrap();
    let after = texts(&f);
    let sys = &after["ig/system.kdl"];
    assert!(sys.contains("store \"cache\" title=\"cache\""));
    assert!(sys.contains("reads \"cache\""));
    assert_eq!(
        inv,
        batch(vec![
            unarrow("web", "cache"),
            Edit::Remove { id: "cache".into() }
        ])
    );
    apply(&f, &inv).unwrap();
    assert_eq!(texts(&f), before);
}

#[test]
fn batch_is_all_or_nothing() {
    let f = spec();
    let before = texts(&f);
    let errs = apply(&f, &batch(vec![add("cache"), arrow("nope", "cache")])).unwrap_err();
    assert!(
        errs.iter()
            .any(|m| m.contains("edit 2") && m.contains("nope"))
    );
    assert_eq!(texts(&f), before);
}

#[test]
fn batch_only_checks_the_end_state() {
    let f = spec();
    let before = texts(&f);
    let e = batch(vec![
        add("x"),
        arrow("web", "x"),
        unarrow("web", "x"),
        Edit::Remove { id: "x".into() },
    ]);
    let inv = apply(&f, &e).unwrap();
    assert_eq!(texts(&f), before);
    apply(&f, &inv).unwrap();
    assert_eq!(texts(&f), before);
}

#[test]
fn a_batch_ending_in_a_state_that_does_not_read_writes_nothing() {
    let f = spec();
    let before = texts(&f);
    let e = batch(vec![
        add("x"),
        arrow("web", "x"),
        Edit::Remove { id: "x".into() },
    ]);
    let errs = apply(&f, &e).unwrap_err();
    assert!(errs.iter().any(|m| m.contains("edit 3")), "{errs:?}");
    assert_eq!(texts(&f), before);
}

#[test]
fn an_empty_batch_is_refused() {
    fails(batch(vec![]), "no edits");
}
