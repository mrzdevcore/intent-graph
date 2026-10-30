//! A box whose `at` changed since the last commit, its code still at the old place.

use super::*;

const OLD: &str = "apps/api/src/application/refund/**";
const NEW: &str = "billing/src/refund/**";
const ID: &str = "api/application-refund";

/// The shop spec with the refund box moved under a new `billing` at `NEW`.
fn moved() -> MapFile {
    let mut s = with_billing();
    let b = s.boxes.iter_mut().find(|b| b.id == ID).unwrap();
    assert_eq!(b.made_of, [OLD]);
    b.parent = Some("billing".into());
    b.made_of = vec![NEW.into()];
    s
}

#[test]
fn a_box_moved_since_the_last_commit_is_to_move_and_its_old_code_is_not_drift() {
    let m = compare(&moved(), &facts(), &graph(), Some(&spec()));
    assert_eq!(origin_of(&m, ID), Origin::Moved);
    let b = m.boxes.iter().find(|b| b.id == ID).unwrap();
    assert!(
        b.nodes.iter().any(|n| n.contains("application/refund")),
        "{b:?}"
    );
    let p = plan(&m);
    assert_eq!(p.moves, [format!("move: {OLD} → {NEW} ({ID})")], "{p:?}");
    assert!(p.drift.is_empty(), "{p:?}");
    // the refund box's own arrows still find their code
    assert_eq!(arrow_origin(&m, ID, "api/infra-stripe"), Origin::Built);
}

#[test]
fn without_a_prior_or_once_the_code_moved_it_is_not_a_move() {
    // no prior: the new place is to build, the old code is api's
    let m = compare(&moved(), &facts(), &graph(), None);
    assert_eq!(origin_of(&m, ID), Origin::Build);
    let api = m.boxes.iter().find(|b| b.id == "api").unwrap();
    assert!(api.nodes.iter().any(|n| n.contains("application/refund")));
    // an unchanged `at` is never a move
    let m = compare(&spec(), &facts(), &graph(), Some(&spec()));
    assert!(m.boxes.iter().all(|b| b.origin == Origin::Built));
}
