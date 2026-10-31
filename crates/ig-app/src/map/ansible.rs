//! The map of an Ansible tree, for a repository ig reads no code in. Each role is a component:
//! a service when it has `meta/targets.yml` (the inventory groups it is deployed to), else a
//! helper. Roles sit in containers by where they are deployed (their targets), a large one split
//! by name. A role's tasks and files are its code level. Arrows: a task including a role or
//! rendering a template, a template's settings naming another service (`PRODUCT_API_HOST`), and
//! the outside systems its settings name (`AUTH0_…`, `ELASTIC_APM_…`, `DATABASE_…`).

mod read;

use super::facts::{BoxKind, FactBox, Guess, Level, dedupe, slug};
use ig_core::model::{Edge, Graph, Metrics, Node, Range};
use ig_core::ports::Files;
use std::collections::{BTreeMap, BTreeSet, HashMap};

/// The most roles in a container before it is split by name.
const MAX: usize = 12;
/// A deployment target with fewer roles joins the brand-specific container.
const FEW: usize = 3;

/// Outside systems by the prefix of the settings that reach them: (prefix, title, kind, logo).
#[rustfmt::skip]
const SYSTEMS: &[(&str, &str, BoxKind, Option<&str>)] = &[
    ("AUTH0", "Auth0", BoxKind::External, Some("auth0")),
    ("ELASTIC_APM", "Elastic APM", BoxKind::External, Some("elastic")),
    ("ELASTICSEARCH", "Elasticsearch", BoxKind::Store, Some("elasticsearch")),
    ("ELASTIC_SEARCH", "Elasticsearch", BoxKind::Store, Some("elasticsearch")),
    ("SENTRY", "Sentry", BoxKind::External, Some("sentry")),
    ("STRIPE", "Stripe", BoxKind::External, Some("stripe")),
    ("ALGOLIA", "Algolia", BoxKind::External, Some("algolia")),
    ("CONTENTFUL", "Contentful", BoxKind::External, Some("contentful")),
    ("DATADOG", "Datadog", BoxKind::External, Some("datadog")),
    ("POSTHOG", "PostHog", BoxKind::External, Some("posthog")),
    ("IZANAMI", "Izanami", BoxKind::External, None),
    ("DALENYS", "Dalenys", BoxKind::External, None),
    ("MANGOPAY", "Mangopay", BoxKind::External, None),
    ("ZENDESK", "Zendesk", BoxKind::External, None),
    ("MAILJET", "Mailjet", BoxKind::External, None),
    ("SENDGRID", "SendGrid", BoxKind::External, None),
    ("OPENAI", "OpenAI", BoxKind::External, None),
    ("MIRAKL", "Mirakl", BoxKind::External, None),
    ("LANGFUSE", "Langfuse", BoxKind::External, None),
    ("METABASE", "Metabase", BoxKind::External, None),
    ("DASHBOARD_METABASE", "Metabase", BoxKind::External, None),
    ("AZURE_BLOB", "Azure Blob Storage", BoxKind::Store, None),
    ("ANTHROPIC", "Anthropic", BoxKind::External, None),
    ("POSTGRES", "PostgreSQL", BoxKind::Store, Some("postgresql")),
    ("PG", "PostgreSQL", BoxKind::Store, Some("postgresql")),
    ("DATABASE", "Database", BoxKind::Store, None),
    ("DB", "Database", BoxKind::Store, None),
    ("MONGO", "MongoDB", BoxKind::Store, Some("mongodb")),
    ("MONGODB", "MongoDB", BoxKind::Store, Some("mongodb")),
    ("REDIS", "Redis", BoxKind::Store, Some("redis")),
    ("KAFKA", "Kafka", BoxKind::Queue, Some("kafka")),
    ("RABBITMQ", "RabbitMQ", BoxKind::Queue, Some("rabbitmq")),
    ("AMQP", "RabbitMQ", BoxKind::Queue, Some("rabbitmq")),
];

/// The boxes and the graph of an Ansible tree.
pub struct Ansible {
    pub boxes: Vec<FactBox>,
    pub graph: Graph,
}

struct Role {
    name: String,
    dir: String,
    files: Vec<String>,
    targets: Vec<String>,
}

