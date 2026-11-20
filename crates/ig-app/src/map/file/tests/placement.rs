use super::*;

#[test]
fn annotate_keeps_igs_kind_where_the_agents_would_sit_out_of_place() {
    let mut m = shop();
    let before = get(&m, "api").kind.value;
    annotate(
        &mut m,
        r#"[{"id": "api", "kind": "domain", "description": "The shop's API."}]"#,
    )
    .unwrap();
    let api = get(&m, "api");
    assert_eq!(api.kind.value, before);
    assert_eq!(api.description.as_ref().unwrap().source, Source::Agent);
    assert!(crate::map::warn::warnings(&m, &Default::default()).is_empty());
}

#[test]
fn a_later_batch_that_moves_the_parent_does_not_leave_the_agents_kind_out_of_place() {
    let mut m = shop();
    annotate(&mut m, r#"[{"id": "api/routers", "kind": "adapter"}]"#).unwrap();
    assert_eq!(get(&m, "api/routers").kind.value, BoxKind::Adapter);
    annotate(&mut m, r#"[{"id": "api", "kind": "group"}]"#).unwrap();
    assert_eq!(get(&m, "api/routers").kind.value, BoxKind::Module);
    let w = crate::map::warn::warnings(&m, &Default::default());
    assert!(!w.iter().any(|w| w.contains("api/routers")), "{w:?}");
}
