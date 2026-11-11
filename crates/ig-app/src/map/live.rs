//! What connectors read, merged into the map: one box per service. A resource finds its box by
//! `runs=`, else by the exact last part of a box's id, else it is drawn unmanaged. Adds state
//! marks, unmanaged boxes and route arrows, and never re-judges what `compare` said. Addresses
//! are for matching only: nothing here writes one to a box, prop, mark or arrow. Pure: no I/O
//! beyond the connectors `read_all` is handed.

use super::facts::{Guess, dedupe, slug};
use super::file::{MapBox, MapFile, fresh};
use super::layers::{Mark, MarkState, mark, sort_marks};
use super::origin::Origin;
use super::{ArrowKind, BoxKind, FactBox, Level, MapArrow};
use ig_core::config::Config;
use ig_core::ports::{
    Connector, Files, Progress, Resource, ResourceId, ResourceKind, Snapshot, State, Target,
};
use std::collections::{BTreeMap, HashMap};

const LAYER: &str = "state";
const TRAEFIK: &str = "traefik";

/// One `ig.yaml` entry's read. `kind` and `remote` are where it looked (`remote`: its `remote` or
/// `file` setting), and `label`, those two, is what a notice names: no other setting, which could
/// be a secret, reaches one.
#[derive(Debug, Clone, PartialEq)]
pub struct ConnectorRead {
    pub label: String,
    pub kind: String,
    pub remote: Option<String>,
    pub result: Result<Snapshot, String>,
}

/// What `apply` has to say about the reads.
#[derive(Debug, Default)]
pub struct Applied {
    pub notices: Vec<String>,
    pub warnings: Vec<String>,
}

/// One read per `config.connectors` entry, said on `progress` as it starts: a server can take
/// seconds. An unknown `kind` is a failed read, so a notice, and it concludes nothing.
pub fn read_all(
    config: &Config,
    connectors: &[&dyn Connector],
    files: &dyn Files,
    progress: &Progress,
) -> Vec<ConnectorRead> {
    let mut reads = vec![];
    for e in &config.connectors {
        let remote = (e.settings.get("remote").or_else(|| e.settings.get("file"))).cloned();
        let label = match &remote {
            Some(r) => format!("{} {r}", e.kind),
            None => e.kind.clone(),
        };
        let result = match connectors.iter().find(|c| c.kind() == e.kind) {
            Some(c) => {
                progress(&format!("reading {label}…"));
                c.read(&e.settings, files).map_err(|e| e.to_string())
            }
            None => Err(format!("{label}: no connector of that kind; skipped")),
        };
        reads.push(ConnectorRead {
            label,
            kind: e.kind.clone(),
            remote,
            result,
        });
    }
    reads
}

/// A `runs=` value: `<kind>:<remote>/<name>`, separated by spaces. A remote may be a path, so
/// the name is what follows the last slash.
pub fn parse_runs(value: &str) -> Result<Vec<ResourceId>, String> {
    let bad = || format!("`runs` is <kind>:<remote>/<name>, separated by spaces, not `{value}`");
    let ids: Vec<ResourceId> = (value.split_whitespace())
        .map(|one| {
            let (connector, rest) = one.split_once(':')?;
            let (remote, name) = rest.rsplit_once('/')?;
            let parts = [connector, remote, name];
            parts.iter().all(|p| !p.is_empty()).then(|| ResourceId {
                connector: connector.into(),
                remote: remote.into(),
                name: name.into(),
            })
        })
        .collect::<Option<_>>()
        .ok_or_else(bad)?;
    if ids.is_empty() {
        return Err(bad());
    }
    Ok(ids)
}

/// The boxes resources have found so far.
#[derive(Default)]
struct Found<'r> {
    /// Box id → the resources it stands for.
    on: BTreeMap<String, Vec<&'r Resource>>,
    /// Resource → its box; the first, when several boxes `runs=` one resource.
    home: HashMap<&'r ResourceId, String>,
}

impl<'r> Found<'r> {
    fn add(&mut self, id: &str, r: &'r Resource) {
        self.on.entry(id.into()).or_default().push(r);
        self.home.entry(&r.id).or_insert_with(|| id.into());
    }
}

/// The Traefik connector's own resource: it holds routes, and matches no other resource's name.
fn is_own(r: &Resource) -> bool {
    r.id.connector == TRAEFIK && r.id.name == TRAEFIK
}