/// The map of the roles under `files`, None when there are none.
pub fn read(files: &dyn Files) -> Option<Ansible> {
    let all = files.glob(&["**/roles/*/**".into()]).ok()?;
    let mut by_dir: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for f in all {
        let Some(i) = f
            .find("roles/")
            .filter(|&i| i == 0 || f[..i].ends_with('/'))
        else {
            continue;
        };
        let rest = &f[i + 6..];
        let Some((name, _)) = rest.split_once('/') else {
            continue;
        };
        by_dir
            .entry(format!("{}roles/{name}", &f[..i]))
            .or_default()
            .push(f);
    }
    let roles: Vec<Role> = by_dir
        .into_iter()
        .filter(|(_, fs)| {
            fs.iter()
                .any(|f| f.contains("/tasks/") || f.contains("/meta/"))
        })
        .map(|(dir, fs)| Role {
            name: dir.rsplit('/').next().unwrap_or_default().to_string(),
            targets: (fs.iter())
                .find(|f| f.ends_with("/meta/targets.yml") || f.ends_with("/meta/targets.yaml"))
                .and_then(|f| files.read(f))
                .map(|t| read::list(&t))
                .unwrap_or_default(),
            dir,
            files: fs,
        })
        .collect();
    if roles.is_empty() {
        return None;
    }
    let graph = graph(files, &roles);
    Some(Ansible {
        boxes: boxes(&roles, &graph),
        graph,
    })
}

fn node(
    id: String,
    kind: &str,
    name: &str,
    file: &str,
    lines: (u32, u32),
    parent: Option<String>,
) -> Node {
    Node {
        id,
        kind: kind.into(),
        name: name.into(),
        file: file.into(),
        range: Range {
            start_line: lines.0,
            end_line: lines.1,
        },
        parent,
        package: None,
        exported: false,
        hash: String::new(),
        metrics: Metrics {
            loc: lines.1.saturating_sub(lines.0) + 1,
            complexity: 0,
        },
        intents: vec![],
    }
}

fn edge(from: &str, to: &str, kind: &str, file: &str, line: u32) -> Edge {
    Edge {
        from: from.into(),
        to: to.into(),
        kind: kind.into(),
        confidence: "inferred".into(),
        file: file.into(),
        line,
        namespace: false,
        reexport: false,
        in_loop: false,
    }
}

/// The outside system a setting reaches, by its prefix.
fn system_of(
    key: &str,
) -> Option<&'static (&'static str, &'static str, BoxKind, Option<&'static str>)> {
    SYSTEMS
        .iter()
        .find(|(p, ..)| key == *p || key.strip_prefix(p).is_some_and(|r| r.starts_with('_')))
}

/// The role a service name stands for: `notifications_api` is `notification_api`, `user_api`
/// is `users_api`, `catalog` is `catalog_api`.
fn role_named<'r>(roles: &'r [Role], base: &str, me: &str) -> Option<&'r Role> {
    let singular: String = base
        .split('_')
        .map(|t| t.strip_suffix('s').filter(|s| s.len() > 2).unwrap_or(t))
        .collect::<Vec<_>>()
        .join("_");
    let plural = match singular.split_once('_') {
        Some((a, b)) => format!("{a}s_{b}"),
        None => format!("{singular}s"),
    };
    let names = [
        base.to_string(),
        singular.clone(),
        plural,
        format!("{base}_api"),
        format!("{singular}_api"),
    ];
    names
        .iter()
        .find_map(|n| roles.iter().find(|r| r.name == *n && r.name != me))
}

