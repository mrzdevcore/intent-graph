use super::*;
use crate::map::file::{MapFile, refresh};
use crate::map::spec::{read, write};
use crate::map::{FactBox, arrows};
use ig_core::model::Graph;
use ig_core::testing::MemFiles;
use std::collections::BTreeMap;

fn graph() -> Graph {
    crate::map::shop::load().1
}

fn facts() -> Vec<FactBox> {
    let (ws, g, cfg) = crate::map::shop::load();
    crate::map::facts(&ws, &g, &cfg)
}

fn read_files(w: &BTreeMap<String, String>) -> MapFile {
    let files: Vec<_> = w.iter().map(|(p, t)| (p.as_str(), t.as_str())).collect();
    read(&MemFiles::with(&files)).unwrap().unwrap()
}

fn spec_files() -> BTreeMap<String, String> {
    let b = facts();
    write(&refresh(None, &b, arrows(&b, &graph()), &[]).0)
}

/// The shop map written to `ig/` and read back.
fn spec() -> MapFile {
    read_files(&spec_files())
}

fn with_billing() -> MapFile {
    let mut w = spec_files();
    w.insert(
        "ig/billing.kdl".into(),
        "api \"billing\" title=\"Billing\" {\n  at \"billing/**\"\n  network \"stripe\"\n}\n"
            .into(),
    );
    read_files(&w)
}

/// `m` without the boxes `gone` says, and every arrow touching them.
fn drop(mut m: MapFile, gone: impl Fn(&MapBox) -> bool) -> MapFile {
    let ids: Vec<String> = m
        .boxes
        .iter()
        .filter(|b| gone(b))
        .map(|b| b.id.clone())
        .collect();
    m.boxes.retain(|b| !ids.contains(&b.id));
    m.arrows
        .retain(|a| !ids.contains(&a.from) && !ids.contains(&a.to));
    m
}

fn without(m: MapFile, id: &str) -> MapFile {
    let pre = format!("{id}/");
    drop(m, |b| b.id == id || b.id.starts_with(&pre))
}

fn without_children(m: MapFile, id: &str) -> MapFile {
    drop(m, |b| b.parent.as_deref() == Some(id))
}

fn origin_of(m: &MapFile, id: &str) -> Origin {
    m.boxes
        .iter()
        .find(|b| b.id == id)
        .unwrap_or_else(|| panic!("no box {id}"))
        .origin
}

fn arrow_origin(m: &MapFile, from: &str, to: &str) -> Origin {
    m.arrows
        .iter()
        .find(|a| a.from == from && a.to == to)
        .unwrap_or_else(|| panic!("no arrow {from}->{to}: {:#?}", m.arrows))
        .origin
}

#[test]
fn the_unchanged_code_matches_its_spec() {
    let m = compare(&spec(), &facts(), &graph());
    for b in &m.boxes {
        assert_eq!(b.origin, Origin::Built, "{}", b.id);
    }
    for a in &m.arrows {
        assert_eq!(a.origin, Origin::Built, "{}", a.id);
    }
    assert_eq!(
        plan(&m),
        Plan {
            build: vec![],
            drift: vec![]
        }
    );
}

#[test]
fn a_planned_box_and_its_arrow_are_to_build() {
    let m = compare(&with_billing(), &facts(), &graph());
    assert_eq!(origin_of(&m, "billing"), Origin::Build);
    assert_eq!(arrow_origin(&m, "billing", "stripe"), Origin::Build);
    let p = plan(&m);
    assert!(
        p.build
            .contains(&"to build: api billing (billing/**)".into()),
        "{p:?}"
    );
    assert!(
        p.build.iter().any(|l| l.contains("billing network stripe")),
        "{p:?}"
    );
    assert!(p.drift.is_empty(), "{p:?}");
}

#[test]
fn code_in_no_box_and_unlisted_arrows_are_drift() {
    let m = compare(&without(spec(), "web"), &facts(), &graph());
    assert_eq!(origin_of(&m, "web"), Origin::Drift);
    assert_eq!(arrow_origin(&m, "web", "api"), Origin::Drift);
    assert!(
        !m.boxes.iter().any(|b| b.id.starts_with("web/")),
        "only the highest"
    );
    let p = plan(&m);
    assert!(
        p.drift
            .contains(&"drift: code under apps/web/** is in no box".into()),
        "{p:?}"
    );
    assert!(
        p.drift
            .iter()
            .any(|l| l == "drift: web network api, not in ig/"),
        "{p:?}"
    );
    let mut sorted = p.drift.clone();
    sorted.sort();
    assert_eq!(p.drift, sorted);
}

#[test]
fn the_deepest_box_claims_a_file() {
    let m = compare(&spec(), &facts(), &graph());
    let api = m.boxes.iter().find(|b| b.id == "api").unwrap();
    assert!(
        api.nodes
            .iter()
            .all(|n| !n.contains("application/checkout"))
    );
    let c = m
        .boxes
        .iter()
        .find(|b| b.id == "api/application-checkout")
        .unwrap();
    assert!(c.nodes.iter().any(|n| n.contains("application/checkout")));
}

