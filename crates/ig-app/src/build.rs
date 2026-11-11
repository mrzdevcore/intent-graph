//! Language-neutral graph assembly: the frontends' nodes and edges merged (code never resolves
//! into tests), blueprint `system` nodes and `http` links, and the extras `setup::facts` reports.

use crate::map::externals;
use ig_core::config::Config;
use ig_core::http;
use ig_core::model::{Edge, Graph, Node, ParseError, Schema, TOOL_VERSION};
use ig_core::ports::{FileFacts, Files, LanguageFrontend, PortError, SchemaReader};
use ig_core::workspace::{self, Layout, Package, SourceFile, WorkspaceInfo};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

pub struct Built {
    pub graph: Graph,
    pub workspace: WorkspaceInfo,
    /// Facts that are not graph data, for `setup::facts`.
    pub extras: Extras,
    /// The tables the tree declares (`ig_schema::read`).
    pub schema: Schema,
    /// What the schema's read could not apply (an `ALTER` of a table nobody created…).
    pub schema_notices: Vec<String>,
}

#[derive(Default)]
pub struct Extras {
    /// External package → number of modules importing it.
    pub packages: BTreeMap<String, usize>,
    /// (module, external package it imports), outside tests.
    pub imports: BTreeSet<(String, String)>,
    /// Callee name → calls whose first argument looks like a URL.
    pub url_callees: BTreeMap<String, u32>,
    /// HTTP client calls that matched no route: (file, line, method, url).
    pub http_unmatched: Vec<(String, u32, String, String)>,
}

/// The tree's packages, as the frontends declare or find them (the first to name a folder
/// names it; the root too when none is declared), and the files some frontend handles.
pub fn discover(
    frontends: &[&dyn LanguageFrontend],
    files: &dyn Files,
    config: &Config,
) -> Result<WorkspaceInfo, PortError> {
    discover_named(frontends, files, config).map(|(ws, _)| ws)
}

/// `discover`, and each frontend's own name for the folders it named.
fn discover_named(
    frontends: &[&dyn LanguageFrontend],
    files: &dyn Files,
    config: &Config,
) -> Result<(WorkspaceInfo, Vec<Vec<Package>>), PortError> {
    let source = |p: &str| frontends.iter().any(|f| f.handles(p, config));
    let declared: Vec<Vec<Package>> = (frontends.iter())
        .map(|f| f.packages(files, config))
        .collect();
    let keep_root = declared.iter().all(Vec::is_empty);
    let own: Vec<Vec<Package>> = (frontends.iter().zip(declared))
        .map(|(f, mut d)| {
            d.extend(f.found(files, config));
            d
        })
        .collect();
    let mut packages: Vec<Package> = own.iter().flatten().cloned().collect();
    packages.sort_by(|a, b| a.dir.cmp(&b.dir));
    packages.dedup_by(|a, b| a.dir == b.dir);
    let layout = Layout {
        packages,
        keep_root,
        fences: frontends.iter().flat_map(|f| f.fences()).copied().collect(),
        source: &source,
    };
    Ok((files.discover(config, &layout)?, own))
}

/// `ws` with the packages `own` names called by those names.
fn named_by(ws: &WorkspaceInfo, own: &[Package]) -> Option<WorkspaceInfo> {
    let same = (ws.packages.iter()).all(|p| own.iter().all(|o| o.dir != p.dir || o.name == p.name));
    if same {
        return None;
    }
    let mut ws = ws.clone();
    for p in &mut ws.packages {
        if let Some(o) = own.iter().find(|o| o.dir == p.dir) {
            p.name = o.name.clone();
        }
    }
    Some(ws)
}