/// Every role file a module, its tasks (and handlers) and other files nodes in it, and the
/// edges between them, the roles and the outside systems.
fn graph(files: &dyn Files, roles: &[Role]) -> Graph {
    let mut g = Graph::empty();
    let entry = |r: &Role| {
        let main = format!("{}/tasks/main.yml", r.dir);
        if r.files.contains(&main) {
            main
        } else {
            r.files[0].clone()
        }
    };
    let mut systems = BTreeSet::new();
    for r in roles {
        for f in &r.files {
            let Some(text) = files.read(f) else { continue };
            let n = text.lines().count().max(1) as u32;
            g.nodes.push(node(
                f.clone(),
                "module",
                f.rsplit('/').next().unwrap_or(f),
                f,
                (1, n),
                None,
            ));
            let rel = &f[r.dir.len() + 1..];
            if (rel.starts_with("tasks/") || rel.starts_with("handlers/"))
                && (f.ends_with(".yml") || f.ends_with(".yaml"))
            {
                let mut seen: HashMap<String, u32> = HashMap::new();
                for t in read::tasks(&text) {
                    let base = format!("{f}#{}", slug(&t.name, "task"));
                    let k = seen.entry(base.clone()).or_default();
                    *k += 1;
                    let id = if *k == 1 { base } else { format!("{base}-{k}") };
                    for x in &t.roles {
                        if let Some(to) = roles.iter().find(|o| o.name == *x) {
                            g.edges.push(edge(&id, &entry(to), "calls", f, t.lines.0));
                        }
                    }
                    for x in &t.files {
                        let dir = if x.ends_with(".j2") {
                            "templates"
                        } else {
                            "tasks"
                        };
                        let to = format!("{}/{dir}/{x}", r.dir);
                        if r.files.contains(&to) {
                            g.edges
                                .push(edge(&id, &format!("{to}#file"), "imports", f, t.lines.0));
                        }
                    }
                    g.nodes
                        .push(node(id, "task", &t.name, f, t.lines, Some(f.clone())));
                }
                continue;
            }
            let id = format!("{f}#file");
            if f.ends_with(".j2") {
                for (base, line) in read::services(&text) {
                    if let Some(to) = role_named(roles, &base, &r.name) {
                        g.edges.push(edge(&id, &entry(to), "http", f, line));
                    }
                }
                for (key, line) in read::settings(&text) {
                    if let Some(s) = system_of(&key) {
                        systems.insert(s.1);
                        g.edges
                            .push(edge(&id, &format!("system:{}", s.1), "uses", f, line));
                    }
                }
            }
            let name = f.rsplit('/').next().unwrap_or(f);
            g.nodes
                .push(node(id, "document", name, f, (1, n), Some(f.clone())));
        }
    }
    for s in systems {
        g.nodes
            .push(node(format!("system:{s}"), "system", s, "", (0, 0), None));
    }
    g.edges
        .sort_by(|a, b| (&a.from, &a.to, a.line).cmp(&(&b.from, &b.to, b.line)));
    g.edges
        .dedup_by(|a, b| a.from == b.from && a.to == b.to && a.kind == b.kind);
    g
}

/// "appro_platform" → "Appro platform"; "b2b" → "B2B".
fn human(s: &str) -> String {
    if s.len() <= 3 {
        return s.to_ascii_uppercase();
    }
    let s = s.replace(['_', '-'], " ");
    let mut c = s.chars();
    c.next()
        .map(|f| f.to_uppercase().chain(c).collect())
        .unwrap_or_default()
}

/// A role's kind by its name: fronts, helpers, else a service.
fn kind_of(r: &Role) -> BoxKind {
    if r.name.starts_with("helper_") || r.targets.is_empty() {
        BoxKind::Module
    } else if has(
        r,
        &[
            "front",
            "ui",
            "backoffice",
            "portal",
            "documentation",
            "status",
            "board",
        ],
    ) {
        BoxKind::App
    } else {
        BoxKind::Service
    }
}

/// Whether a word of `r`'s name (split on `_`) is one of `ws`.
fn has(r: &Role, ws: &[&str]) -> bool {
    ws.iter().any(|w| r.name.split('_').any(|t| t == *w))
}

/// One component: a role, or the roles of one kind when a container has too many.
struct Part<'r> {
    /// Its id's last part: the role's name, or the kind's and its chunk's (never a count, so it
    /// keeps its annotations when roles come and go).
    key: String,
    title: String,
    kind: BoxKind,
    roles: Vec<&'r Role>,
}

/// "APIs", "Fronts"… for the roles of a kind put together: a service named for jobs is a job's.
fn plural(r: &Role) -> &'static str {
    let jobs = [
        "job", "consumer", "importer", "indexing", "sitemap", "task", "cron",
    ];
    match kind_of(r) {
        BoxKind::App => "Fronts",
        BoxKind::Module => "Helpers",
        _ if has(r, &jobs) => "Jobs",
        _ => "APIs",
    }
}

