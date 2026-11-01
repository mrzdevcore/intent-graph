//! Reads a Traefik dynamic config file in the repo. Only each router's `rule` and `service` and
//! each service's server urls are deserialized: middlewares and the rest are never looked at.

use ig_core::ports::{
    Connector, Files, Link, PortError, Resource, ResourceId, ResourceKind, Settings, Snapshot,
    State, Target,
};
use serde::Deserialize;
use std::collections::BTreeMap;

pub struct TraefikConnector;

#[derive(Deserialize)]
struct Doc {
    http: Option<Http>,
}

#[derive(Deserialize)]
struct Http {
    #[serde(default)]
    routers: BTreeMap<String, Router>,
    #[serde(default)]
    services: BTreeMap<String, Service>,
}

#[derive(Deserialize)]
struct Router {
    rule: Option<String>,
    service: Option<String>,
}

#[derive(Deserialize)]
struct Service {
    #[serde(rename = "loadBalancer")]
    load_balancer: Option<Balancer>,
}

#[derive(Deserialize)]
struct Balancer {
    #[serde(default)]
    servers: Vec<Server>,
}

#[derive(Deserialize)]
struct Server {
    url: Option<String>,
}

/// The first `Host(`name`)` of a rule.
fn host(rule: &str) -> Option<&str> {
    let rest = &rule[rule.find("Host(`")? + 6..];
    Some(&rest[..rest.find('`')?])
}

/// The host of a server url: scheme, userinfo, port, path, query and fragment cut off, an
/// IPv6 host without its brackets.
fn url_host(url: &str) -> &str {
    let rest = url.split_once("://").map_or(url, |(_, r)| r);
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    let host = authority.rsplit_once('@').map_or(authority, |(_, h)| h);
    match host.strip_prefix('[') {
        Some(v6) => v6.split(']').next().unwrap_or(""),
        None => host.split(':').next().unwrap_or(""),
    }
}

fn relative(file: &str) -> bool {
    !file.is_empty() && !file.starts_with('/') && !file.split('/').any(|p| p == "..")
}

impl Connector for TraefikConnector {
    fn kind(&self) -> &'static str {
        "traefik"
    }
    fn read(&self, settings: &Settings, files: &dyn Files) -> Result<Snapshot, PortError> {
        let file = settings.get("file").map(String::as_str).unwrap_or("");
        if !relative(file) {
            return Err(PortError::Io(format!(
                "traefik: file {file:?} must be relative, with no .."
            )));
        }
        let broken = || PortError::Io(format!("traefik {file}: could not read it"));
        let doc: Doc =
            serde_yaml_ng::from_str(&files.read(file).ok_or_else(broken)?).map_err(|_| broken())?;
        let http = doc.http.unwrap_or(Http {
            routers: BTreeMap::new(),
            services: BTreeMap::new(),
        });
        let id = |name: &str| ResourceId {
            connector: "traefik".into(),
            remote: file.into(),
            name: name.into(),
        };
        let mut snap = Snapshot::default();
        snap.resources.push(Resource {
            id: id("traefik"),
            title: "traefik".into(),
            kind: ResourceKind::Service,
            parent: None,
            state: State::Unknown,
            addresses: vec![],
        });
        for (name, router) in &http.routers {
            // `@internal` is Traefik's own (dashboard, API), nothing anyone runs
            let Some(service) = (router.service.as_deref()).filter(|s| !s.ends_with("@internal"))
            else {
                continue;
            };
            // another provider's has no servers here: its route is looked up by the router's name
            let svc = (http.services.get(service)).filter(|_| !service.contains('@'));
            snap.resources.push(Resource {
                id: id(name),
                title: router.rule.as_deref().and_then(host).unwrap_or(name).into(),
                kind: ResourceKind::Route,
                parent: Some(id("traefik")),
                state: State::Unknown,
                addresses: vec![],
            });
            // ponytail: the first server stands for the service; balancing across the rest is not drawn.
            let server = svc
                .iter()
                .flat_map(|s| &s.load_balancer)
                .flat_map(|b| &b.servers)
                .find_map(|s| s.url.as_deref())
                .map(url_host)
                .filter(|h| !h.is_empty());
            let is_ip =
                |h: &&str| h.contains(':') || h.chars().all(|c| c.is_ascii_digit() || c == '.');
            let address = server.filter(is_ip).map(String::from);
            let mut names: Vec<String> = server
                .filter(|h| !is_ip(h))
                .map(String::from)
                .into_iter()
                .collect();
            names.push(name.clone());
            snap.links.push(Link {
                from: id(name),
                to: Target::Lookup { address, names },
                label: None,
            });
        }
        Ok(snap)
    }
}

#[cfg(test)]
mod tests;
