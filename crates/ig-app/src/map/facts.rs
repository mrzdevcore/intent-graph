//! The boxes ig knows from the code alone: the system, its containers (workspace packages,
//! blueprint systems and the known outside systems the code imports) and their components
//! (source folders, read through the folder pattern).

use ig_core::config::Config;
use ig_core::model::Graph;
use ig_core::workspace::WorkspaceInfo;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    System,
    Container,
    Component,
    Code,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BoxKind {
    Person,
    System,
    External,
    App,
    #[serde(alias = "api", alias = "worker")]
    Service,
    Job,
    Store,
    Queue,
    Model,
    #[serde(alias = "library")]
    Module,
    Entry,
    Usecase,
    Port,
    Adapter,
    Agent,
    Domain,
    Group,
    Folder,
    File,
    /// A UI component (Vue, React), opening onto its parts.
    Component,
    /// One part list of a component: its inputs, state, uses or template.
    Parts,
    /// An Ansible task.
    Task,
    /// A database's table or view, inside the store it is kept in.
    Table,
    Function,
    Class,
    Method,
}

/// Where a kind sits: the system, a block that runs, a part of one's design, a plain group, or code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Root,
    Run,
    Design,
    Group,
    Code,
}

impl BoxKind {
    /// Kinds that name code, not architecture: never written to (or read from) `ig/`.
    pub fn code_level(self) -> bool {
        self.role() == Role::Code
    }

    pub fn role(self) -> Role {
        use BoxKind::*;
        match self {
            System => Role::Root,
            Group => Role::Group,
            Service | Job | App | Store | Queue | Model | External | Person => Role::Run,
            Entry | Usecase | Domain | Port | Adapter | Agent | Module => Role::Design,
            Folder | File | Component | Parts | Task | Table | Function | Class | Method => {
                Role::Code
            }
        }
    }
}

/// ig's own reading of a box.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Guess {
    pub kind: BoxKind,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub pattern: Option<Pattern>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub slot: Option<String>,
    /// A simple-icons slug.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub logo: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FactBox {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub parent: Option<String>,
    pub level: Level,
    /// Globs of the files the box is made of.
    pub made_of: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub package: Option<String>,
    pub files: u32,
    /// Graph node ids in the box (its modules; a blueprint system's node).
    pub nodes: Vec<String>,
    pub guess: Guess,
}

/// Components shown per container; the smallest past this are grouped into one box.
const MAX_COMPONENTS: usize = 12;

const APPLICATION: &[&str] = &["application", "app", "usecases", "use-cases"];
const INFRA: &[&str] = &["infra", "infrastructure"];
const ENTRY: &[&str] = &["routers", "api", "controllers", "pages"];
/// Layered bands, top to bottom: (band name, folder names).
const BANDS: &[(&str, &[&str])] = &[
    ("ui", &["ui", "presentation", "pages", "components"]),
    ("services", &["services", "business", "features"]),
    ("data", &["data", "repositories", "db", "models"]),
];