/// The containers, at most `room`: roles by where they are deployed (their targets), the
/// smallest put together past `room`. Each holds its roles, or past `MAX` its roles by kind.
fn containers(roles: &[Role], room: usize) -> Vec<(String, Vec<Part<'_>>)> {
    let mut by: BTreeMap<String, Vec<&Role>> = BTreeMap::new();
    for r in roles {
        let key = if kind_of(r) == BoxKind::Module {
            "Helpers".to_string()
        } else {
            r.targets
                .iter()
                .map(|t| human(t))
                .collect::<Vec<_>>()
                .join(" + ")
        };
        by.entry(key).or_default().push(r);
    }
    let mut groups: Vec<(String, Vec<&Role>)> = by.into_iter().collect();
    let small = |g: &[(String, Vec<&Role>)]| {
        let mut idx: Vec<usize> = (0..g.len()).filter(|&i| g[i].0 != "Helpers").collect();
        idx.sort_by_key(|&i| (g[i].1.len(), i));
        idx
    };
    // targets of a few roles each, then the smallest, join the others
    let few: Vec<usize> = small(&groups)
        .into_iter()
        .filter(|&i| groups[i].1.len() < FEW)
        .collect();
    let mut merge: BTreeSet<usize> = if few.len() > 1 {
        few.into_iter().collect()
    } else {
        BTreeSet::new()
    };
    for i in small(&groups) {
        if groups.len() - merge.len() + usize::from(!merge.is_empty()) <= room.max(2) {
            break;
        }
        merge.insert(i);
    }
    if !merge.is_empty() {
        let mut other = vec![];
        for i in merge.iter().rev() {
            other.extend(groups.remove(*i).1);
        }
        other.sort_by(|a, b| a.name.cmp(&b.name));
        groups.push(("Other targets".into(), other));
    }
    groups
        .into_iter()
        .map(|(title, rs)| {
            let parts = if rs.len() <= MAX {
                rs.into_iter()
                    .map(|r| Part {
                        key: r.name.clone(),
                        title: r.name.clone(),
                        kind: kind_of(r),
                        roles: vec![r],
                    })
                    .collect()
            } else {
                let mut by_kind: BTreeMap<&str, (BoxKind, Vec<&Role>)> = BTreeMap::new();
                for r in rs {
                    let k = kind_of(r);
                    by_kind.entry(plural(r)).or_insert((k, vec![])).1.push(r);
                }
                // a kind's roles by `MAX`, each role then a folder in it
                let mut out = vec![];
                for (t, (kind, roles)) in by_kind {
                    let n = roles.len().div_ceil(MAX);
                    for (k, c) in roles.chunks(roles.len().div_ceil(n)).enumerate() {
                        let title = if n > 1 {
                            format!("{t} {}/{n}", k + 1)
                        } else {
                            t.to_string()
                        };
                        out.push(Part {
                            key: if n > 1 {
                                format!("{t}-{}", k + 1)
                            } else {
                                t.to_string()
                            },
                            title: format!("{title} ({})", c.len()),
                            kind,
                            roles: c.to_vec(),
                        });
                    }
                }
                out
            };
            (title, parts)
        })
        .collect()
}

fn guess(kind: BoxKind, title: String, logo: Option<&str>) -> Guess {
    Guess {
        kind,
        title,
        pattern: None,
        slot: None,
        logo: logo.map(String::from),
    }
}

