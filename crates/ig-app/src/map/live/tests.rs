use super::*;
use crate::map::arrows::lift;
use crate::map::origin::plan_of;
use crate::map::spec;
use ig_core::ports::{Link, PortError, Settings};
use ig_core::testing::MemFiles;
use std::collections::HashMap;

fn map(body: &str) -> MapFile {
    let text = format!("system \"sys\" title=\"Sys\" {{\n{body}\n}}\n");
    spec::parse(vec![("ig/system.kdl".into(), Some(text))]).unwrap()
}

fn id(connector: &str, remote: &str, name: &str) -> ResourceId {
    ResourceId {
        connector: connector.into(),
        remote: remote.into(),
        name: name.into(),
    }
}

fn at(remote: &str, name: &str, state: State, addresses: &[&str]) -> Resource {
    Resource {
        id: id("incus", remote, name),
        title: name.into(),
        kind: ResourceKind::Service,
        parent: None,
        state,
        addresses: addresses.iter().map(|a| a.to_string()).collect(),
    }
}

fn instance(remote: &str, name: &str, state: State) -> Resource {
    at(remote, name, state, &[])
}

fn incus(remote: &str, resources: Vec<Resource>) -> ConnectorRead {
    ConnectorRead {
        label: format!("incus {remote}"),
        result: Ok(Snapshot {
            resources,
            links: vec![],
        }),
    }
}

/// `homelab` holding one instance.
fn lab(name: &str, state: State) -> ConnectorRead {
    incus("homelab", vec![instance("homelab", name, state)])
}

fn failed(label: &str) -> ConnectorRead {
    ConnectorRead {
        label: label.into(),
        result: Err("no route to host".into()),
    }
}

/// A Traefik file holding routes as (router name, the address it was given, the names it goes by).
fn traefik(routes: &[(&str, Option<&str>, &[&str])]) -> ConnectorRead {
    let own = id("traefik", "t.yml", "traefik");
    let mut s = Snapshot::default();
    s.resources.push(Resource {
        id: own.clone(),
        title: "traefik".into(),
        kind: ResourceKind::Service,
        parent: None,
        state: State::Unknown,
        addresses: vec![],
    });
    for (name, address, names) in routes {
        let route = id("traefik", "t.yml", name);
        s.resources.push(Resource {
            id: route.clone(),
            title: format!("{name}.example.test"),
            kind: ResourceKind::Route,
            parent: Some(own.clone()),
            state: State::Unknown,
            addresses: vec![],
        });
        s.links.push(Link {
            from: route,
            to: Target::Lookup {
                address: address.map(String::from),
                names: names.iter().map(|n| n.to_string()).collect(),
            },
            label: None,
        });
    }
    ConnectorRead {
        label: "traefik t.yml".into(),
        result: Ok(s),
    }
}

fn find<'a>(m: &'a MapFile, id: &str) -> &'a MapBox {
    (m.boxes.iter().find(|b| b.id == id)).unwrap_or_else(|| panic!("no box {id} in {:#?}", m.boxes))
}

fn has(m: &MapFile, id: &str) -> bool {
    m.boxes.iter().any(|b| b.id == id)
}

fn marks<'a>(m: &'a MapFile, target: &str) -> Vec<&'a Mark> {
    m.marks.iter().filter(|k| k.target == target).collect()
}

/// The one mark on `target`: its layer must be `state`; (state, value).
fn only(m: &MapFile, target: &str) -> (MarkState, Option<String>) {
    let k = marks(m, target);
    assert_eq!(k.len(), 1, "{target}: {:?}", m.marks);
    assert_eq!(k[0].layer, "state");
    (k[0].state, k[0].value.clone())
}

fn arrow<'a>(m: &'a MapFile, from: &str) -> &'a MapArrow {
    (m.arrows.iter().find(|a| a.from == from))
        .unwrap_or_else(|| panic!("no arrow from {from} in {:#?}", m.arrows))
}

