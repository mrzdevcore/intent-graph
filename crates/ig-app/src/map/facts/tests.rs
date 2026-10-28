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

fn module(path: &str) -> ig_core::model::Node {
    serde_json::from_value(serde_json::json!({
        "id": path, "kind": "module", "name": path, "file": path,
        "range": {"startLine": 1, "endLine": 1}, "hash": "",
        "metrics": {"loc": 1, "complexity": 1}, "intents": []
    }))
    .unwrap()
}

/// A workspace of `(package name, dir, files)` and a graph of its modules.
fn repo(pkgs: &[(&str, &str, &[&str])]) -> (WorkspaceInfo, Graph) {
    let mut g = Graph::empty();
    let mut files = vec![];
    for (i, (_, _, paths)) in pkgs.iter().enumerate() {
        for p in *paths {
            files.push(SourceFile {
                path: p.to_string(),
                package: i,
            });
            g.nodes.push(module(p));
        }
    }
    let packages = pkgs
        .iter()
        .map(|(name, dir, _)| Package {
            name: name.to_string(),
            dir: dir.to_string(),
            tsconfig: None,
            source_entry: None,
        })
        .collect();
    let ws = WorkspaceInfo {
        root: "repo".into(),
        packages,
        files,
    };
    (ws, g)
}

fn valid(id: &str) -> bool {
    id.len() <= 64
        && id.starts_with(|c: char| c.is_ascii_lowercase() || c.is_ascii_digit())
        && id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

#[test]
fn ids_valid_and_unique() {
    let long = format!("@x/{}", "long".repeat(20));
    let mut many: Vec<String> = (0..12).map(|i| format!("a/src/f{i:02}/x.ts")).collect();
    // A real folder `other`, big enough to stay beside the `other` group.
    many.extend(["a/src/other/x.ts".into(), "a/src/other/y.ts".into()]);
    let many: Vec<&str> = many.iter().map(String::as_str).collect();
    let (ws, g) = repo(&[
        ("@a/utils", "a", &many),
        (
            "@b/utils",
            "b",
            &[
                "b/src/domain/order/x.ts",
                "b/src/domain-order/y.ts",
                "b/src/application/c/z.ts",
                "b/src/infra/p/w.ts",
            ],
        ),
        ("stripe", "s", &["s/src/x.ts"]),
        ("@x/café", "c", &["c/src/x.ts"]),
        ("@scope/___", "e", &["e/src/x.ts"]),
        (&long, "l", &["l/src/x.ts"]),
    ]);
    let cfg = Config::parse("blueprint: { systems: { Stripe: [stripe] } }\n").unwrap();
    let b = facts(&ws, &g, &cfg);
    let mut ids: Vec<&str> = b.iter().map(|b| b.id.as_str()).collect();
    let n = ids.len();
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), n, "duplicate ids in {ids:?}");
    for x in b.iter().filter(|x| x.level != Level::System) {
        let (head, tail) = match x.id.split_once('/') {
            Some((c, s)) => (c, Some(s)),
            None => (x.id.as_str(), None),
        };
        assert!(valid(head) && tail.is_none_or(valid), "invalid id {}", x.id);
        let parent = x.parent.as_deref().unwrap();
        assert!(b.iter().any(|p| p.id == parent), "{} has no parent", x.id);
    }
    let containers = of_level(&b, Level::Container);
    for want in ["utils", "utils-2", "stripe", "stripe-2", "caf", "pkg"] {
        assert!(containers.contains(&want), "{want} not in {containers:?}");
    }
    for want in ["utils/other", "utils/other-2", "utils-2/domain-order-2"] {
        assert!(ids.contains(&want), "{want} not in {ids:?}");
    }
}

#[test]
fn every_file_in_exactly_one_box() {
    let paths = [
        "src/domain/order/o.ts",
        "src/application/a.ts",
        "src/application/checkout/c.ts",
        "src/infra/db/d.ts",
        "src/modules/orders/domain/x.ts",
        "src/modules/orders/application/y.ts",
        "src/modules/orders/infra/z.ts",
        "src/modules/billing/b.ts",
    ];
    let (ws, g) = repo(&[("repo", ".", &paths)]);
    let b = facts(&ws, &g, &Config::default());
    let comps: Vec<&FactBox> = b.iter().filter(|b| b.level == Level::Component).collect();
    for p in paths {
        let owners: Vec<&str> = comps
            .iter()
            .filter(|c| c.nodes.iter().any(|n| n == p))
            .map(|c| c.id.as_str())
            .collect();
        assert_eq!(owners.len(), 1, "{p} is in {owners:?}");
    }
    assert_eq!(
        comps.iter().map(|c| c.files).sum::<u32>(),
        paths.len() as u32
    );
}

#[test]
fn a_package_named_system_is_not_the_root() {
    let (ws, g) = repo(&[
        ("@acme/system", "a", &["a/src/x/a.ts"]),
        ("@acme/web", "b", &["b/src/x/b.ts"]),
    ]);
    let b = facts(&ws, &g, &Config::default());
    assert_eq!(of_level(&b, Level::System), ["system"]);
    assert_eq!(of_level(&b, Level::Container), ["system-2", "web"]);
    assert_eq!(get(&b, "system-2/x").parent.as_deref(), Some("system-2"));
}

