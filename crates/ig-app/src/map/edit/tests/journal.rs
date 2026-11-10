use super::*;
use crate::map::edit::journal::*;

/// The spec files only, not the journal.
fn ig(f: &MemFiles) -> BTreeMap<String, String> {
    texts(f)
        .into_iter()
        .filter(|(p, _)| p.starts_with("ig/"))
        .collect()
}

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

fn set(id: &str, v: &str) -> Edit {
    Edit::Set {
        id: id.into(),
        key: "title".into(),
        value: Some(v.into()),
        restore: vec![],
    }
}

#[test]
fn claude_edits_start_not_kept_and_keep_marks_them() {
    let f = spec();
    let a = record(&f, By::Claude, &add("cache")).unwrap();
    let p = record(&f, By::Person, &set("db", "PG")).unwrap();
    assert!(!a.kept && p.kept && (a.n, p.n) == (1, 2));
    keep(&f, None).unwrap();
    assert!(entries(&f).0.iter().all(|e| e.kept));
}

#[test]
fn keep_all_skips_undone_entries() {
    let f = spec();
    record(&f, By::Claude, &add("cache")).unwrap();
    undo(&f).unwrap();
    keep(&f, None).unwrap();
    redo(&f).unwrap();
    assert!(!entries(&f).0[0].kept);
}

#[test]
fn drop_gives_the_files_back_and_removes_the_entry() {
    let f = spec();
    let before = ig(&f);
    let a = record(&f, By::Claude, &add("cache")).unwrap();
    assert_ne!(ig(&f), before);
    drop(&f, a.n).unwrap();
    assert_eq!(ig(&f), before);
    assert!(entries(&f).0.is_empty());
    assert_eq!(record(&f, By::Claude, &add("cache")).unwrap().n, 2);
}