/// The system, its containers and their components, sorted by id.
pub fn facts(ws: &WorkspaceInfo, g: &Graph, cfg: &Config) -> Vec<FactBox> {
    let modules: Vec<&str> = g
        .nodes
        .iter()
        .filter(|n| n.is_module())
        .map(|n| n.id.as_str())
        .collect();
    let title = ws
        .packages
        .iter()
        .find(|p| p.dir == ".")
        .map(|p| p.name.clone())
        .or_else(|| ws.root.file_name().map(|n| n.to_string_lossy().into()))
        .unwrap_or_else(|| "system".into());
    let mut out = vec![FactBox {
        id: "system".into(),
        parent: None,
        level: Level::System,
        made_of: vec!["**".into()],
        package: None,
        files: ws.files.len() as u32,
        nodes: vec![],
        guess: guess(BoxKind::System, title, None, None),
    }];
    let with_src: Vec<usize> = (0..ws.packages.len())
        .filter(|i| ws.files.iter().any(|f| f.package == *i))
        .collect();
    // Known outside systems the code imports (graph `system` nodes the blueprint doesn't declare).
    let detected: Vec<(&str, &super::externals::Known)> = g
        .nodes
        .iter()
        .filter(|n| n.kind == "system" && !cfg.blueprint.systems.iter().any(|(s, _)| *s == n.name))
        .filter_map(|n| Some((n.id.as_str(), super::externals::by_title(&n.name)?)))
        .collect();
    // Container ids (packages, blueprint systems, then detected ones) made unique before
    // components use them.
    let pkgs: Vec<usize> = with_src
        .iter()
        .copied()
        .filter(|&i| ws.packages[i].dir != "." || with_src.len() == 1)
        .collect();
    let mut ids: Vec<(String, usize)> = pkgs
        .iter()
        .map(|&i| {
            let name = &ws.packages[i].name;
            slug(name.rsplit('/').next().unwrap_or(name), "pkg")
        })
        .chain(cfg.blueprint.systems.iter().map(|(n, _)| slug(n, "system")))
        .chain(detected.iter().map(|(_, k)| k.id.to_string()))
        .enumerate()
        .map(|(k, id)| (id, k))
        .collect();
    ids.sort();
    // `system` is the root's id
    let unique = dedupe(ids.iter().map(|(id, _)| id.clone()).collect(), &["system"]);
    let mut id_of = vec![String::new(); ids.len()];
    for ((_, k), id) in ids.iter().zip(unique) {
        id_of[*k] = id;
    }
    for (k, &i) in pkgs.iter().enumerate() {
        let p = &ws.packages[i];
        let files: Vec<&str> = ws
            .files
            .iter()
            .filter(|f| f.package == i)
            .map(|f| f.path.as_str())
            .collect();
        container(&mut out, &id_of[k], &p.name, &p.dir, &files, &modules);
    }
    for (k, (name, pkgs_of)) in cfg.blueprint.systems.iter().enumerate() {
        let node = format!("system:{name}");
        let store = crate::build::system_category(name, pkgs_of) == "database";
        out.push(FactBox {
            id: id_of[pkgs.len() + k].clone(),
            parent: Some("system".into()),
            level: Level::Container,
            made_of: vec![],
            package: None,
            files: 0,
            nodes: g
                .nodes
                .iter()
                .filter(|n| n.id == node)
                .map(|_| node.clone())
                .collect(),
            guess: guess(
                if store {
                    BoxKind::Store
                } else {
                    BoxKind::External
                },
                name.clone(),
                None,
                None,
            ),
        });
    }
    let first = pkgs.len() + cfg.blueprint.systems.len();
    for (k, (node, known)) in detected.iter().enumerate() {
        let mut g = guess(known.kind, known.title.into(), None, None);
        g.logo = known.logo.map(String::from);
        out.push(FactBox {
            id: id_of[first + k].clone(),
            parent: Some("system".into()),
            level: Level::Container,
            made_of: vec![],
            package: None,
            files: 0,
            nodes: vec![node.to_string()],
            guess: g,
        });
    }
    group::group_containers(&mut out);
    out.sort_by(|a, b| a.id.cmp(&b.id));
    out
}

fn guess(kind: BoxKind, title: String, pattern: Option<Pattern>, slot: Option<&str>) -> Guess {
    Guess {
        kind,
        title,
        pattern,
        slot: slot.map(String::from),
        logo: None,
    }
}

/// "use-cases" → "Use cases".
fn title(s: &str) -> String {
    let s = s.replace(['-', '_'], " ");
    let mut c = s.chars();
    c.next()
        .map(|f| f.to_uppercase().chain(c).collect())
        .unwrap_or_default()
}