#[test]
fn root_files_have_a_component() {
    // only loose files: one component holds them all
    let (ws, g) = repo(&[("flat", ".", &["src/a.ts", "src/b.ts"])]);
    let b = facts(&ws, &g, &Config::default());
    assert_eq!(of_level(&b, Level::Component), ["flat/root"]);
    let root = get(&b, "flat/root");
    assert_eq!(root.nodes, ["src/a.ts", "src/b.ts"]);
    assert_eq!(root.made_of, ["src/*"]);
    assert_eq!(root.guess.title, "Src");
    // beside folders: the loose files get their own component
    let (ws, g) = repo(&[("mixed", ".", &["src/main.ts", "src/x/y.ts"])]);
    let b = facts(&ws, &g, &Config::default());
    assert_eq!(of_level(&b, Level::Component), ["mixed/root", "mixed/x"]);
    assert_eq!(get(&b, "mixed/root").nodes, ["src/main.ts"]);
    assert_eq!(get(&b, "mixed/root").guess.kind, BoxKind::Entry);
    // no src folder: "Root"
    let (ws, g) = repo(&[("bare", ".", &["a.ts"])]);
    let b = facts(&ws, &g, &Config::default());
    assert_eq!(get(&b, "bare/root").guess.title, "Root");
    assert_eq!(get(&b, "bare/root").made_of, ["*"]);
}

/// 13 packages under `packages/` (and 2 apps), plus 2 outside systems.
fn many() -> (WorkspaceInfo, Graph, Config) {
    let paths: Vec<(String, String, Vec<String>)> = (1..=13)
        .map(|i| (format!("p{i}"), format!("packages/p{i}")))
        .chain((1..=2).map(|i| (format!("a{i}"), format!("apps/a{i}"))))
        .map(|(n, d)| (n.clone(), d.clone(), vec![format!("{d}/src/x/{n}.ts")]))
        .collect();
    let pkgs: Vec<(&str, &str, Vec<&str>)> = paths
        .iter()
        .map(|(n, d, f)| {
            (
                n.as_str(),
                d.as_str(),
                f.iter().map(String::as_str).collect(),
            )
        })
        .collect();
    let pkgs: Vec<(&str, &str, &[&str])> = pkgs
        .iter()
        .map(|(n, d, f)| (*n, *d, f.as_slice()))
        .collect();
    let (ws, g) = repo(&pkgs);
    let cfg = Config::parse("blueprint: { systems: { Stripe: [stripe], Pg: [pg] } }\n").unwrap();
    (ws, g, cfg)
}

#[test]
fn containers_past_twelve_are_grouped() {
    let (ws, g, cfg) = many();
    let b = facts(&ws, &g, &cfg);
    let roots: Vec<&str> = b
        .iter()
        .filter(|x| x.parent.as_deref() == Some("system"))
        .map(|x| x.id.as_str())
        .collect();
    assert!(roots.len() <= 12, "{roots:?}");
    assert_eq!(roots, ["apps", "packages", "pg", "stripe"]);
    let p = get(&b, "packages");
    assert_eq!(p.guess.kind, BoxKind::Group);
    assert_eq!(p.files, 13);
    let kids: Vec<&FactBox> = b
        .iter()
        .filter(|x| x.parent.as_deref() == Some("packages"))
        .collect();
    assert_eq!(kids.len(), 12);
    assert!(kids.iter().all(|k| k.level == Level::Component));
    assert_eq!(kids.iter().map(|k| k.files).sum::<u32>(), 13);
    assert_eq!(get(&b, "packages/p1").made_of, ["packages/p1/**"]);
    // every box's parent exists, and the levels nest
    for x in b.iter().filter(|x| x.level != Level::System) {
        let p = get(&b, x.parent.as_deref().unwrap());
        assert!(p.level < x.level, "{} under {}", x.id, p.id);
    }
}

#[test]
fn outside_systems_grouped_when_they_crowd_the_root() {
    let pkgs: Vec<(String, String)> = (1..=11)
        .map(|i| (format!("p{i}"), format!("p{i}/src/x/a.ts")))
        .collect();
    let pkgs: Vec<(&str, &str, Vec<&str>)> = pkgs
        .iter()
        .map(|(n, f)| (n.as_str(), n.as_str(), vec![f.as_str()]))
        .collect();
    let pkgs: Vec<(&str, &str, &[&str])> = pkgs
        .iter()
        .map(|(n, d, f)| (*n, *d, f.as_slice()))
        .collect();
    let (ws, g) = repo(&pkgs);
    let cfg = Config::parse("blueprint: { systems: { A: [a], B: [b], C: [c] } }\n").unwrap();
    let b = facts(&ws, &g, &cfg);
    let roots = b
        .iter()
        .filter(|x| x.parent.as_deref() == Some("system"))
        .count();
    assert_eq!(roots, 12);
    assert_eq!(get(&b, "outside").guess.kind, BoxKind::External);
    assert_eq!(get(&b, "outside/a").level, Level::Component);
}

#[test]
fn root_file_arrows_are_drawn() {
    let (ws, mut g) = repo(&[("mixed", ".", &["src/main.ts", "src/x/y.ts"])]);
    g.edges.push(
        serde_json::from_value(serde_json::json!({
            "from": "src/main.ts", "to": "src/x/y.ts", "kind": "imports",
            "confidence": "resolved", "file": "src/main.ts", "line": 1
        }))
        .unwrap(),
    );
    let b = facts(&ws, &g, &Config::default());
    let ids: Vec<String> = crate::map::arrows(&b, &g)
        .into_iter()
        .map(|a| a.id)
        .collect();
    assert_eq!(ids, ["mixed/root->mixed/x"]);
}