/// Builds the graph of the tree `files` is rooted at. Each source file goes to the first
/// frontend that handles it; `tables` reads its tables. `graph.commit` is left for the caller.
pub fn build_graph(
    frontends: &[&dyn LanguageFrontend],
    tables: Option<&dyn SchemaReader>,
    files: &dyn Files,
    config: &Config,
) -> Result<Built, PortError> {
    let (ws, own) = discover_named(frontends, files, config)?;
    let tests = workspace::globset(&config.tests).map_err(|e| PortError::Io(e.to_string()))?;
    let mut facts: Vec<FileFacts> = vec![];
    let mut edge_lists: Vec<Vec<Edge>> = vec![];
    let mut parse_errors: Vec<ParseError> = vec![];
    let mut packages: Vec<(String, String, u32)> = vec![];
    for (i, fe) in frontends.iter().enumerate() {
        let mine: Vec<(SourceFile, String)> = ws
            .files
            .iter()
            .filter(|f| {
                fe.handles(&f.path, config)
                    && !frontends[..i].iter().any(|p| p.handles(&f.path, config))
            })
            // ponytail: serial reads; parallelise the reads (needs a batch read on the Files port) if large repos need it
            .map(|f| (f.clone(), files.read(&f.path).unwrap_or_default()))
            .collect();
        let renamed = named_by(&ws, &own[i]);
        let out = fe.build(&mine, renamed.as_ref().unwrap_or(&ws), config);
        facts.extend(out.facts);
        edge_lists.push(out.edges);
        parse_errors.extend(out.parse_errors);
        packages.extend(out.packages);
    }
    facts.sort_by(|a, b| a.file.cmp(&b.file));

    // A file's Prisma schema: its package's, else the root's.
    let schema = |file: &str| {
        let dir = ws
            .files
            .iter()
            .find(|f| f.path == file)
            .map_or(".", |f| ws.packages[f.package].dir.as_str());
        [
            format!("{dir}/prisma/schema.prisma"),
            "prisma/schema.prisma".into(),
        ]
        .iter()
        .find_map(|p| files.read(p.trim_start_matches("./")))
    };
    let (system_nodes, blueprint_edges, renames, http_unmatched) =
        blueprint_links(&facts, &packages, config, &tests, &schema);
    let mut extras = Extras {
        http_unmatched,
        ..Extras::default()
    };
    let mut seen = HashSet::new();
    for (file, p, _) in packages.iter().filter(|(f, _, _)| !tests.is_match(f)) {
        if seen.insert((file, p)) {
            *extras.packages.entry(p.clone()).or_default() += 1;
            extras.imports.insert((file.clone(), p.clone()));
        }
    }
    for f in facts.iter().filter(|f| !tests.is_match(&f.file)) {
        for (k, v) in &f.url_callees {
            *extras.url_callees.entry(k.clone()).or_default() += v;
        }
    }
    edge_lists.push(blueprint_edges);
    for f in &mut facts {
        for n in &mut f.nodes {
            if let Some(name) = renames.get(&n.id) {
                n.name = name.clone();
            }
        }
    }

    let mut nodes: Vec<Node> = system_nodes;
    let (schema, schema_notices) = tables
        .map(|r| read_schema(r, files, &ws, config, &tests))
        .unwrap_or_default();
    if let Some(db) = database(&schema, &nodes) {
        nodes.push(db);
    }
    for f in &mut facts {
        if let Some(m) = &f.parse_error {
            parse_errors.push(ParseError {
                file: f.file.clone(),
                message: m.clone(),
            });
        }
        nodes.append(&mut f.nodes);
    }
    nodes.sort_by(|a, b| a.id.cmp(&b.id));
    let known: HashMap<&str, &str> = nodes
        .iter()
        .map(|n| (n.id.as_str(), n.file.as_str()))
        .collect();
    let mut merged: BTreeMap<(String, String, String), Edge> = BTreeMap::new();
    for e in edge_lists.into_iter().flatten() {
        let (Some(from), Some(to)) = (known.get(e.from.as_str()), known.get(e.to.as_str())) else {
            continue;
        };
        // Non-test code never resolves into test files (stubs, mocks); the import itself stays.
        if e.kind != "imports" && tests.is_match(to) && !tests.is_match(from) {
            continue;
        }
        let key = (e.from.clone(), e.kind.clone(), e.to.clone());
        match merged.get_mut(&key) {
            Some(x) => {
                x.namespace |= e.namespace;
                x.reexport |= e.reexport;
                x.in_loop |= e.in_loop;
                if e.resolved() {
                    x.confidence = e.confidence.clone();
                }
                if (e.file.as_str(), e.line) < (x.file.as_str(), x.line) {
                    x.file = e.file;
                    x.line = e.line;
                }
            }
            None => {
                merged.insert(key, e);
            }
        }
    }
    let edges = merged.into_values().collect();
    parse_errors.sort_by(|a, b| (&a.file, &a.message).cmp(&(&b.file, &b.message)));
    parse_errors.dedup();
    let graph = Graph {
        schema_version: 1,
        tool_version: TOOL_VERSION.into(),
        commit: None,
        nodes,
        edges,
        parse_errors,
    };
    Ok(Built {
        graph,
        workspace: ws,
        extras,
        schema,
        schema_notices,
    })
}