fn boxes(roles: &[Role], g: &Graph) -> Vec<FactBox> {
    let root = roles[0]
        .dir
        .rsplit_once("/roles/")
        .map_or("system", |p| p.0);
    let title = root
        .rsplit('/')
        .next()
        .filter(|t| !t.is_empty())
        .unwrap_or("infrastructure");
    let mut out = vec![FactBox {
        id: "system".into(),
        parent: None,
        level: Level::System,
        made_of: vec!["**".into()],
        package: None,
        files: roles.iter().map(|r| r.files.len() as u32).sum(),
        nodes: vec![],
        guess: guess(BoxKind::System, title.into(), None),
    }];
    let systems: Vec<&Node> = g.nodes.iter().filter(|n| n.kind == "system").collect();
    // the outside systems in one box of their own when they would not fit beside the roles
    let apart = systems.len() > MAX / 3;
    // past `MAX`, the services apart from the stores and queues
    // a roll-up holds run blocks, so it is a group
    let buckets: Vec<(&str, &str, Vec<&Node>)> = if !apart {
        vec![]
    } else if systems.len() <= MAX {
        vec![("outside", "Outside systems", systems.clone())]
    } else {
        let data = |n: &&Node| {
            SYSTEMS
                .iter()
                .any(|s| s.1 == n.name && s.2 != BoxKind::External)
        };
        let (d, e): (Vec<&Node>, Vec<&Node>) = systems.iter().copied().partition(data);
        vec![
            ("outside", "Outside services", e),
            ("data", "Data stores", d),
        ]
    };
    // (id, title, kind, systems), by `MAX`
    let mut outside: Vec<(String, String, Vec<&Node>)> = vec![];
    for (key, title, ns) in buckets {
        let n = ns.len().div_ceil(MAX);
        for (k, c) in ns.chunks(ns.len().div_ceil(n).max(1)).enumerate() {
            let (id, t) = if n > 1 {
                (format!("{key}-{}", k + 1), format!("{title} {}/{n}", k + 1))
            } else {
                (key.to_string(), title.to_string())
            };
            outside.push((id, format!("{t} ({})", c.len()), c.to_vec()));
        }
    }
    let room = MAX - if apart { outside.len() } else { systems.len() };
    let groups = containers(roles, room);
    let tops: Vec<String> = (groups.iter().map(|(t, _)| slug(t, "roles")))
        .chain(if apart {
            outside.iter().map(|o| o.0.clone()).collect::<Vec<_>>()
        } else {
            systems.iter().map(|n| slug(&n.name, "system")).collect()
        })
        .collect();
    let ids = dedupe(tops, &["system"]);
    let dirs =
        |rs: &[&Role]| -> Vec<String> { rs.iter().map(|r| format!("{}/**", r.dir)).collect() };
    let all = |rs: &[&Role]| -> Vec<String> { rs.iter().flat_map(|r| r.files.clone()).collect() };
    let count = |rs: &[&Role]| rs.iter().map(|r| r.files.len() as u32).sum();
    for ((title, parts), id) in groups.iter().zip(&ids) {
        let rs: Vec<&Role> = parts.iter().flat_map(|p| p.roles.clone()).collect();
        out.push(FactBox {
            id: id.clone(),
            parent: Some("system".into()),
            level: Level::Container,
            made_of: dirs(&rs),
            package: None,
            files: count(&rs),
            nodes: all(&rs),
            guess: guess(BoxKind::Group, title.clone(), None),
        });
        let kids = dedupe(parts.iter().map(|p| slug(&p.key, "role")).collect(), &[]);
        for (p, k) in parts.iter().zip(kids) {
            out.push(FactBox {
                id: format!("{id}/{k}"),
                parent: Some(id.clone()),
                level: Level::Component,
                made_of: dirs(&p.roles),
                package: None,
                files: count(&p.roles),
                nodes: all(&p.roles),
                guess: guess(p.kind, p.title.clone(), None),
            });
        }
    }
    let rest = &ids[groups.len()..];
    let system = |n: &Node, id: String, parent: &str, level: Level| {
        let s = SYSTEMS.iter().find(|s| s.1 == n.name).unwrap();
        FactBox {
            id,
            parent: Some(parent.into()),
            level,
            made_of: vec![],
            package: None,
            files: 0,
            nodes: vec![n.id.clone()],
            guess: guess(s.2, n.name.clone(), s.3),
        }
    };
    for ((_, title, ns), id) in outside.iter().zip(rest) {
        out.push(FactBox {
            id: id.clone(),
            parent: Some("system".into()),
            level: Level::Container,
            made_of: vec![],
            package: None,
            files: 0,
            nodes: ns.iter().map(|n| n.id.clone()).collect(),
            guess: guess(BoxKind::Group, title.clone(), None),
        });
        let kids = dedupe(ns.iter().map(|n| slug(&n.name, "system")).collect(), &[]);
        for (n, k) in ns.iter().zip(kids) {
            out.push(system(n, format!("{id}/{k}"), id, Level::Component));
        }
    }
    if !apart {
        for (n, id) in systems.iter().zip(rest) {
            out.push(system(n, id.clone(), "system", Level::Container));
        }
    }
    out.sort_by(|a, b| a.id.cmp(&b.id));
    out
}

#[cfg(test)]
mod tests;
