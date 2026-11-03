use super::*;
use ig_core::ports::{Resource, ResourceKind, Target};
use ig_core::testing::MemFiles;

const FILE: &str = r#"
http:
  middlewares:
    auth: { basicAuth: { users: ["SECRET_CANARY"] } }
  routers:
    dash: { rule: "Host(`dash.example.test`)", service: api@internal }
    app:  { rule: "Host(`app.example.test`)", service: app-svc }
    wiki-router: { rule: "Host(`wiki.example.test`) && PathPrefix(`/x`)", service: wiki-svc }
    multi: { rule: "Host(`m.example.test`)", service: multi-svc }
  services:
    app-svc:   { loadBalancer: { servers: [{ url: "http://10.0.0.2:80" }] } }
    wiki-svc:  { loadBalancer: { servers: [{ url: "http://wiki:8080" }] } }
    multi-svc: { loadBalancer: { servers: [{ url: "10.0.0.5:80" }, { url: "http://10.0.0.6:80" }] } }
"#;

fn read_at(file: &str, text: &str) -> Result<Snapshot, PortError> {
    let settings = Settings::from([("file".to_string(), file.to_string())]);
    TraefikConnector.read(&settings, &MemFiles::with(&[(file, text)]))
}
fn read_text(text: &str) -> Result<Snapshot, PortError> {
    read_at("t.yml", text)
}
fn read(text: &str) -> Snapshot {
    read_text(text).unwrap()
}
fn read_missing(file: &str) -> Result<Snapshot, PortError> {
    let settings = Settings::from([("file".to_string(), file.to_string())]);
    TraefikConnector.read(&settings, &MemFiles::default())
}
fn routes(s: &Snapshot) -> Vec<&Resource> {
    s.resources
        .iter()
        .filter(|r| r.kind == ResourceKind::Route)
        .collect()
}
fn target(s: &Snapshot, router: &str) -> Target {
    let link = s.links.iter().find(|l| l.from.name == router).unwrap();
    link.to.clone()
}

#[test]
fn a_router_on_another_provider_is_a_route_found_by_its_name() {
    let s = read(
        "http:\n  routers:\n    api: { rule: \"Host(`a.example.test`)\", service: svc@docker }\n    dash: { service: api@internal }\n",
    );
    let titles: Vec<_> = routes(&s).iter().map(|r| r.title.clone()).collect();
    assert_eq!(titles, ["a.example.test"]);
    assert_eq!(
        target(&s, "api"),
        Target::Lookup {
            address: None,
            names: vec!["api".into()]
        }
    );
}

#[test]
fn routers_become_routes_inside_one_service() {
    let s = read(FILE);
    let titles: Vec<_> = routes(&s).iter().map(|r| r.title.clone()).collect();
    assert_eq!(
        titles,
        ["app.example.test", "m.example.test", "wiki.example.test"]
    ); // no api@internal
    assert!(!format!("{s:?}").contains("SECRET_CANARY"));
}
#[test]
fn a_link_looks_up_an_ip_then_names() {
    assert_eq!(
        target(&read(FILE), "app"),
        Target::Lookup {
            address: Some("10.0.0.2".into()),
            names: vec!["app".into()]
        }
    );
    assert_eq!(
        target(&read(FILE), "wiki-router"),
        Target::Lookup {
            address: None,
            names: vec!["wiki".into(), "wiki-router".into()]
        }
    );
}
#[test]
fn the_first_server_stands_for_a_service_and_a_missing_scheme_is_fine() {
    assert_eq!(
        target(&read(FILE), "multi"),
        Target::Lookup {
            address: Some("10.0.0.5".into()),
            names: vec!["multi".into()]
        }
    );
}
#[test]
fn a_bad_path_a_missing_file_and_broken_yaml_are_errors_not_panics() {
    for bad in ["/etc/x", "../x", ""] {
        assert!(read_at(bad, FILE).unwrap_err().to_string().contains("file"));
    }
    assert!(read_missing("t.yml").is_err());
    assert!(read_text("{ not: [yaml").is_err());
}

fn one(url: &str) -> Snapshot {
    read(&format!(
        "http:\n  routers:\n    r: {{ rule: \"Host(`r.example.test`)\", service: s }}\n  services:\n    s: {{ loadBalancer: {{ servers: [{{ url: \"{url}\" }}] }} }}\n"
    ))
}

#[test]
fn userinfo_is_never_the_host_nor_kept() {
    let s = one("http://bob:hunter2@10.0.0.2:80");
    assert_eq!(
        target(&s, "r"),
        Target::Lookup {
            address: Some("10.0.0.2".into()),
            names: vec!["r".into()]
        }
    );
    let s = one("http://bob@wiki:80/p?q=1#f");
    assert_eq!(
        target(&s, "r"),
        Target::Lookup {
            address: None,
            names: vec!["wiki".into(), "r".into()]
        }
    );
    for t in [
        format!("{:?}", one("http://bob:hunter2@10.0.0.2:80")),
        format!("{s:?}"),
    ] {
        assert!(!t.contains("bob") && !t.contains("hunter2"));
    }
}
#[test]
fn an_ipv6_host_is_an_address_and_a_query_is_cut() {
    assert_eq!(
        target(&one("http://[::1]:80"), "r"),
        Target::Lookup {
            address: Some("::1".into()),
            names: vec!["r".into()]
        }
    );
    assert_eq!(
        target(&one("wiki?x=1"), "r"),
        Target::Lookup {
            address: None,
            names: vec!["wiki".into(), "r".into()]
        }
    );
}