/// The schema of the tree: its `.sql` and `.prisma` files (found as source files are: what git
/// lists, outside build output and `ignore`) and the source files that may declare models,
/// outside tests and folders whose name starts with `.` (a `.claude/worktrees` checkout is
/// another copy of the same migrations); the notices are on statements it could not apply.
fn read_schema(
    reader: &dyn SchemaReader,
    files: &dyn Files,
    ws: &WorkspaceInfo,
    config: &Config,
    tests: &globset::GlobSet,
) -> (Schema, Vec<String>) {
    let declared = |p: &str| p.ends_with(".sql") || p.ends_with(".prisma");
    let layout = Layout {
        packages: vec![],
        keep_root: true,
        fences: vec![],
        source: &declared,
    };
    let found = files.discover(config, &layout).map_or(vec![], |w| w.files);
    let sources = ws.files.iter().chain(&found).map(|f| f.path.clone());
    let mut paths: Vec<String> = sources
        .filter(|p| !p.split('/').any(|d| d.starts_with('.')))
        .filter(|p| reader.wants(p) && !tests.is_match(p))
        .collect();
    paths.sort();
    paths.dedup();
    // ponytail: every source file is read again here; the frontends read them once already
    let texts: Vec<(String, String)> = paths
        .into_iter()
        .filter_map(|p| Some((p.clone(), files.read(&p)?)))
        .collect();
    reader.read(&texts)
}

/// The database a schema with tables is kept in, when no known database is imported: a system
/// node named for its dialect.
fn database(schema: &Schema, nodes: &[Node]) -> Option<Node> {
    if schema.tables.is_empty() {
        return None;
    }
    let is_db = |n: &Node| {
        n.kind == "system"
            && externals::by_title(&n.name).is_some_and(|k| externals::relational(k.id))
    };
    if nodes.iter().any(is_db) {
        return None;
    }
    let name = externals::for_dialect(schema.dialect.as_deref()).title;
    let id = format!("system:{name}");
    Some(Node {
        hash: ig_core::model::sha1_hex(&id),
        id,
        kind: "system".into(),
        name: name.into(),
        file: String::new(),
        range: ig_core::model::Range {
            start_line: 1,
            end_line: 1,
        },
        parent: None,
        package: None,
        exported: false,
        metrics: ig_core::model::Metrics {
            loc: 0,
            complexity: 0,
        },
        intents: vec![],
    })
}

/// System nodes, blueprint edges, route renames (id → name) and unmatched http calls.
type Links = (
    Vec<Node>,
    Vec<Edge>,
    HashMap<String, String>,
    Vec<(String, u32, String, String)>,
);

