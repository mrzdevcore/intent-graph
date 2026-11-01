use super::*;
use crate::map::origin::plan_of;
use crate::map::spec;
use ig_core::ports::{Link, PortError, Settings};
use ig_core::testing::MemFiles;

mod routes;

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
        kind: "incus".into(),
        remote: Some(remote.into()),
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
    let (kind, remote) = label.split_once(' ').unwrap();
    ConnectorRead {
        label: label.into(),
        kind: kind.into(),
        remote: Some(remote.into()),
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
        kind: "traefik".into(),
        remote: Some("t.yml".into()),
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
    // only what runs is "not in ig/", not the group ig made to hold it
    assert_eq!(group.origin, Origin::Built);
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
        result: Err("incus homelab: could not read the instance list".into()),
        ..failed("incus homelab")
    };
    let out = apply(&mut m, &[read]);
    assert_eq!(
        out.notices,
        ["incus homelab: could not read the instance list"]
    );
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
    let said = std::sync::Arc::new(std::sync::Mutex::new(vec![]));
    let heard = said.clone();
    let progress = move |line: &str| heard.lock().unwrap().push(line.to_string());
    let reads = read_all(
        &config,
        &[&incus, &traefik],
        &MemFiles::with(&[]),
        &progress,
    );
    assert_eq!(
        *said.lock().unwrap(),
        ["reading incus homelab…", "reading traefik t.yml…"]
    );
    assert_eq!(reads.len(), 3);
    assert_eq!(reads[0].label, "incus homelab");
    assert_eq!(reads[0].kind, "incus");
    assert_eq!(reads[0].remote.as_deref(), Some("homelab"));
    assert_eq!(reads[0].result, Ok(Snapshot::default()));
    let skipped = "nope x: no connector of that kind; skipped".to_string();
    assert_eq!(reads[1].result, Err(skipped));
    assert_eq!(reads[2].label, "traefik t.yml");
    assert_eq!(reads[2].result, Err("traefik t.yml: broken".to_string()));
}

#[test]
fn a_box_that_can_run_matches_by_name_at_any_level() {
    for kind in ["service", "app", "store", "queue", "job"] {
        let mut m = map(&format!("{kind} \"redis\" title=\"R\""));
        apply(&mut m, &[lab("redis", State::Running)]);
        assert_eq!(only(&m, "redis"), (MarkState::Ok, None), "{kind}");
        assert!(!has(&m, "homelab/redis"), "{kind}");
    }
    let mut m = map("service \"a\" title=\"A\" {\n store \"a/redis\" title=\"R\"\n}");
    apply(&mut m, &[lab("redis", State::Stopped)]);
    assert_eq!(find(&m, "a/redis").level, Level::Component);
    assert_eq!(only(&m, "a/redis"), (MarkState::Down, None));
}

#[test]
fn a_box_that_is_no_server_neither_matches_nor_blocks_a_match() {
    for kind in ["person", "external", "group"] {
        let other = format!("{kind} \"redis\" title=\"R\"");
        let mut m = map(&other);
        let out = apply(&mut m, &[lab("redis", State::Running)]);
        assert!(marks(&m, "redis").is_empty(), "{kind}");
        assert!(out.warnings.is_empty(), "{kind}: {:?}", out.warnings);
        assert_eq!(find(&m, "homelab/redis").origin, Origin::Unmanaged);

        let mut m = map(&format!(
            "{other}\ngroup \"services\" title=\"S\" {{\n service \"services/redis\" title=\"R\"\n}}"
        ));
        let out = apply(&mut m, &[lab("redis", State::Running)]);
        assert_eq!(only(&m, "services/redis"), (MarkState::Ok, None), "{kind}");
        assert!(marks(&m, "redis").is_empty(), "{kind}");
        assert!(out.warnings.is_empty(), "{kind}: {:?}", out.warnings);
        assert!(!has(&m, "homelab/redis"), "{kind}");
    }
}

#[test]
fn a_code_component_does_not_make_a_match_ambiguous() {
    let mut m = map(
        "service \"a\" title=\"A\" {\n module \"a/redis\" title=\"M\"\n}\nservice \"redis\" title=\"R\"",
    );
    let out = apply(&mut m, &[lab("redis", State::Running)]);
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    assert_eq!(only(&m, "redis"), (MarkState::Ok, None));
    assert!(marks(&m, "a/redis").is_empty());
}

#[test]
fn runs_works_on_a_box_of_any_kind() {
    let mut m = map("person \"me\" title=\"M\" runs=\"incus:homelab/gitlab\"");
    apply(&mut m, &[lab("gitlab", State::Running)]);
    assert_eq!(only(&m, "me"), (MarkState::Ok, None));
}

/// `ig.yaml`'s connectors read by `connector`.
fn read(yaml: &str, connector: &Fake) -> Vec<ConnectorRead> {
    let config = ig_core::config::Config::parse(yaml).unwrap();
    read_all(&config, &[connector], &MemFiles::with(&[]), &|_| {})
}

const RUNS_GITLAB: &str = "service \"web\" title=\"W\" runs=\"incus:homelab/gitlab\"";

#[test]
fn an_extra_setting_does_not_change_what_a_read_covers() {
    let yaml = "connectors:\n  - { kind: incus, remote: homelab, note: lab }\n";

    let down = Fake("incus", Err(PortError::Io("no route to host".into())));
    let reads = read(yaml, &down);
    let mut m = map(RUNS_GITLAB);
    let out = apply(&mut m, &reads);
    assert_eq!(out.notices.len(), 1);
    assert!(marks(&m, "web").is_empty(), "{:?}", m.marks);

    let up = Fake("incus", Ok(Snapshot::default()));
    let reads = read(yaml, &up);
    let mut m = map(RUNS_GITLAB);
    apply(&mut m, &reads);
    assert_eq!(
        only(&m, "web"),
        (MarkState::Warn, Some("not found on homelab".into()))
    );
}

#[test]
fn a_skipped_kind_says_nothing() {
    let up = Fake("traefik", Ok(Snapshot::default()));
    let reads = read(
        "connectors:\n  - { kind: incus, remote: homelab }\n  - { kind: traefik, file: t.yml }\n",
        &up,
    );
    let mut m = map(RUNS_GITLAB);
    let out = apply(&mut m, &reads);
    assert_eq!(out.notices.len(), 1, "{:?}", out.notices);
    assert!(marks(&m, "web").is_empty(), "{:?}", m.marks);
}

#[test]
fn only_a_read_of_that_kind_and_remote_can_say_not_found() {
    // another remote, and another kind on that remote: nothing looked there
    let mut m = map("service \"web\" title=\"W\" runs=\"incus:homelab/x traefik:lab2/x\"");
    apply(&mut m, &[incus("lab2", vec![])]);
    assert!(marks(&m, "web").is_empty(), "{:?}", m.marks);

    // a file is the remote of a connector that reads one
    let up = Fake("traefik", Ok(Snapshot::default()));
    let reads = read("connectors:\n  - { kind: traefik, file: t.yml }\n", &up);
    let mut m = map("service \"web\" title=\"W\" runs=\"traefik:t.yml/gone\"");
    apply(&mut m, &reads);
    assert_eq!(
        only(&m, "web"),
        (MarkState::Warn, Some("not found on t.yml".into()))
    );
}