/// The boxes the name rule can match to `name`: the kinds that run, at any level, and not one
/// that says what it runs.
fn named(m: &MapFile, name: &str) -> Vec<String> {
    use BoxKind::{App, Job, Queue, Service, Store};
    (m.boxes.iter())
        .filter(|b| matches!(b.kind.value, Service | App | Store | Queue | Job))
        .filter(|b| !b.props.contains_key("runs"))
        .filter(|b| b.id.rsplit('/').next() == Some(name))
        .map(|b| b.id.clone())
        .collect()
}

fn word(target: &str, state: MarkState, badge: &str, value: Option<String>) -> Mark {
    let mut k = mark(LAYER, target, state, badge.into());
    k.value = value;
    k
}

/// `runs=` first. What it names and no read has is a warn mark, only when a read succeeded for
/// that kind and remote: nothing else says what runs there.
fn match_runs<'r>(
    m: &mut MapFile,
    services: &[&'r Resource],
    looked: &[(&str, &str)],
    found: &mut Found<'r>,
) {
    let runs: Vec<(String, Vec<ResourceId>)> = (m.boxes.iter())
        .filter_map(|b| Some((b.id.clone(), parse_runs(b.props.get("runs")?).ok()?)))
        .collect();
    for (id, wanted) in &runs {
        for w in wanted {
            if let Some(r) = services.iter().find(|r| r.id == *w) {
                found.add(id, r);
            } else if looked.contains(&(w.connector.as_str(), w.remote.as_str())) {
                let value = format!("not found on {}", w.remote);
                let k = word(id, MarkState::Warn, "not found", Some(value));
                if !m.marks.contains(&k) {
                    m.marks.push(k);
                }
            }
        }
    }
}

/// Then the exact name: more than one candidate on either side is no match, and a warning.
fn match_names<'r>(
    m: &MapFile,
    services: &[&'r Resource],
    found: &mut Found<'r>,
    warnings: &mut Vec<String>,
) {
    let mut by_name: BTreeMap<&str, Vec<&'r Resource>> = BTreeMap::new();
    for r in services
        .iter()
        .filter(|r| !is_own(r) && !found.home.contains_key(&r.id))
    {
        by_name.entry(&r.id.name).or_default().push(r);
    }
    for (name, rs) in by_name {
        let boxes = named(m, name);
        match (&boxes[..], &rs[..]) {
            ([], _) => {}
            ([id], [r]) => found.add(id, r),
            ([id], _) => {
                let on: Vec<String> = rs.iter().map(|r| r.id.remote.clone()).collect();
                warnings.push(format!(
                    "`{name}` is on {}: `{id}` matches none, say which with runs=",
                    on.join(", ")
                ));
            }
            _ => warnings.push(two_boxes(name, &boxes)),
        }
    }
}

fn two_boxes(name: &str, boxes: &[String]) -> String {
    format!(
        "boxes {} are all named `{name}`: none matches, say which with runs=",
        boxes.join(", ")
    )
}

/// The box that holds Traefik's routes, when one matches by the rules above: a box linked to any
/// resource called `traefik`, else the one box of that name.
fn traefik_box(
    m: &MapFile,
    services: &[&Resource],
    found: &Found,
    warnings: &mut Vec<String>,
) -> Option<String> {
    let linked = (services.iter())
        .filter(|r| r.id.name == TRAEFIK)
        .find_map(|r| found.home.get(&r.id).cloned());
    if linked.is_some() || !services.iter().any(|r| is_own(r)) {
        return linked;
    }
    let boxes = named(m, TRAEFIK);
    if boxes.len() > 1 {
        let w = two_boxes(TRAEFIK, &boxes);
        if !warnings.contains(&w) {
            warnings.push(w);
        }
        return None;
    }
    boxes.into_iter().next()
}

/// A box nothing in `ig/` describes, with nothing for an agent to read. `origin`: `Unmanaged` for
/// what runs, `Built` for what ig draws around it, which nobody could add to `ig/`.
fn new_box(
    id: String,
    parent: &str,
    level: Level,
    kind: BoxKind,
    title: &str,
    origin: Origin,
) -> MapBox {
    let mut b = fresh(&FactBox {
        id,
        parent: Some(parent.into()),
        level,
        made_of: vec![],
        package: None,
        files: 0,
        nodes: vec![],
        guess: Guess {
            kind,
            title: title.into(),
            pattern: None,
            slot: None,
            logo: None,
        },
    });
    b.origin = origin;
    b.needs_reading = false;
    b
}