/// `http` edges from client calls to route nodes, and `system` nodes with `uses` edges from the
/// modules importing their packages: the blueprint's, and the known ones (`map::externals`)
/// imported outside tests, joined to a blueprint system of the same name.
fn blueprint_links(
    facts: &[FileFacts],
    packages: &[(String, String, u32)],
    config: &Config,
    tests: &globset::GlobSet,
    schema: &dyn Fn(&str) -> Option<String>,
) -> Links {
    let bp = &config.blueprint;
    let mut edges = vec![];
    let mut unmatched = vec![];
    // Route node id → display name with `${CONST}` substituted (the id stays as matched).
    let mut renames = HashMap::new();
    let edge = |from: &str, to: &str, kind: &str, conf: &str, file: &str, line: u32| Edge {
        from: from.into(),
        to: to.into(),
        kind: kind.into(),
        confidence: conf.into(),
        file: file.into(),
        line,
        namespace: false,
        reexport: false,
        in_loop: false,
    };
    if !bp.http_calls.is_empty() {
        // Route node names are "METHOD /path" (or just "/path" for any method).
        let routes: Vec<(String, Vec<String>, &str, String)> = facts
            .iter()
            .flat_map(|f| f.nodes.iter().map(move |n| (f, n)))
            .filter(|(_, n)| n.kind == bp.route_kind)
            .map(|(f, n)| {
                let (m, p) = http::route(&n.name, &f.url_consts);
                (m, http::url_segments(&p), n.id.as_str(), p)
            })
            .collect();
        for (m, _, id, p) in &routes {
            if !p.contains('\u{0}') && n_name_differs(facts, id, m, p) {
                renames.insert(
                    id.to_string(),
                    if m == "*" {
                        p.clone()
                    } else {
                        format!("{m} {p}")
                    },
                );
            }
        }
        let routes: Vec<(String, Vec<String>, &str)> =
            routes.into_iter().map(|(m, s, id, _)| (m, s, id)).collect();
        for f in facts {
            for h in &f.http {
                let (hits, conf) = http::match_call(&routes, &h.method, &h.url, &bp.http_prefix);
                if hits.is_empty() {
                    unmatched.push((f.file.clone(), h.line, h.method.clone(), h.url.clone()));
                }
                for id in hits {
                    edges.push(edge(&h.from, id, "http", conf, &f.file, h.line));
                }
            }
        }
    }
    let mut systems = std::collections::BTreeMap::new();
    let slug = crate::map::facts::slug;
    for (file, pkg, line) in packages {
        let name = match bp
            .systems
            .iter()
            .find(|(_, pkgs)| pkgs.iter().any(|p| p == pkg))
        {
            Some((name, _)) => name.clone(),
            None if tests.is_match(file) => continue,
            None => {
                let Some(k) = externals::system(pkg, || schema(file)) else {
                    continue;
                };
                bp.systems
                    .iter()
                    .map(|(n, _)| n)
                    .find(|n| [k.id.to_string(), slug(k.title, "")].contains(&slug(n, "")))
                    .cloned()
                    .unwrap_or_else(|| k.title.to_string())
            }
        };
        let id = format!("system:{name}");
        edges.push(edge(file, &id, "uses", "resolved", file, *line));
        systems.entry(id).or_insert(name);
    }
    let nodes = systems
        .into_iter()
        .map(|(id, name)| Node {
            hash: ig_core::model::sha1_hex(&id),
            id,
            kind: "system".into(),
            name,
            file: String::new(),
            range: ig_core::model::Range {
                start_line: 1,
                end_line: 1,
            },
            parent: None,
            package: None,
            exported: false,
            metrics: ig_core::model::Metrics {
                loc: 0,
                complexity: 0,
            },
            intents: vec![],
        })
        .collect();
    (nodes, edges, renames, unmatched)
}

fn n_name_differs(facts: &[FileFacts], id: &str, m: &str, p: &str) -> bool {
    let want = if m == "*" {
        p.to_string()
    } else {
        format!("{m} {p}")
    };
    facts
        .iter()
        .flat_map(|f| f.nodes.iter())
        .find(|n| n.id == id)
        .is_some_and(|n| n.name != want)
}

#[rustfmt::skip]
const SYSTEM_CATEGORIES: &[(&str, &[&str])] = &[
    ("database", &["postgres", "pg", "mysql", "mariadb", "mongo", "mongoose", "prisma", "typeorm", "knex", "sequelize", "sqlite", "redis", "ioredis", "dynamo", "firestore", "supabase", "cassandra", "sqlalchemy"]),
    // Never plain "elastic": "Elastic APM" is monitoring.
    ("search", &["elastic search", "elasticsearch", "@elastic/elasticsearch", "algolia", "meili", "typesense", "opensearch"]),
    ("monitoring", &["apm", "datadog", "sentry", "newrelic", "opentelemetry", "grafana", "prometheus", "logrocket"]),
    ("auth", &["auth0", "okta", "keycloak", "cognito", "clerk", "firebase-auth", "next-auth", "passport"]),
    ("cms", &["contentful", "strapi", "sanity", "prismic", "storyblok", "directus", "ghost"]),
    ("analytics", &["segment", "mixpanel", "amplitude", "google-analytics", "gtag", "matomo", "posthog"]),
    ("queue", &["kafka", "rabbit", "amqp", "sqs", "bull", "nats", "pika", "celery"]),
    ("storage", &["s3", "@aws-sdk/client-s3", "minio", "cloudinary", "blob"]),
    ("payment", &["stripe", "paypal", "adyen"]),
    ("email", &["sendgrid", "mailgun", "nodemailer", "postmark", "ses"]),
];

/// The icon category of a blueprint system: the first whose keywords occur (case-insensitive) in
/// its name or one of its packages, else "service".
pub fn system_category(name: &str, packages: &[String]) -> &'static str {
    let hay: Vec<String> = std::iter::once(name)
        .chain(packages.iter().map(String::as_str))
        .map(str::to_lowercase)
        .collect();
    SYSTEM_CATEGORIES
        .iter()
        .find(|(_, kws)| kws.iter().any(|k| hay.iter().any(|h| h.contains(k))))
        .map_or("service", |(c, _)| c)
}

#[cfg(test)]
mod tests;
