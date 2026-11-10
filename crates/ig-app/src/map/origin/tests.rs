use super::*;
use crate::map::file::{MapFile, refresh};
use crate::map::spec::{read, write};
use crate::map::{FactBox, Level, arrows};
use ig_core::model::Graph;
use ig_core::testing::MemFiles;
use std::collections::BTreeMap;

mod moves;

#[test]
fn an_arrow_touching_a_box_that_runs_is_a_declaration() {
    let compared = |runs: &str| {
        let text = format!(
            "system \"s\" title=\"S\" {{\n service \"a\" title=\"A\"{runs} {{\n calls \"b\"\n }}\n service \"b\" title=\"B\"\n}}\n"
        );
        let m = crate::map::spec::parse(vec![("ig/system.kdl".into(), Some(text))]).unwrap();
        compare(&m, &[], &Graph::empty(), None)
    };
    let m = compared(" runs=\"incus:h/a\"");
    assert_eq!(m.arrows[0].origin, Origin::Built);
    assert!(plan(&m).is_empty());
    assert_eq!(compared("").arrows[0].origin, Origin::Build);
}

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
    let m = compare(&spec(), &facts(), &graph(), None);
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
            moves: vec![],
            drift: vec![],
            not_read: vec![],
        }
    );
}

#[test]
fn a_planned_box_and_its_arrow_are_to_build() {
    let m = compare(&with_billing(), &facts(), &graph(), None);
    assert_eq!(origin_of(&m, "billing"), Origin::Build);
    assert_eq!(arrow_origin(&m, "billing", "stripe"), Origin::Build);
    let p = plan(&m);
    assert!(
        p.build
            .contains(&"to build: service billing (billing/**)".into()),
        "{p:?}"
    );
    assert!(
        p.build.iter().any(|l| l.contains("billing calls stripe")),
        "{p:?}"
    );
    assert!(p.drift.is_empty(), "{p:?}");
}

#[test]
fn code_in_no_box_and_unlisted_arrows_are_drift() {
    let m = compare(&without(spec(), "web"), &facts(), &graph(), None);
    assert_eq!(origin_of(&m, "web"), Origin::Drift);
    assert_eq!(arrow_origin(&m, "web", "api"), Origin::Drift);
    assert!(
        !m.boxes.iter().any(|b| b.id.starts_with("web/")),
        "only the highest"
    );
    let p = plan(&m);
    assert!(
        p.drift
            .contains(&"drift: code under apps/web/** is in no box (web)".into()),
        "{p:?}"
    );
    assert!(
        p.drift
            .iter()
            .any(|l| l == "drift: web calls api, not in ig/ (web->api)"),
        "{p:?}"
    );
    let mut sorted = p.drift.clone();
    sorted.sort();
    assert_eq!(p.drift, sorted);
}

#[test]
fn the_deepest_box_claims_a_file() {
    let m = compare(&spec(), &facts(), &graph(), None);
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
    let m = compare(&without_children(spec(), "api"), &facts(), &graph(), None);
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
    a.kind = ArrowKind::Sends;
    a.kinds = vec![ArrowKind::Sends];
    let m = compare(&s, &facts(), &graph(), None);
    assert_eq!(arrow_origin(&m, "api", "stripe"), Origin::Drift);
    let both: Vec<_> = m
        .arrows
        .iter()
        .filter(|a| a.from == "api" && a.to == "postgres")
        .collect();
    assert_eq!(both.len(), 2, "{both:#?}");
    assert!(
        both.iter()
            .any(|a| a.kind == ArrowKind::Sends && a.origin == Origin::Build)
    );
    assert!(
        both.iter()
            .any(|a| a.kind == ArrowKind::Reads && a.origin == Origin::Drift)
    );
    let drift = both.iter().find(|a| a.origin == Origin::Drift).unwrap();
    assert_eq!(drift.id, "api->postgres~drift");
    let mut ids: Vec<_> = m.arrows.iter().map(|a| a.id.as_str()).collect();
    ids.sort();
    let n = ids.len();
    ids.dedup();
    assert_eq!(ids.len(), n, "arrow ids are unique");
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
    let m = compare(&s, &facts(), &graph(), None);
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
    let m = compare(&s, &facts(), &graph(), None);
    assert_eq!(origin_of(&m, "api"), Origin::Built);
    let d = m.boxes.iter().find(|b| b.origin == Origin::Drift).unwrap();
    assert_eq!(d.nodes, ["apps/api/src/infra/prisma/order-repo.ts"]);
    assert_eq!(d.parent.as_deref(), Some("system"));
    let p = plan(&m);
    assert!(
        p.drift
            .contains(&"drift: code under apps/api/src/infra/prisma/order-repo.ts is in no box (apps-api-src-infra-prisma-order-repo-ts)".into()),
        "{p:?}"
    );
}