/// A package's container and its components.
fn container(
    out: &mut Vec<FactBox>,
    id: &str,
    name: &str,
    dir: &str,
    files: &[&str],
    modules: &[&str],
) {
    let id = id.to_string();
    let base = if dir == "." {
        String::new()
    } else {
        format!("{dir}/")
    };
    let src = format!("{base}src/");
    let root = if files.iter().any(|f| f.starts_with(&src)) {
        src
    } else {
        base.clone()
    };
    // Folders below the source root, up to 3 deep, with their file counts (direct files).
    let rel: Vec<&str> = files.iter().filter_map(|f| f.strip_prefix(&root)).collect();
    let mut folders = std::collections::BTreeSet::new();
    for f in &rel {
        let parts: Vec<&str> = f.split('/').collect();
        for d in 1..parts.len().min(4) {
            folders.insert(parts[..d].join("/"));
        }
    }
    let pattern = detect(&folders);
    // Files under `folder` (only its direct files when `shallow`), relative to the source root.
    let under = |folder: &str, f: &str, shallow: bool| {
        f.strip_prefix(&format!("{folder}/"))
            .is_some_and(|r| !shallow || !r.contains('/'))
    };
    let app = rel.iter().any(|f| {
        f.split('/').any(|d| d == "pages" || d == "app")
            || ["main.", "server.", "app."]
                .iter()
                .any(|e| f.starts_with(e))
    });
    let pkg_nodes: Vec<String> = modules
        .iter()
        .filter(|m| files.contains(m))
        .map(|m| m.to_string())
        .collect();
    out.push(FactBox {
        id: id.clone(),
        parent: Some("system".into()),
        level: Level::Container,
        made_of: vec![if base.is_empty() {
            "**".into()
        } else {
            format!("{base}**")
        }],
        package: Some(name.into()),
        files: files.len() as u32,
        nodes: pkg_nodes,
        guess: guess(
            if app { BoxKind::App } else { BoxKind::Module },
            title(&id),
            Some(pattern.clone()),
            None,
        ),
    });

    // (folder, slot) of each component.
    let slotted: Vec<&String> = folders
        .iter()
        .filter(|f| slot(&pattern, last(f)).is_some())
        .filter(|f| {
            !folders
                .iter()
                .any(|o| f.starts_with(&format!("{o}/")) && slot(&pattern, last(o)).is_some())
        })
        .collect();
    let kids = |p: &str| -> Vec<String> {
        folders
            .iter()
            .filter(|f| under(p, f, true))
            .cloned()
            .collect()
    };
    let has_direct = |p: &str| rel.iter().any(|f| under(p, f, true));
    // (folder, slot, only its direct files)
    let mut comps: Vec<(String, Option<&str>, bool)> = vec![];
    for p in &slotted {
        let s = slot(&pattern, last(p));
        let k = kids(p);
        if k.is_empty() {
            comps.push((p.to_string(), s, false));
        } else if has_direct(p) {
            comps.push((p.to_string(), s, true));
        }
        comps.extend(k.into_iter().map(|k| (k, s, false)));
    }
    let other = other(&pattern);
    // Other folders; those holding pattern folders deeper down are split into their children.
    let mut stack: Vec<String> = folders
        .iter()
        .filter(|f| !f.contains('/'))
        .cloned()
        .collect();
    while let Some(f) = stack.pop() {
        if slotted.contains(&&f) {
            continue;
        }
        if slotted.iter().any(|s| under(&f, s, false)) {
            if has_direct(&f) {
                comps.push((f.clone(), other, true));
            }
            stack.extend(kids(&f));
        } else {
            comps.push((f, other, false));
        }
    }
    let mut boxes: Vec<FactBox> = comps
        .into_iter()
        .map(|(folder, s, shallow)| {
            let pre = &root;
            FactBox {
                id: format!("{id}/{}", slug(&folder.replace('/', "-"), "dir")),
                parent: Some(id.clone()),
                level: Level::Component,
                made_of: vec![format!(
                    "{root}{folder}/{}",
                    if shallow { "*" } else { "**" }
                )],
                package: Some(name.into()),
                files: rel.iter().filter(|f| under(&folder, f, shallow)).count() as u32,
                nodes: modules
                    .iter()
                    .filter(|m| {
                        m.strip_prefix(pre.as_str())
                            .is_some_and(|r| under(&folder, r, shallow))
                    })
                    .map(|m| m.to_string())
                    .collect(),
                guess: guess(kind_of(&folder), title(last(&folder)), None, s),
            }
        })
        .collect();
    // the files right under the source root (all of them when it has no folders)
    let loose: Vec<&&str> = rel.iter().filter(|f| !f.contains('/')).collect();
    if !loose.is_empty() {
        let entry = loose.iter().any(|f| {
            ["main.", "server.", "app.", "index."]
                .iter()
                .any(|e| f.starts_with(e))
        });
        boxes.push(FactBox {
            id: format!("{id}/root"),
            parent: Some(id.clone()),
            level: Level::Component,
            made_of: vec![format!("{root}*")],
            package: Some(name.into()),
            files: loose.len() as u32,
            nodes: modules
                .iter()
                .filter(|m| m.strip_prefix(&root).is_some_and(|r| !r.contains('/')))
                .map(|m| m.to_string())
                .collect(),
            guess: guess(
                if entry {
                    BoxKind::Entry
                } else {
                    BoxKind::Usecase
                },
                title(if root == base { "root" } else { "src" }),
                None,
                other,
            ),
        });
    }
    cap(&mut boxes, &id, Some(name), other, "folders");
    out.extend(boxes);
}