const GITLAB: &str =
    "group \"services\" title=\"Services\" {\n service \"services/gitlab\" title=\"GitLab\"\n}";

#[test]
fn a_box_named_like_an_instance_gets_its_state() {
    let mut m = map(GITLAB);
    let boxes = m.boxes.len();
    let out = apply(&mut m, &[lab("gitlab", State::Stopped)]);
    assert_eq!(only(&m, "services/gitlab"), (MarkState::Down, None));
    assert_eq!(m.boxes.len(), boxes);
    assert!(out.notices.is_empty() && out.warnings.is_empty());
}

#[test]
fn an_instance_nothing_describes_is_unmanaged_in_a_group_named_for_its_remote() {
    let mut m = map(GITLAB);
    apply(&mut m, &[lab("redis", State::Running)]);
    let group = find(&m, "homelab");
    assert_eq!(group.kind.value, BoxKind::Group);
    assert_eq!(group.parent.as_deref(), Some("sys"));
    assert_eq!(group.title.value, "homelab");
    assert_eq!(group.origin, Origin::Unmanaged);
    let redis = find(&m, "homelab/redis");
    assert_eq!(redis.origin, Origin::Unmanaged);
    assert_eq!(redis.parent.as_deref(), Some("homelab"));
    assert_eq!(redis.title.value, "redis");
    assert!(!redis.needs_reading);
    assert_eq!(only(&m, "homelab/redis"), (MarkState::Ok, None));
    // not in ig/, and nothing to build
    assert!(plan_of(&m.boxes, &m.arrows, &[]).is_empty());
    assert_eq!(
        serde_json::to_value(Origin::Unmanaged).unwrap(),
        "unmanaged"
    );
}

#[test]
fn a_box_with_no_instance_and_no_runs_gets_no_mark() {
    let mut m = map(GITLAB);
    apply(&mut m, &[lab("redis", State::Running)]);
    assert!(marks(&m, "services/gitlab").is_empty());
}

#[test]
fn no_reads_change_nothing() {
    let mut m = map("service \"web\" title=\"Web\" runs=\"incus:homelab/gitlab\"");
    let before = m.clone();
    let out = apply(&mut m, &[]);
    assert_eq!(m, before);
    assert!(out.notices.is_empty() && out.warnings.is_empty());
}

#[test]
fn runs_finds_its_instance_whatever_the_box_is_called() {
    // and a box saying what it runs takes nothing by name: `redis` here stays unmanaged
    let mut m = map("service \"redis\" title=\"R\" runs=\"incus:homelab/gitlab\"");
    apply(
        &mut m,
        &[incus(
            "homelab",
            vec![
                instance("homelab", "gitlab", State::Running),
                instance("homelab", "redis", State::Stopped),
            ],
        )],
    );
    assert_eq!(only(&m, "redis"), (MarkState::Ok, None));
    assert!(!has(&m, "homelab/gitlab"));
    assert_eq!(only(&m, "homelab/redis"), (MarkState::Down, None));
}

#[test]
fn runs_wins_over_a_box_of_that_name() {
    let mut m = map(&format!(
        "{GITLAB}\nservice \"web\" title=\"W\" runs=\"incus:homelab/gitlab\""
    ));
    apply(&mut m, &[lab("gitlab", State::Running)]);
    assert_eq!(only(&m, "web"), (MarkState::Ok, None));
    assert!(marks(&m, "services/gitlab").is_empty());
}

#[test]
fn runs_naming_nothing_warns_on_the_box() {
    let mut m = map("service \"web\" title=\"W\" runs=\"incus:homelab/gone incus:homelab/lost\"");
    apply(&mut m, &[incus("homelab", vec![])]);
    let (state, value) = only(&m, "web");
    assert_eq!(state, MarkState::Warn);
    assert_eq!(value.as_deref(), Some("not found on homelab"));
}