/// `id`, suffixed when a box has it.
fn unique(m: &MapFile, id: String) -> String {
    let taken: Vec<&str> = m.boxes.iter().map(|b| b.id.as_str()).collect();
    dedupe(vec![id], &taken).remove(0)
}

fn root(m: &MapFile) -> String {
    (m.boxes.iter().find(|b| b.level == Level::System))
        .map_or("system".to_string(), |b| b.id.clone())
}

/// What found no box goes in a group named after its remote.
fn unmanaged<'r>(m: &mut MapFile, services: &[&'r Resource], found: &mut Found<'r>) {
    let root = root(m);
    let mut groups: BTreeMap<&str, String> = BTreeMap::new();
    for r in services {
        if is_own(r) || found.home.contains_key(&r.id) {
            continue;
        }
        let remote = r.id.remote.as_str();
        let group = groups.entry(remote).or_insert_with(|| {
            let id = unique(m, slug(remote, "remote"));
            let b = new_box(
                id.clone(),
                &root,
                Level::Container,
                BoxKind::Group,
                remote,
                Origin::Built,
            );
            m.boxes.push(b);
            id
        });
        let id = unique(m, format!("{group}/{}", slug(&r.id.name, "instance")));
        let b = new_box(
            id.clone(),
            group,
            Level::Container,
            BoxKind::Service,
            &r.title,
            Origin::Unmanaged,
        );
        m.boxes.push(b);
        found.add(&id, r);
    }
}

/// Running is ok, stopped is down, `Unknown` says nothing; a box of several shows the worst,
/// with a count.
fn state_mark(id: &str, rs: &[&Resource]) -> Option<Mark> {
    let count = |s: State| rs.iter().filter(|r| r.state == s).count();
    let (stopped, running) = (count(State::Stopped), count(State::Running));
    let (state, badge, n) = match (stopped, running) {
        (0, 0) => return None,
        (0, n) => (MarkState::Ok, "running", n),
        (n, _) => (MarkState::Down, "stopped", n),
    };
    let value = (rs.len() > 1).then(|| format!("{n} of {} {badge}", rs.len()));
    Some(word(id, state, badge, value))
}

/// Routes go inside `holder`, the box that matched, else the one instance called `traefik`, else a box
/// of their own at the top level.
// ponytail: every route sits in Traefik's box, the only connector with routes; use `parent` when a second comes.
fn place_routes<'r>(
    m: &mut MapFile,
    all: &[&'r Resource],
    services: &[&Resource],
    holder: Option<String>,
    found: &mut Found<'r>,
) {
    let routes: Vec<&'r Resource> = (all.iter().copied())
        .filter(|r| r.kind == ResourceKind::Route)
        .collect();
    if routes.is_empty() {
        return;
    }
    let holder = holder
        .or_else(|| {
            match one(services
                .iter()
                .copied()
                .filter(|r| !is_own(r) && r.id.name == TRAEFIK))
            {
                Some(r) => found.home.get(&r.id).cloned(),
                None => None,
            }
        })
        .unwrap_or_else(|| {
            let id = unique(m, TRAEFIK.into());
            let root = root(m);
            let b = new_box(
                id.clone(),
                &root,
                Level::Container,
                BoxKind::Service,
                TRAEFIK,
                Origin::Unmanaged,
            );
            m.boxes.push(b);
            id
        });
    for r in routes {
        let id = unique(m, format!("{holder}/route-{}", slug(&r.id.name, "route")));
        let mut b = new_box(
            id.clone(),
            &holder,
            Level::Component,
            BoxKind::Entry,
            &r.title,
            Origin::Built,
        );
        b.props.insert("kind".into(), "route".into());
        m.boxes.push(b);
        found.home.insert(&r.id, id);
    }
}

/// The only item: two is no match, since a wrong match is worse than none.
fn one<'r>(mut it: impl Iterator<Item = &'r Resource>) -> Option<&'r Resource> {
    let first = it.next()?;
    it.next().is_none().then_some(first)
}

