//! The boxes ig knows from the code alone: the system, its containers (workspace packages and
//! blueprint systems) and their components (source folders, read through the folder pattern).

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
    Api,
    Worker,
    Store,
    Queue,
    Library,
    Entry,
    Usecase,
    Port,
    Domain,
    Function,
    Class,
    Group,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Pattern {
    Onion,
    Layered { bands: Vec<String> },
    Hexagonal,
    None,
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
    for &i in &with_src {
        let p = &ws.packages[i];
        if p.dir == "." && with_src.len() > 1 {
            continue;
        }
        let files: Vec<&str> = ws
            .files
            .iter()
            .filter(|f| f.package == i)
            .map(|f| f.path.as_str())
            .collect();
        container(&mut out, &p.name, &p.dir, &files, &modules);
    }
    for (name, pkgs) in &cfg.blueprint.systems {
        let node = format!("system:{name}");
        let store = crate::build::system_category(name, pkgs) == "database";
        out.push(FactBox {
            id: slug(name),
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
    out.sort_by(|a, b| a.id.cmp(&b.id));
    out
}

fn guess(kind: BoxKind, title: String, pattern: Option<Pattern>, slot: Option<&str>) -> Guess {
    Guess {
        kind,
        title,
        pattern,
        slot: slot.map(String::from),
    }
}

/// Lowercase, runs of anything but letters and digits as one `-`.
fn slug(s: &str) -> String {
    let s: String = s
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect();
    s.split('-')
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join("-")
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
fn container(out: &mut Vec<FactBox>, name: &str, dir: &str, files: &[&str], modules: &[&str]) {
    let id = slug(name.rsplit('/').next().unwrap_or(name));
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
    let inside = |folder: &str| -> Vec<&str> {
        let pre = format!("{root}{folder}/");
        modules
            .iter()
            .copied()
            .filter(|m| m.starts_with(&pre))
            .collect()
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
            if app { BoxKind::App } else { BoxKind::Library },
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
    let mut comps: Vec<(String, Option<&str>)> = vec![];
    for p in &slotted {
        let s = slot(&pattern, last(p));
        let pre = format!("{p}/");
        let kids: Vec<&String> = folders
            .iter()
            .filter(|f| f.strip_prefix(&pre).is_some_and(|r| !r.contains('/')))
            .collect();
        comps.extend(kids.iter().map(|k| (k.to_string(), s)));
        if kids.is_empty()
            || rel
                .iter()
                .any(|f| f.strip_prefix(&pre).is_some_and(|r| !r.contains('/')))
        {
            comps.push((p.to_string(), s));
        }
    }
    let other = match pattern {
        Pattern::Onion => Some("infrastructure"),
        Pattern::Hexagonal => Some("adapters"),
        // ponytail: unknown folders of a layered package sit in the bottom band (shared code).
        Pattern::Layered { ref bands } => bands.last().map(String::as_str),
        Pattern::None => None,
    };
    for top in folders.iter().filter(|f| !f.contains('/')) {
        let pre = format!("{top}/");
        if !slotted.iter().any(|s| *s == top || s.starts_with(&pre)) {
            comps.push((top.clone(), other));
        }
    }
    let mut boxes: Vec<FactBox> = comps
        .into_iter()
        .map(|(folder, s)| {
            let nodes = inside(&folder);
            let pre = format!("{folder}/");
            FactBox {
                id: format!("{id}/{}", slug(&folder)),
                parent: Some(id.clone()),
                level: Level::Component,
                made_of: vec![format!("{root}{folder}/**")],
                package: Some(name.into()),
                files: rel.iter().filter(|f| f.starts_with(&pre)).count() as u32,
                nodes: nodes.into_iter().map(String::from).collect(),
                guess: guess(kind_of(&folder), title(last(&folder)), None, s),
            }
        })
        .collect();
    if boxes.len() > MAX_COMPONENTS {
        boxes.sort_by(|a, b| b.files.cmp(&a.files).then(a.id.cmp(&b.id)));
        let rest = boxes.split_off(MAX_COMPONENTS - 1);
        let mut nodes: Vec<String> = rest.iter().flat_map(|b| b.nodes.clone()).collect();
        nodes.sort();
        boxes.push(FactBox {
            id: format!("{id}/other"),
            parent: Some(id.clone()),
            level: Level::Component,
            made_of: rest.iter().flat_map(|b| b.made_of.clone()).collect(),
            package: Some(name.into()),
            files: rest.iter().map(|b| b.files).sum(),
            nodes,
            guess: guess(
                BoxKind::Group,
                format!("Other ({} folders)", rest.len()),
                None,
                other,
            ),
        });
    }
    out.extend(boxes);
}

fn last(folder: &str) -> &str {
    folder.rsplit('/').next().unwrap_or(folder)
}

/// The pattern the folder names show (ruling R7).
fn detect(folders: &std::collections::BTreeSet<String>) -> Pattern {
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
fn slot(p: &Pattern, name: &str) -> Option<&'static str> {
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
        BoxKind::Entry
    } else {
        BoxKind::Usecase
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ig_core::workspace::{Package, SourceFile};

    fn get<'a>(boxes: &'a [FactBox], id: &str) -> &'a FactBox {
        boxes
            .iter()
            .find(|b| b.id == id)
            .unwrap_or_else(|| panic!("no box {id}"))
    }

    fn of_level(boxes: &[FactBox], level: Level) -> Vec<&str> {
        boxes
            .iter()
            .filter(|b| b.level == level)
            .map(|b| b.id.as_str())
            .collect()
    }

    fn single(paths: &[String]) -> WorkspaceInfo {
        WorkspaceInfo {
            root: "repo".into(),
            packages: vec![Package {
                name: "repo".into(),
                dir: ".".into(),
                tsconfig: None,
                source_entry: None,
            }],
            files: paths
                .iter()
                .map(|p| SourceFile {
                    path: p.clone(),
                    package: 0,
                })
                .collect(),
        }
    }

    #[test]
    fn shop_containers() {
        let (ws, g, cfg) = crate::map::shop::load();
        let b = facts(&ws, &g, &cfg);
        assert_eq!(
            of_level(&b, Level::Container),
            ["api", "api-client", "postgres", "stripe", "web"]
        );
        assert_eq!(get(&b, "postgres").guess.kind, BoxKind::Store);
        assert_eq!(get(&b, "stripe").guess.kind, BoxKind::External);
        assert_eq!(get(&b, "web").parent.as_deref(), Some("system"));
    }

    #[test]
    fn shop_onion_guess() {
        let (ws, g, cfg) = crate::map::shop::load();
        let b = facts(&ws, &g, &cfg);
        assert_eq!(get(&b, "api").guess.pattern, Some(Pattern::Onion));
        let checkout = get(&b, "api/application-checkout");
        assert_eq!(checkout.guess.slot.as_deref(), Some("application"));
        assert_eq!(checkout.guess.kind, BoxKind::Usecase);
        assert_eq!(checkout.parent.as_deref(), Some("api"));
        let order = get(&b, "api/domain-order");
        assert_eq!(order.guess.slot.as_deref(), Some("domain"));
        assert_eq!(order.guess.kind, BoxKind::Domain);
        assert_eq!(get(&b, "api/application-ports").guess.kind, BoxKind::Port);
        let routers = get(&b, "api/routers");
        assert_eq!(routers.guess.slot.as_deref(), Some("infrastructure"));
        assert_eq!(routers.guess.kind, BoxKind::Entry);
        assert_eq!(get(&b, "api/infra-prisma").files, 2);
    }

    #[test]
    fn single_package_repo() {
        let ws = single(&["src/a/x.ts".into(), "src/b/y.ts".into()]);
        let b = facts(&ws, &Graph::empty(), &Config::default());
        assert_eq!(of_level(&b, Level::Container), ["repo"]);
        assert_eq!(of_level(&b, Level::Component), ["repo/a", "repo/b"]);
        assert_eq!(get(&b, "repo").guess.pattern, Some(Pattern::None));
    }

    #[test]
    fn groups_past_twelve() {
        let paths: Vec<String> = (0..30).map(|i| format!("src/f{i:02}/x.ts")).collect();
        let b = facts(&single(&paths), &Graph::empty(), &Config::default());
        let comps: Vec<&FactBox> = b.iter().filter(|b| b.level == Level::Component).collect();
        assert!(comps.len() <= 12, "{} components", comps.len());
        let groups: Vec<&&FactBox> = comps
            .iter()
            .filter(|b| b.guess.kind == BoxKind::Group)
            .collect();
        assert_eq!(groups.len(), 1);
        assert_eq!(comps.iter().map(|b| b.files).sum::<u32>(), 30);
    }
}
