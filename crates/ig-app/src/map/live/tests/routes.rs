//! Traefik's routes: where they sit and what they reach.

use super::*;
use crate::map::arrows::lift;
use std::collections::HashMap;

#[test]
fn a_route_resolves_by_address_then_by_name() {
    let mut m = map(GITLAB);
    apply(
        &mut m,
        &[
            incus(
                "homelab",
                vec![
                    at("homelab", "gitlab", State::Running, &["10.0.0.5"]),
                    // stopped: no addresses
                    instance("homelab", "wiki", State::Stopped),
                ],
            ),
            traefik(&[
                // the address wins over a name that is also there
                ("by-address", Some("10.0.0.5"), &["wiki"]),
                ("by-host", None, &["wiki"]),
                // a stale address falls back on the router's name, the route's own too
                ("wiki", Some("10.0.0.99"), &["wiki"]),
            ]),
        ],
    );
    let a = arrow(&m, "traefik/route-by-address");
    assert_eq!(a.to, "services/gitlab");
    assert_eq!((a.kind, a.origin), (ArrowKind::Calls, Origin::Built));
    assert_eq!(a.kinds, [ArrowKind::Calls]);
    assert_eq!(a.parent, "sys");
    assert_eq!(a.id, "traefik/route-by-address->services/gitlab");
    assert_eq!(arrow(&m, "traefik/route-by-host").to, "homelab/wiki");
    assert_eq!(arrow(&m, "traefik/route-wiki").to, "homelab/wiki");
    let route = find(&m, "traefik/route-by-host");
    assert_eq!(route.props.get("kind").map(String::as_str), Some("route"));
    assert_eq!(route.title.value, "by-host.example.test");
    assert!(marks(&m, "traefik/route-wiki").is_empty());
    // addresses are for matching: they reach neither box, prop, mark nor arrow
    assert!(!serde_json::to_string(&m).unwrap().contains("10.0.0."));
}

#[test]
fn a_route_resolving_to_nothing_warns() {
    let mut m = map(GITLAB);
    apply(
        &mut m,
        &[
            incus("homelab", vec![]),
            traefik(&[("gone", Some("10.0.0.9"), &["gone"])]),
        ],
    );
    assert_eq!(
        only(&m, "traefik/route-gone"),
        (MarkState::Warn, Some("no instance at this address".into()))
    );
    assert!(m.arrows.is_empty());
}

#[test]
fn a_route_is_not_called_stale_when_an_entry_was_skipped() {
    // a typo in `kind`: the instances are out there, on the entry nothing read
    let up = Fake("incus", Ok(Snapshot::default()));
    let mut reads = read(
        "connectors:\n  - { kind: incus, remote: homelab }\n  - { kind: incuss, remote: lab2 }\n",
        &up,
    );
    reads.push(traefik(&[("gone", Some("10.0.0.9"), &["gone"])]));
    let mut m = map(GITLAB);
    let out = apply(&mut m, &reads);
    assert!(marks(&m, "traefik/route-gone").is_empty(), "{:?}", m.marks);
    assert_eq!(out.notices.len(), 1, "{:?}", out.notices);
}

#[test]
fn a_route_is_not_called_stale_when_nothing_reports_addresses() {
    let mut m = map(GITLAB);
    apply(&mut m, &[traefik(&[("gone", Some("10.0.0.9"), &["gone"])])]);
    assert!(marks(&m, "traefik/route-gone").is_empty(), "{:?}", m.marks);
    assert!(has(&m, "traefik/route-gone"));
}

#[test]
fn a_route_never_picks_between_two_instances() {
    let mut m = map(GITLAB);
    apply(
        &mut m,
        &[
            incus(
                "homelab",
                vec![
                    at("homelab", "a", State::Running, &["10.0.0.7"]),
                    at("homelab", "b", State::Running, &["10.0.0.7"]),
                ],
            ),
            incus("lab2", vec![instance("lab2", "c", State::Running)]),
            incus("lab3", vec![instance("lab3", "c", State::Running)]),
            traefik(&[("shared", Some("10.0.0.7"), &["c"])]),
        ],
    );
    assert!(m.arrows.is_empty());
    assert_eq!(only(&m, "traefik/route-shared").0, MarkState::Warn);
}