/// A lookup against every snapshot's services: the address, then each name, exact only.
fn resolve<'r>(
    services: &[&'r Resource],
    address: Option<&str>,
    names: &[String],
) -> Option<&'r Resource> {
    let them = || services.iter().copied().filter(|r| !is_own(r));
    address
        .and_then(|a| one(them().filter(|r| r.addresses.iter().any(|x| x == a))))
        .or_else(|| {
            names
                .iter()
                .find_map(|n| one(them().filter(|r| r.id.name == *n)))
        })
}

/// Each link an arrow to where it resolved, its parent the lowest common parent of its ends so
/// that `arrows::lift` keeps it in `parts`; one that resolves nowhere a warn mark, unless `pause`,
/// since the target may be where nothing looked.
fn draw_links(
    m: &mut MapFile,
    live: &[&Snapshot],
    services: &[&Resource],
    found: &Found,
    pause: bool,
) {
    let parents: HashMap<&str, &str> = (m.boxes.iter())
        .filter_map(|b| Some((b.id.as_str(), b.parent.as_deref()?)))
        .collect();
    let above = |id: &str| -> Vec<String> {
        std::iter::successors(parents.get(id).copied(), |p| parents.get(p).copied())
            .take(parents.len())
            .map(String::from)
            .collect()
    };
    for l in live.iter().flat_map(|s| &s.links) {
        let Some(from) = found.home.get(&l.from) else {
            continue;
        };
        let to = match &l.to {
            Target::Resource(id) => found.home.get(id),
            Target::Lookup { address, names } => {
                resolve(services, address.as_deref(), names).and_then(|r| found.home.get(&r.id))
            }
        };
        match to {
            Some(to) => {
                let ends = above(to);
                m.arrows.push(MapArrow {
                    note: None,
                    context: None,
                    shape: None,
                    written: Default::default(),
                    seen: vec![],
                    id: format!("{from}->{to}"),
                    parent: (above(from).into_iter())
                        .find(|p| ends.contains(p))
                        .unwrap_or_default(),
                    from: from.clone(),
                    to: to.clone(),
                    kind: ArrowKind::Calls,
                    label: l.label.clone(),
                    count: 0,
                    kinds: vec![ArrowKind::Calls],
                    in_loop: false,
                    origin: Origin::Built,
                    parts: vec![],
                    ..Default::default()
                });
            }
            None if !pause => m.marks.push(word(
                from,
                MarkState::Warn,
                "stale",
                Some("no instance at this address".into()),
            )),
            None => {}
        }
    }
}

/// `reads` merged into `m`. A failed read gives one notice and concludes nothing. With no reads
/// `m` is left as it is.
pub fn apply(m: &mut MapFile, reads: &[ConnectorRead]) -> Applied {
    let mut out = Applied::default();
    if reads.is_empty() {
        return out;
    }
    let mut live: Vec<&Snapshot> = vec![];
    for r in reads {
        match &r.result {
            Ok(s) => live.push(s),
            Err(e) if e.starts_with(&r.label) => out.notices.push(e.clone()),
            Err(e) => out.notices.push(format!("{}: {e}", r.label)),
        }
    }
    let looked: Vec<(&str, &str)> = (reads.iter())
        .filter(|r| r.result.is_ok())
        .filter_map(|r| Some((r.kind.as_str(), r.remote.as_deref()?)))
        .collect();
    let all: Vec<&Resource> = live.iter().flat_map(|s| &s.resources).collect();
    let services: Vec<&Resource> = (all.iter().copied())
        .filter(|r| r.kind == ResourceKind::Service)
        .collect();
    let mut found = Found::default();
    match_runs(m, &services, &looked, &mut found);
    match_names(m, &services, &mut found, &mut out.warnings);
    let holder = traefik_box(m, &services, &found, &mut out.warnings);
    unmanaged(m, &services, &mut found);
    m.marks
        .extend(found.on.iter().filter_map(|(id, rs)| state_mark(id, rs)));
    place_routes(m, &all, &services, holder, &mut found);
    // a read that failed or was skipped may hold the target, and so may a server nobody read
    // ponytail: only Traefik reports no addresses; ask the connector when a second does not
    let reports = |r: &ConnectorRead| r.result.is_ok() && r.kind != TRAEFIK;
    let pause = live.len() < reads.len() || !reads.iter().any(reports);
    draw_links(m, &live, &services, &found, pause);
    sort_marks(&mut m.marks);
    out
}

#[cfg(test)]
mod tests;