#[test]
fn runs_listing_several_shows_the_worst_state() {
    let two = |a: State, b: State| {
        let mut m = map("service \"w\" title=\"W\" runs=\"incus:homelab/a incus:homelab/b\"");
        apply(
            &mut m,
            &[incus(
                "homelab",
                vec![instance("homelab", "a", a), instance("homelab", "b", b)],
            )],
        );
        m
    };
    let m = two(State::Running, State::Stopped);
    assert_eq!(
        only(&m, "w"),
        (MarkState::Down, Some("1 of 2 stopped".into()))
    );
    // Unknown never outranks another state
    let m = two(State::Unknown, State::Stopped);
    assert_eq!(
        only(&m, "w"),
        (MarkState::Down, Some("1 of 2 stopped".into()))
    );
    let m = two(State::Unknown, State::Running);
    assert_eq!(
        only(&m, "w"),
        (MarkState::Ok, Some("1 of 2 running".into()))
    );
    let m = two(State::Running, State::Running);
    assert_eq!(
        only(&m, "w"),
        (MarkState::Ok, Some("2 of 2 running".into()))
    );
    assert!(marks(&two(State::Unknown, State::Unknown), "w").is_empty());
}

#[test]
fn two_boxes_with_one_name_match_neither_and_warn() {
    let mut m = map(&format!(
        "{GITLAB}\ngroup \"other\" title=\"O\" {{\n service \"other/gitlab\" title=\"G\"\n}}"
    ));
    let out = apply(&mut m, &[lab("gitlab", State::Running)]);
    assert_eq!(out.warnings.len(), 1, "{:?}", out.warnings);
    assert!(out.warnings[0].contains("services/gitlab"));
    assert!(out.warnings[0].contains("other/gitlab"));
    assert!(marks(&m, "services/gitlab").is_empty());
    assert!(marks(&m, "other/gitlab").is_empty());
    assert_eq!(find(&m, "homelab/gitlab").origin, Origin::Unmanaged);
}

#[test]
fn two_remotes_with_one_name_match_neither_and_warn() {
    let mut m = map(GITLAB);
    let out = apply(
        &mut m,
        &[
            lab("gitlab", State::Running),
            incus("lab2", vec![instance("lab2", "gitlab", State::Stopped)]),
        ],
    );
    assert_eq!(out.warnings.len(), 1, "{:?}", out.warnings);
    assert!(out.warnings[0].contains("services/gitlab"));
    assert!(out.warnings[0].contains("homelab"));
    assert!(out.warnings[0].contains("lab2"));
    assert!(marks(&m, "services/gitlab").is_empty());
    assert_eq!(only(&m, "homelab/gitlab"), (MarkState::Ok, None));
    assert_eq!(only(&m, "lab2/gitlab"), (MarkState::Down, None));
}

#[test]
fn an_unmanaged_group_never_takes_an_existing_id() {
    let mut m = map("service \"homelab\" title=\"H\"");
    apply(&mut m, &[lab("redis", State::Running)]);
    let group = find(&m, "homelab-2");
    assert_eq!(group.kind.value, BoxKind::Group);
    assert_eq!(group.title.value, "homelab");
    assert_eq!(
        find(&m, "homelab-2/redis").parent.as_deref(),
        Some("homelab-2")
    );
    assert_eq!(find(&m, "homelab").kind.value, BoxKind::Service);
    assert_eq!(find(&m, "homelab").origin, Origin::Built);
}

#[test]
fn instances_whose_names_slug_alike_keep_their_own_boxes() {
    let mut m = map(GITLAB);
    apply(
        &mut m,
        &[incus(
            "homelab",
            vec![
                instance("homelab", "a.b", State::Running),
                instance("homelab", "a-b", State::Stopped),
            ],
        )],
    );
    assert_eq!(only(&m, "homelab/a-b"), (MarkState::Ok, None));
    assert_eq!(only(&m, "homelab/a-b-2"), (MarkState::Down, None));
}