#[test]
fn routes_sit_in_a_traefik_box_then_a_traefik_instance_then_their_own() {
    let route = [("web", None, &["gitlab"][..])];
    // a box of that name, the instance's state on it; the two `traefik` are no ambiguity
    let mut m = map("group \"edge\" title=\"E\" {\n service \"edge/traefik\" title=\"T\"\n}");
    let out = apply(&mut m, &[lab("traefik", State::Running), traefik(&route)]);
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    assert_eq!(
        find(&m, "edge/traefik/route-web").parent.as_deref(),
        Some("edge/traefik")
    );
    assert_eq!(only(&m, "edge/traefik").0, MarkState::Ok);
    assert!(!has(&m, "homelab/traefik") && !has(&m, "traefik") && !has(&m, "t-yml"));

    // a box saying it runs the instance
    let mut m = map("service \"proxy\" title=\"P\" runs=\"incus:homelab/traefik\"");
    apply(&mut m, &[lab("traefik", State::Stopped), traefik(&route)]);
    assert!(has(&m, "proxy/route-web"));
    assert_eq!(only(&m, "proxy").0, MarkState::Down);

    // an instance of that name, whose state then applies
    let mut m = map(GITLAB);
    apply(&mut m, &[lab("traefik", State::Stopped), traefik(&route)]);
    assert_eq!(
        find(&m, "homelab/traefik/route-web").parent.as_deref(),
        Some("homelab/traefik")
    );
    assert_eq!(only(&m, "homelab/traefik").0, MarkState::Down);

    // neither: a box of its own at the top level, with no mark and no group named for the file
    let mut m = map(GITLAB);
    apply(&mut m, &[traefik(&route)]);
    let own = find(&m, "traefik");
    assert_eq!(own.parent.as_deref(), Some("sys"));
    assert_eq!(own.origin, Origin::Unmanaged);
    assert_eq!(
        find(&m, "traefik/route-web").parent.as_deref(),
        Some("traefik")
    );
    assert_eq!(find(&m, "traefik/route-web").origin, Origin::Built);
    // not in ig/, and nothing to build
    assert!(plan_of(&m.boxes, &m.arrows, &[]).is_empty());
    assert!(marks(&m, "traefik").is_empty());
    assert!(!has(&m, "t-yml"));

    // no routes, nothing to hold
    let mut m = map(GITLAB);
    apply(&mut m, &[traefik(&[])]);
    assert!(!has(&m, "traefik"));
}

#[test]
fn two_boxes_named_traefik_hold_no_routes_and_warn() {
    let mut m = map(
        "group \"a\" title=\"A\" {\n service \"a/traefik\" title=\"T\"\n}\ngroup \"b\" title=\"B\" {\n service \"b/traefik\" title=\"T\"\n}",
    );
    let out = apply(&mut m, &[traefik(&[("web", None, &["web"])])]);
    assert_eq!(out.warnings.len(), 1, "{:?}", out.warnings);
    assert!(out.warnings[0].contains("a/traefik") && out.warnings[0].contains("b/traefik"));
    assert!(has(&m, "traefik/route-web"));
}

#[test]
fn route_ends_are_kept_when_they_cross_groups() {
    let mut m = map(
        "group \"edge\" title=\"E\" {\n service \"edge/traefik\" title=\"T\"\n}\ngroup \"apps\" title=\"A\" {\n service \"apps/a\" title=\"A\"\n service \"apps/b\" title=\"B\"\n}",
    );
    apply(
        &mut m,
        &[
            incus(
                "homelab",
                vec![
                    instance("homelab", "a", State::Running),
                    instance("homelab", "b", State::Running),
                ],
            ),
            traefik(&[("r1", None, &["a"]), ("r2", None, &["b"])]),
        ],
    );
    assert_eq!(arrow(&m, "edge/traefik/route-r1").parent, "sys");
    let parents: HashMap<&str, &str> = (m.boxes.iter())
        .filter_map(|b| Some((b.id.as_str(), b.parent.as_deref()?)))
        .collect();
    let drawn = lift(&parents, m.arrows.clone());
    assert_eq!(drawn.len(), 1, "{drawn:#?}");
    assert_eq!(
        (drawn[0].from.as_str(), drawn[0].to.as_str()),
        ("edge", "apps")
    );
    let parts: Vec<&str> = drawn[0].parts.iter().map(|p| p.id.as_str()).collect();
    assert_eq!(
        parts,
        [
            "edge/traefik/route-r1->apps/a",
            "edge/traefik/route-r2->apps/b"
        ]
    );
}

#[test]
fn a_route_inside_a_group_gets_that_group_as_parent() {
    let mut m = map(
        "group \"apps\" title=\"A\" {\n service \"apps/traefik\" title=\"T\"\n service \"apps/a\" title=\"A\"\n}",
    );
    apply(
        &mut m,
        &[lab("a", State::Running), traefik(&[("r", None, &["a"])])],
    );
    assert_eq!(arrow(&m, "apps/traefik/route-r").parent, "apps");
}

#[test]
fn a_box_that_is_no_server_holds_no_routes() {
    for kind in ["person", "external"] {
        let mut m = map(&format!("{kind} \"traefik\" title=\"T\""));
        let out = apply(&mut m, &[traefik(&[("web", None, &["web"])])]);
        assert!(!has(&m, "traefik/route-web"), "{kind}");
        assert!(has(&m, "traefik-2/route-web"), "{kind}");
        assert!(out.warnings.is_empty(), "{kind}: {:?}", out.warnings);
    }
}
