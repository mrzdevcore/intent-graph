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
        kind: BoxKind::Store,
        id: id.into(),
        title: id.into(),
        at: vec![],
        before: None,
    }
}

fn set(id: &str, v: &str) -> Edit {
    Edit::Set {
        id: id.into(),
        key: "title".into(),
        value: Some(v.into()),
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