/// `boxes` (the children of `id`) with the smallest past 12 merged into `{id}/other`, and their
/// id parts made unique (folder slugs can collide: `domain/order` and `domain-order`, a folder
/// named `other`).
pub(super) fn cap(
    boxes: &mut Vec<FactBox>,
    id: &str,
    package: Option<&str>,
    slot: Option<&str>,
    noun: &str,
) {
    if boxes.len() > MAX_COMPONENTS {
        boxes.sort_by(|a, b| b.files.cmp(&a.files).then(a.id.cmp(&b.id)));
        let rest = boxes.split_off(MAX_COMPONENTS - 1);
        let mut nodes: Vec<String> = rest.iter().flat_map(|b| b.nodes.clone()).collect();
        nodes.sort();
        boxes.push(FactBox {
            id: format!("{id}/other"),
            parent: Some(id.into()),
            level: Level::Component,
            made_of: rest.iter().flat_map(|b| b.made_of.clone()).collect(),
            package: package.map(String::from),
            files: rest.iter().map(|b| b.files).sum(),
            nodes,
            guess: guess(
                BoxKind::Group,
                format!("Other ({} {noun})", rest.len()),
                None,
                slot,
            ),
        });
    }
    boxes.sort_by(|a, b| a.id.cmp(&b.id));
    let pre = format!("{id}/");
    let parts = boxes
        .iter()
        .map(|b| b.id[pre.len()..].to_string())
        .collect();
    for (b, part) in boxes.iter_mut().zip(dedupe(parts, &[])) {
        b.id = format!("{pre}{part}");
    }
}

fn last(folder: &str) -> &str {
    folder.rsplit('/').next().unwrap_or(folder)
}

/// The slot of the folders a pattern does not name.
pub(super) fn other(p: &Pattern) -> Option<&str> {
    match p {
        Pattern::Onion => Some("infrastructure"),
        Pattern::Hexagonal => Some("adapters"),
        // ponytail: unknown folders of a layered package sit in the bottom band (shared code).
        Pattern::Layered { bands } => bands.last().map(String::as_str),
        Pattern::None => None,
    }
}

/// The pattern the folder names show (ruling R7).
pub(super) fn detect(folders: &std::collections::BTreeSet<String>) -> Pattern {
    let names: std::collections::HashSet<String> =
        folders.iter().map(|f| last(f).to_lowercase()).collect();
    let any = |set: &[&str]| set.iter().any(|n| names.contains(*n));
    if any(&["domain"]) && any(APPLICATION) && any(INFRA) {
        return Pattern::Onion;
    }
    if any(&["domain", "core"]) && any(&["ports"]) && any(&["adapters"]) {
        return Pattern::Hexagonal;
    }
    let bands: Vec<String> = BANDS
        .iter()
        .filter(|(_, n)| any(n))
        .map(|(b, _)| b.to_string())
        .collect();
    if bands.len() >= 2 {
        return Pattern::Layered { bands };
    }
    Pattern::None
}

/// The ring or band a pattern folder named `name` stands for.
pub(super) fn slot(p: &Pattern, name: &str) -> Option<&'static str> {
    let n = name.to_lowercase();
    let n = n.as_str();
    match p {
        Pattern::Onion if n == "domain" => Some("domain"),
        Pattern::Onion if APPLICATION.contains(&n) => Some("application"),
        Pattern::Onion if INFRA.contains(&n) => Some("infrastructure"),
        Pattern::Hexagonal if n == "domain" || n == "core" => Some("core"),
        Pattern::Hexagonal if n == "ports" => Some("ports"),
        Pattern::Hexagonal if n == "adapters" => Some("adapters"),
        Pattern::Layered { bands } => BANDS
            .iter()
            .find(|(b, names)| names.contains(&n) && bands.iter().any(|x| x == b))
            .map(|(b, _)| *b),
        _ => None,
    }
}

/// The kind a component folder suggests (ruling R12); a use case by default.
fn kind_of(folder: &str) -> BoxKind {
    let segs: Vec<String> = folder.split('/').map(str::to_lowercase).collect();
    let has = |set: &[&str]| segs.iter().any(|s| set.contains(&s.as_str()));
    if ENTRY.contains(&segs[segs.len() - 1].as_str()) {
        BoxKind::Entry
    } else if has(&["ports"]) {
        BoxKind::Port
    } else if has(APPLICATION) {
        BoxKind::Usecase
    } else if has(&["domain", "core"]) {
        BoxKind::Domain
    } else if has(INFRA) || has(&["adapters"]) {
        BoxKind::Adapter
    } else {
        BoxKind::Usecase
    }
}

mod group;
mod pattern;
mod slug;
pub use pattern::Pattern;
pub(super) use slug::dedupe;
pub(crate) use slug::slug;

#[cfg(test)]
mod tests;