#[test]
fn drift_ids_stay_valid_slugs_when_long_and_de_clashed() {
    // two loose files whose slugs agree for 64 chars: both fit the limit, the second de-clashed
    let mut s = spec();
    for b in s.boxes.iter_mut() {
        match b.id.as_str() {
            "api" => b.made_of = vec![],
            "api/infra-prisma" => b.made_of = vec!["apps/api/src/infra/prisma/client.ts".into()],
            _ => {}
        }
    }
    let long = "w".repeat(90);
    let mut g = graph();
    let base = (g.nodes.iter())
        .find(|n| n.file == "apps/api/src/infra/prisma/order-repo.ts" && n.is_module())
        .unwrap()
        .clone();
    for n in ["a", "b"] {
        let file = format!("apps/api/src/infra/prisma/{long}{n}.ts");
        g.nodes.push(ig_core::model::Node {
            id: file.clone(),
            file,
            ..base.clone()
        });
    }
    let m = compare(&s, &facts(), &g, None);
    let ids: Vec<&str> = (m.boxes.iter())
        .filter(|b| b.origin == Origin::Drift && b.nodes.iter().any(|n| n.contains(&long)))
        .map(|b| b.id.as_str())
        .collect();
    assert_eq!(ids.len(), 2, "{ids:?}");
    assert!(
        ids[0] != ids[1] && ids.iter().all(|i| i.len() <= 64),
        "{ids:?}"
    );
    crate::map::file::validate(&m).unwrap();
}

#[test]
fn a_box_with_no_files_and_only_unbuilt_children_is_to_build() {
    let mut w = spec_files();
    w.insert(
        "ig/ops.kdl".into(),
        "worker \"ops\" title=\"Ops\" {\n  worker \"ops/jobs\" title=\"Jobs\" {\n    at \"ops/**\"\n  }\n}\n".into(),
    );
    let m = compare(&read_files(&w), &facts(), &graph(), None);
    assert_eq!(origin_of(&m, "ops"), Origin::Build);
    assert_eq!(origin_of(&m, "ops/jobs"), Origin::Build);
    assert!(
        plan(&m).build.contains(&"to build: service ops".into()),
        "{:?}",
        plan(&m)
    );
}

#[test]
fn an_outside_system_missing_from_the_spec_is_drift() {
    let mut s = with_billing();
    s.boxes.retain(|b| b.id != "postgres");
    s.arrows.retain(|a| a.to != "postgres");
    let m = compare(&s, &facts(), &graph(), None);
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
    let m = compare(&spec(), &facts(), &graph(), None);
    let j = serde_json::to_string(&m).unwrap();
    assert!(!j.contains("origin"));
    let w = compare(&without(spec(), "web"), &facts(), &graph(), None);
    assert!(
        serde_json::to_string(&w)
            .unwrap()
            .contains("\"origin\":\"drift\"")
    );
}

#[test]
fn a_box_with_no_files_whose_children_are_declarations_is_built() {
    let mut w = spec_files();
    w.insert(
        "ig/vendors.kdl".into(),
        "external \"vendors\" title=\"Vendors\" {\n  external \"vendors/mail\" title=\"Mail\"\n  external \"vendors/sms\" title=\"Sms\"\n}\n".into(),
    );
    let m = compare(&read_files(&w), &facts(), &graph(), None);
    assert_eq!(origin_of(&m, "vendors"), Origin::Built);
    assert_eq!(origin_of(&m, "vendors/mail"), Origin::Built);
    assert!(plan(&m).build.is_empty(), "{:?}", plan(&m));
}

/// The spec with `api` holding no `at` of its own and without `api/domain-order`: that
/// component's code is in no box.
fn without_a_component() -> MapFile {
    let mut s = without(spec(), "api/domain-order");
    for b in s.boxes.iter_mut().filter(|b| b.id == "api") {
        b.made_of = vec![];
    }
    s
}