#[test]
fn drop_works_after_a_person_edits_another_node() {
    let f = spec();
    let a = record(&f, By::Claude, &add("cache")).unwrap();
    record(&f, By::Person, &set("db", "PG")).unwrap();
    drop(&f, a.n).unwrap();
    assert!(ig(&f)["ig/system.kdl"].contains(r#"title="PG""#));
    assert!(!ig(&f)["ig/system.kdl"].contains("cache"));
    assert_eq!(entries(&f).0.len(), 1);
}

#[test]
fn drop_fails_and_changes_nothing_when_the_node_is_gone() {
    let f = spec();
    let a = record(&f, By::Claude, &add("cache")).unwrap();
    record(&f, By::Person, &Edit::Remove { id: "cache".into() }).unwrap();
    let (files, list) = (ig(&f), entries(&f).0);
    assert!(drop(&f, a.n).is_err());
    assert_eq!((ig(&f), entries(&f).0), (files, list));
}

#[test]
fn undo_and_redo_round_trip_and_a_new_record_ends_redo() {
    let f = spec();
    let before = ig(&f);
    record(&f, By::Person, &add("cache")).unwrap();
    let after = ig(&f);
    assert_eq!(undo(&f).unwrap().unwrap().n, 1);
    assert_eq!(ig(&f), before);
    assert!(undo(&f).unwrap().is_none());
    assert_eq!(redo(&f).unwrap().unwrap().n, 1);
    assert_eq!(ig(&f), after);
    assert!(redo(&f).unwrap().is_none());
    undo(&f).unwrap();
    record(&f, By::Person, &add("queue")).unwrap();
    assert!(redo(&f).unwrap().is_none());
    assert_eq!(entries(&f).0.len(), 1);
}

#[test]
fn a_corrupt_journal_is_empty_with_a_notice() {
    let f = spec();
    f.write_atomic(JOURNAL, b"{nope").unwrap();
    let (list, notice) = entries(&f);
    assert!(list.is_empty() && notice.is_some());
    assert_eq!(record(&f, By::Person, &add("cache")).unwrap().n, 1);
    assert!(entries(&f).1.is_none());
    assert_eq!(f.read_bytes(&format!("{JOURNAL}.bad")).unwrap(), b"{nope");
}

#[test]
fn undo_then_record_never_reuses_the_undone_number() {
    let f = spec();
    record(&f, By::Person, &add("cache")).unwrap();
    undo(&f).unwrap();
    assert_eq!(record(&f, By::Person, &add("queue")).unwrap().n, 2);
}

#[test]
fn dropping_an_undone_entry_leaves_the_spec_alone() {
    let f = spec();
    let before = ig(&f);
    let a = record(&f, By::Claude, &add("cache")).unwrap();
    undo(&f).unwrap();
    drop(&f, a.n).unwrap();
    assert_eq!(ig(&f), before);
    assert!(entries(&f).0.is_empty());
}

#[test]
fn drop_refuses_an_entry_a_later_person_edit_changed() {
    // by a recorded person edit
    let f = spec();
    let a = record(&f, By::Claude, &set("db", "X")).unwrap();
    record(&f, By::Person, &set("db", "Y")).unwrap();
    let (files, list) = (ig(&f), entries(&f).0);
    let e = drop(&f, a.n).unwrap_err();
    assert_eq!(
        e,
        vec![format!("entry {} changed since; nothing done", a.n)]
    );
    assert_eq!((ig(&f), entries(&f).0), (files, list));
    // by hand
    let f = spec();
    let a = record(&f, By::Claude, &set("db", "X")).unwrap();
    let t = ig(&f)["ig/system.kdl"].replace(r#"title="X""#, r#"title="Y""#);
    f.write_atomic("ig/system.kdl", t.as_bytes()).unwrap();
    let (files, list) = (ig(&f), entries(&f).0);
    assert!(drop(&f, a.n).is_err());
    assert_eq!((ig(&f), entries(&f).0), (files, list));
}

#[test]
fn drop_checks_each_kind_of_edit_still_holds() {
    let mv = |p: &str| Edit::Move {
        id: "api/orders".into(),
        parent: p.into(),
        before: None,
        slot: None,
    };
    let arrow = Edit::Arrow {
        from: "web".into(),
        to: "db".into(),
        kind: ArrowKind::Reads,
        label: None,
    };
    let unarrow = Edit::Unarrow {
        from: "web".into(),
        to: "db".into(),
        kind: ArrowKind::Reads,
    };
    let rename = |a: &str, b: &str| Edit::Rename {
        id: a.into(),
        to: b.into(),
    };
    for (claude, person) in [
        (mv("web"), mv("api")),
        (rename("db", "pg"), rename("pg", "db")),
        (arrow, unarrow),
        (add("cache"), Edit::Remove { id: "cache".into() }),
    ] {
        let f = spec();
        let a = record(&f, By::Claude, &claude).unwrap();
        record(&f, By::Person, &person).unwrap();
        let files = ig(&f);
        assert!(drop(&f, a.n).is_err(), "{claude:?}");
        assert_eq!(ig(&f), files, "{claude:?}");
    }
    // removed, then written again by the person
    let f = spec();
    record(&f, By::Person, &add("cache")).unwrap();
    let a = record(&f, By::Claude, &Edit::Remove { id: "cache".into() }).unwrap();
    record(&f, By::Person, &add("cache")).unwrap();
    assert!(drop(&f, a.n).is_err());
}

#[test]
fn redo_refuses_when_the_spec_changed_since_the_undo() {
    let f = spec();
    record(&f, By::Person, &set("db", "X")).unwrap();
    undo(&f).unwrap();
    let t = ig(&f)["ig/system.kdl"].replace(r#"title="Postgres""#, r#"title="Z""#);
    f.write_atomic("ig/system.kdl", t.as_bytes()).unwrap();
    let (files, list) = (ig(&f), entries(&f).0);
    assert_eq!(
        redo(&f).unwrap_err(),
        vec!["entry 1 changed since; nothing done".to_string()]
    );
    assert_eq!((ig(&f), entries(&f).0), (files, list));
}

#[test]
fn keep_of_an_unknown_entry_errs_and_writes_nothing() {
    let f = spec();
    record(&f, By::Claude, &add("cache")).unwrap();
    let before = f.read_bytes(JOURNAL);
    assert_eq!(
        keep(&f, Some(9)).unwrap_err(),
        vec!["no journal entry 9".to_string()]
    );
    assert_eq!(f.read_bytes(JOURNAL), before);
}

#[test]
fn a_batch_is_one_journal_entry_and_undoes_in_one_step() {
    let f = spec();
    let before = ig(&f);
    let e = Edit::Batch {
        edits: vec![
            add("cache"),
            Edit::Arrow {
                from: "web".into(),
                to: "cache".into(),
                kind: ArrowKind::Reads,
                label: None,
            },
            set("db", "PG"),
        ],
    };
    record(&f, By::Claude, &e).unwrap();
    assert_eq!(entries(&f).0.len(), 1);
    let done = ig(&f);
    assert_ne!(done, before);
    undo(&f).unwrap();
    assert_eq!(ig(&f), before);
    redo(&f).unwrap();
    assert_eq!(ig(&f), done);
    undo(&f).unwrap();
    assert_eq!(ig(&f), before);
}

fn arrow_edit(from: &str, to: &str) -> Edit {
    Edit::Arrow {
        from: from.into(),
        to: to.into(),
        kind: ArrowKind::Reads,
        label: None,
    }
}

#[test]
fn a_batch_of_two_sets_of_one_key_undoes() {
    let f = spec();
    let before = ig(&f);
    let e = Edit::Batch {
        edits: vec![set("db", "A"), set("db", "B")],
    };
    record(&f, By::Person, &e).unwrap();
    undo(&f).unwrap();
    assert_eq!(ig(&f), before);
    redo(&f).unwrap();
    undo(&f).unwrap();
    assert_eq!(ig(&f), before);
}

#[test]
fn a_batch_that_adds_and_removes_undoes() {
    let f = spec();
    let before = ig(&f);
    let e = Edit::Batch {
        edits: vec![
            add("x"),
            arrow_edit("web", "x"),
            Edit::Unarrow {
                from: "web".into(),
                to: "x".into(),
                kind: ArrowKind::Reads,
            },
            Edit::Remove { id: "x".into() },
        ],
    };
    record(&f, By::Person, &e).unwrap();
    assert_eq!(ig(&f), before);
    undo(&f).unwrap();
    assert_eq!(ig(&f), before);
}

#[test]
fn a_batch_is_not_undone_after_a_touched_box_changed() {
    let f = spec();
    let e = Edit::Batch {
        edits: vec![set("db", "A"), set("db", "B")],
    };
    record(&f, By::Person, &e).unwrap();
    apply(&f, &set("db", "C")).unwrap();
    let now = ig(&f);
    let errs = undo(&f).unwrap_err();
    assert!(errs.iter().any(|m| m.contains("changed since")), "{errs:?}");
    assert_eq!(ig(&f), now);
}