#[test]
fn a_failed_connector_concludes_nothing() {
    let mut m = map("service \"web\" title=\"W\" runs=\"incus:homelab/gitlab\"");
    let out = apply(
        &mut m,
        &[
            failed("incus homelab"),
            traefik(&[("gone", Some("10.0.0.9"), &["gone"])]),
        ],
    );
    assert_eq!(out.notices, ["incus homelab: no route to host"]);
    assert!(out.warnings.is_empty());
    assert!(marks(&m, "web").is_empty());
    assert!(!has(&m, "homelab"));
    // the other connector still draws, and its route may point at the one that failed
    assert_eq!(find(&m, "traefik/route-gone").kind.value, BoxKind::Entry);
    assert!(marks(&m, "traefik/route-gone").is_empty());

    // only the one that failed: another remote's result still counts
    let mut m = map("service \"web\" title=\"W\" runs=\"incus:lab2/gone incus:homelab/x\"");
    apply(&mut m, &[failed("incus homelab"), incus("lab2", vec![])]);
    assert_eq!(
        only(&m, "web"),
        (MarkState::Warn, Some("not found on lab2".into()))
    );

    // Traefik failing leaves the instances
    let mut m = map(GITLAB);
    let out = apply(
        &mut m,
        &[failed("traefik t.yml"), lab("gitlab", State::Running)],
    );
    assert_eq!(out.notices.len(), 1);
    assert_eq!(only(&m, "services/gitlab"), (MarkState::Ok, None));
    assert!(!has(&m, "traefik"));
}

#[test]
fn a_notice_does_not_name_its_connector_twice() {
    let mut m = map(GITLAB);
    let read = ConnectorRead {
        label: "incus homelab".into(),
        result: Err("incus homelab: could not read the instance list".into()),
    };
    let out = apply(&mut m, &[read]);
    assert_eq!(
        out.notices,
        ["incus homelab: could not read the instance list"]
    );
}

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
fn parse_runs_refuses_what_it_cannot_read() {
    assert!(parse_runs("gitlab").is_err());
    assert!(parse_runs("incus:homelab").is_err());
    assert_eq!(parse_runs("incus:a/b incus:a/c").unwrap().len(), 2);
    for bad in ["", "  ", ":a/b", "incus:/b", "incus:a/", "incus:a/b nope"] {
        assert!(parse_runs(bad).is_err(), "{bad:?}");
    }
    // a Traefik remote is a path: the name is what follows the last slash
    assert_eq!(
        parse_runs("traefik:conf/t.yml/traefik").unwrap(),
        [id("traefik", "conf/t.yml", "traefik")]
    );
}

struct Fake(&'static str, Result<Snapshot, PortError>);

impl Connector for Fake {
    fn kind(&self) -> &'static str {
        self.0
    }
    fn read(&self, _: &Settings, _: &dyn Files) -> Result<Snapshot, PortError> {
        self.1.clone()
    }
}

#[test]
fn read_all_reads_each_entry_and_names_what_it_skips() {
    let config = ig_core::config::Config::parse(
        "connectors:\n  - { kind: incus, remote: homelab }\n  - { kind: nope, remote: x }\n  - { kind: traefik, file: t.yml }\n",
    )
    .unwrap();
    let incus = Fake("incus", Ok(Snapshot::default()));
    let traefik = Fake(
        "traefik",
        Err(PortError::Io("traefik t.yml: broken".into())),
    );
    let (reads, notices) = read_all(&config, &[&incus, &traefik], &MemFiles::with(&[]));
    assert_eq!(reads.len(), 2);
    assert_eq!(reads[0].label, "incus homelab");
    assert_eq!(reads[0].result, Ok(Snapshot::default()));
    assert_eq!(reads[1].label, "traefik t.yml");
    assert_eq!(reads[1].result, Err("traefik t.yml: broken".to_string()));
    assert_eq!(notices.len(), 1);
    assert!(notices[0].contains("nope x"), "{notices:?}");
}