#[test]
fn a_drift_component_comes_back_as_a_container_with_a_slug_id() {
    let m = compare(&without_a_component(), &facts(), &graph(), None);
    let d = m.boxes.iter().find(|b| b.origin == Origin::Drift).unwrap();
    assert_eq!(d.id, "api-domain-order");
    assert_eq!(d.level, Level::Container);
    assert_eq!(d.parent.as_deref(), Some("system"));
}

#[test]
fn every_drift_fixture_is_a_valid_map() {
    let mut moved = spec();
    for b in moved
        .boxes
        .iter_mut()
        .filter(|b| b.id == "web" || b.id.starts_with("web/"))
    {
        b.made_of = vec!["apps/web/nothing/**".into()];
    }
    let mut partly = spec();
    for b in partly.boxes.iter_mut() {
        match b.id.as_str() {
            "api" => b.made_of = vec![],
            "api/infra-prisma" => b.made_of = vec!["apps/api/src/infra/prisma/client.ts".into()],
            _ => {}
        }
    }
    let mut no_pg = spec();
    no_pg.boxes.retain(|b| b.id != "postgres");
    no_pg.arrows.retain(|a| a.to != "postgres");
    let mut kinds = spec();
    for a in kinds.arrows.iter_mut().filter(|a| a.id == "api->postgres") {
        a.kind = ArrowKind::Sends;
    }
    for (name, s) in [
        ("without web", without(spec(), "web")),
        ("moved", moved),
        ("partly", partly),
        ("no postgres", no_pg),
        ("component", without_a_component()),
        ("kinds", kinds),
    ] {
        let m = compare(&s, &facts(), &graph(), None);
        assert!(m.boxes.iter().any(|b| b.origin == Origin::Drift) || name == "kinds");
        if let Err(e) = crate::map::file::validate(&m) {
            panic!("{name}: {e:?}");
        }
    }
}

fn spec_arrow(from: &str, kind: ArrowKind, to: &str) -> MapArrow {
    MapArrow {
        note: None,
        context: None,
        id: format!("{from}->{to}"),
        parent: "system".into(),
        from: from.into(),
        to: to.into(),
        kind,
        label: None,
        count: 0,
        kinds: vec![kind],
        in_loop: false,
        origin: Origin::Built,
        parts: vec![],
    }
}

#[test]
fn an_arrow_from_a_person_is_a_declaration() {
    let mut w = spec_files();
    w.insert(
        "ig/patient.kdl".into(),
        "person \"patient\" title=\"Patient\" {\n  uses \"web\"\n}\n".into(),
    );
    let m = compare(&read_files(&w), &facts(), &graph(), None);
    assert_eq!(arrow_origin(&m, "patient", "web"), Origin::Built);
    assert!(plan(&m).build.is_empty(), "{:?}", plan(&m));
}

#[test]
fn a_bundle_arrow_in_the_spec_matches_any_code_arrow_between_the_pair() {
    let mut s = spec();
    for a in s.arrows.iter_mut().filter(|a| a.id == "api->postgres") {
        a.kind = ArrowKind::Bundle;
        a.kinds = vec![ArrowKind::Bundle];
    }
    let m = compare(&s, &facts(), &graph(), None);
    assert_eq!(arrow_origin(&m, "api", "postgres"), Origin::Built);
    assert!(plan(&m).drift.is_empty(), "{:?}", plan(&m));
}

#[test]
fn a_cross_level_arrow_is_built_by_code_under_its_ends() {
    let s = spec();
    let built = compare(&s, &facts(), &graph(), None);
    let owner = |file: &str| {
        let b = built
            .boxes
            .iter()
            .find(|b| b.nodes.iter().any(|n| n == file));
        b.unwrap().id.clone()
    };
    let checkout = owner("apps/web/pages/checkout.tsx");
    assert!(checkout.starts_with("web/"), "{checkout}");
    let other = (built.boxes.iter())
        .find(|b| b.parent.as_deref() == Some("api"))
        .map(|b| b.id.clone())
        .unwrap();
    let mut s = s;
    s.arrows
        .push(spec_arrow(&checkout, ArrowKind::Calls, "api-client"));
    s.arrows
        .push(spec_arrow(&other, ArrowKind::Calls, "api-client"));
    let m = compare(&s, &facts(), &graph(), None);
    assert_eq!(arrow_origin(&m, &checkout, "api-client"), Origin::Built);
    assert_eq!(arrow_origin(&m, &other, "api-client"), Origin::Build);
    assert!(plan(&m).drift.is_empty(), "{:?}", plan(&m));
}