#[test]
fn code_inside_a_described_box_needs_no_description() {
    let m = compare(&without_children(spec(), "api"), &facts(), &graph());
    assert!(plan(&m).drift.is_empty(), "{:?}", plan(&m));
    assert_eq!(origin_of(&m, "api"), Origin::Built);
}

#[test]
fn an_unlisted_arrow_between_spec_boxes_is_drift_and_a_kind_mismatch_is_to_build() {
    let mut s = spec();
    s.arrows.retain(|a| !(a.from == "api" && a.to == "stripe"));
    let a = s
        .arrows
        .iter_mut()
        .find(|a| a.from == "api" && a.to == "postgres")
        .unwrap();
    a.kind = ArrowKind::Event;
    a.kinds = vec![ArrowKind::Event];
    let m = compare(&s, &facts(), &graph());
    assert_eq!(arrow_origin(&m, "api", "stripe"), Origin::Drift);
    let both: Vec<_> = m
        .arrows
        .iter()
        .filter(|a| a.from == "api" && a.to == "postgres")
        .collect();
    assert_eq!(both.len(), 2, "{both:#?}");
    assert!(
        both.iter()
            .any(|a| a.kind == ArrowKind::Event && a.origin == Origin::Build)
    );
    assert!(
        both.iter()
            .any(|a| a.kind == ArrowKind::Data && a.origin == Origin::Drift)
    );
}

#[test]
fn a_spec_box_whose_code_moved_is_to_build_and_the_code_drift_under_another_id() {
    let mut s = spec();
    for b in s
        .boxes
        .iter_mut()
        .filter(|b| b.id == "web" || b.id.starts_with("web/"))
    {
        b.made_of = vec!["apps/web/nothing/**".into()];
    }
    let m = compare(&s, &facts(), &graph());
    assert_eq!(origin_of(&m, "web"), Origin::Build);
    assert_eq!(origin_of(&m, "web-drift"), Origin::Drift);
    assert_eq!(m.boxes.iter().filter(|b| b.id == "web").count(), 1);
}

#[test]
fn files_left_in_a_partly_claimed_box_are_drift_by_their_folder() {
    // api has no `at` of its own and infra-prisma claims only client.ts: order-repo.ts is in no
    // box, and no fact box holds only unclaimed files
    let mut s = spec();
    for b in s.boxes.iter_mut() {
        match b.id.as_str() {
            "api" => b.made_of = vec![],
            "api/infra-prisma" => b.made_of = vec!["apps/api/src/infra/prisma/client.ts".into()],
            _ => {}
        }
    }
    let m = compare(&s, &facts(), &graph());
    assert_eq!(origin_of(&m, "api"), Origin::Built);
    let d = m.boxes.iter().find(|b| b.origin == Origin::Drift).unwrap();
    assert_eq!(d.nodes, ["apps/api/src/infra/prisma/order-repo.ts"]);
    assert_eq!(d.parent.as_deref(), Some("system"));
    let p = plan(&m);
    assert!(
        p.drift
            .contains(&"drift: code under apps/api/src/infra/prisma/** is in no box".into()),
        "{p:?}"
    );
}

#[test]
fn a_box_with_no_files_and_only_unbuilt_children_is_to_build() {
    let mut w = spec_files();
    w.insert(
        "ig/ops.kdl".into(),
        "worker \"ops\" title=\"Ops\" {\n  worker \"ops/jobs\" title=\"Jobs\" {\n    at \"ops/**\"\n  }\n}\n".into(),
    );
    let m = compare(&read_files(&w), &facts(), &graph());
    assert_eq!(origin_of(&m, "ops"), Origin::Build);
    assert_eq!(origin_of(&m, "ops/jobs"), Origin::Build);
    assert!(
        plan(&m).build.contains(&"to build: worker ops".into()),
        "{:?}",
        plan(&m)
    );
}

#[test]
fn an_outside_system_missing_from_the_spec_is_drift() {
    let mut s = with_billing();
    s.boxes.retain(|b| b.id != "postgres");
    s.arrows.retain(|a| a.to != "postgres");
    let m = compare(&s, &facts(), &graph());
    assert_eq!(origin_of(&m, "postgres"), Origin::Drift);
    assert!(
        plan(&m)
            .drift
            .contains(&"drift: store postgres is in the code, not in ig/".into())
    );
    assert_eq!(arrow_origin(&m, "api", "postgres"), Origin::Drift);
    assert_eq!(
        origin_of(&m, "stripe"),
        Origin::Built,
        "a declaration is built"
    );
}

#[test]
fn origin_is_not_written_when_built() {
    let m = compare(&spec(), &facts(), &graph());
    let j = serde_json::to_string(&m).unwrap();
    assert!(!j.contains("origin"));
    let w = compare(&without(spec(), "web"), &facts(), &graph());
    assert!(
        serde_json::to_string(&w)
            .unwrap()
            .contains("\"origin\":\"drift\"")
    );
}
