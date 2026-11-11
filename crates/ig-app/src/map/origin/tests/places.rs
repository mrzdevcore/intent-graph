use super::*;

#[test]
fn an_arrow_says_where_ig_writes_it_and_where_the_code_does_it() {
    let m = compare(&spec(), &facts(), &graph(), None);
    let a = m.arrows.iter().find(|a| a.id == "web->api").unwrap();
    assert!(
        a.written
            .0
            .as_deref()
            .is_some_and(|w| w.starts_with("ig/") && w.contains(".kdl:")),
        "{:?}",
        a.written
    );
    assert!(
        !a.seen.is_empty() && a.seen.len() <= crate::map::arrows::SEEN,
        "{:?}",
        a.seen
    );
    assert!(
        a.seen
            .iter()
            .all(|s| s.starts_with("apps/web/")
                && s.rsplit(':').next().unwrap().parse::<u32>().is_ok()),
        "{:?}",
        a.seen
    );
    let m = compare(&without(spec(), "web"), &facts(), &graph(), None);
    let d = m
        .arrows
        .iter()
        .find(|a| a.from == "web" && a.to == "api")
        .unwrap();
    assert_eq!(d.origin, Origin::Drift);
    assert!(d.written.0.is_none());
    assert!(!d.seen.is_empty(), "{d:?}");
}