#[test]
fn a_spec_arrow_to_its_own_container_covers_no_code_inside_it() {
    // a component's arrow to its own container is not satisfied by, and covers none of, the
    // code between that component and its siblings
    let s = spec();
    let built = compare(&s, &facts(), &graph(), None);
    let a = (built.arrows.iter())
        .find(|a| a.parent == "api" && a.from.starts_with("api/"))
        .unwrap()
        .clone();
    let mut s = s;
    s.arrows.retain(|x| x.id != a.id);
    s.arrows.push(spec_arrow(&a.from, a.kind, "api"));
    let m = compare(&s, &facts(), &graph(), None);
    assert!(
        m.arrows
            .iter()
            .any(|x| x.id == a.id && x.origin == Origin::Drift)
    );
    assert_eq!(arrow_origin(&m, &a.from, "api"), Origin::Build);
}

fn pieces(claimed: &[&str], left: &[&str]) -> Vec<(String, Vec<String>)> {
    let c: BTreeSet<&str> = claimed.iter().copied().collect();
    left_over(&c, left)
        .into_iter()
        .map(|(id, _, at, _)| (id, at))
        .collect()
}

#[test]
fn left_over_takes_the_highest_folder_without_claimed_code() {
    let claimed = ["apps/api/src/a.ts", "apps/web/b.ts"];
    // a claimed sibling: the folder under it
    assert_eq!(
        pieces(&claimed, &["apps/api/scripts/seed.ts"]),
        [(
            "apps/api/scripts".into(),
            vec!["apps/api/scripts/**".into()]
        )]
    );
    // a fresh folder under apps/
    assert_eq!(
        pieces(&claimed, &["apps/admin/x.ts", "apps/admin/y/z.ts"]),
        [("apps/admin".into(), vec!["apps/admin/**".into()])]
    );
    // no claimed code in lib
    assert_eq!(
        pieces(&claimed, &["lib/c.ts"]),
        [("lib".into(), vec!["lib/**".into()])]
    );
    // never `**`
    assert!(
        pieces(&[], &["lib/c.ts", "tools/d.ts"])
            .iter()
            .all(|p| p.1[0] != "**")
    );
}

#[test]
fn left_over_loose_files_are_listed_unless_the_folder_has_none_claimed() {
    let claimed = ["apps/api/src/a.ts", "apps/api/main.ts", "top.ts"];
    // apps/api holds claimed main.ts: the loose file is named
    assert_eq!(
        pieces(&claimed, &["apps/api/seed.ts"]),
        [("apps/api/seed.ts".into(), vec!["apps/api/seed.ts".into()])]
    );
    // apps/ has claimed code below but no claimed direct file: one glob
    assert_eq!(
        pieces(&claimed, &["apps/x.ts", "apps/y.ts"]),
        [("apps".into(), vec!["apps/*".into()])]
    );
    // the repo root, with a claimed root file
    assert_eq!(
        pieces(&claimed, &["other.ts"]),
        [("other.ts".into(), vec!["other.ts".into()])]
    );
    // the repo root, with none claimed
    assert_eq!(
        pieces(&["apps/api/src/a.ts"], &["other.ts", "more.ts"]),
        [("root-files".into(), vec!["*".into()])]
    );
}

#[test]
fn slug_ids_and_escaped_globs() {
    assert_eq!(slug("apps/web/Button.tsx", "drift"), "apps-web-button-tsx");
    assert_eq!(slug("apps/web/[id].tsx", "drift"), "apps-web-id-tsx");
    assert_eq!(slug("...", "drift"), "drift");
    let c: BTreeSet<&str> = ["apps/api/a.ts"].into();
    let p = left_over(&c, &["apps/api/[slug]/x.ts", "apps/api/.env.ts"]);
    assert_eq!(p[0].2, ["apps/api/[[]slug[]]/**"]);
    // loose and claimed sibling: by path, escaped; the title drops only the last extension
    let c: BTreeSet<&str> = ["apps/api/a.ts"].into();
    let p = left_over(&c, &["apps/api/.env.ts"]);
    assert_eq!(
        (p[0].1.as_str(), p[0].2[0].as_str()),
        (".env", "apps/api/.env.ts")
    );
}
