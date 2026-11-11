use super::*;
use ig_core::testing::MemFiles;

#[test]
fn rs_frontend_meets_the_contract() {
    ig_core::testing::frontend_contract(
        &RsFrontend,
        "src/lib.rs",
        "pub fn f() -> u32 {\n    g()\n}\n\nfn g() -> u32 {\n    1\n}\n",
    );
}

const SHOP: &[(&str, &str)] = &[
    (
        "crates/core/src/lib.rs",
        "pub mod model;\nmod price;\npub use price::total;\n\n/// @intent SHOP-1\npub trait Store {\n    fn save(&self, o: &model::Order);\n}\n",
    ),
    (
        "crates/core/src/model.rs",
        "pub struct Order {\n    pub items: Vec<u32>,\n}\n\nimpl Order {\n    pub fn new() -> Self {\n        Self::empty()\n    }\n    fn empty() -> Self {\n        Order { items: vec![] }\n    }\n    pub fn total(&self) -> u32 {\n        crate::price::total(&self.items)\n    }\n}\n",
    ),
    (
        "crates/core/src/price.rs",
        "use super::model::Order;\n\npub fn total(items: &[u32]) -> u32 {\n    let mut t = 0;\n    for i in items {\n        t += add(*i);\n    }\n    t\n}\n\nfn add(i: u32) -> u32 {\n    if i > 0 && i < 10 { i } else { 0 }\n}\n\n#[cfg(test)]\nmod tests {\n    use super::*;\n\n    #[test]\n    fn adds() {\n        assert_eq!(total(&[1]), 1);\n    }\n}\n",
    ),
    (
        "crates/app/src/main.rs",
        "use shop_core::model::Order;\nuse shop_core::{total, Store};\nuse serde::Serialize;\nmod db;\n\nfn main() {\n    let o = Order::new();\n    total(&o.items);\n}\n",
    ),
    (
        "crates/app/src/db.rs",
        "use shop_core::Store;\nuse shop_core::model::Order;\n\npub struct Pg;\n\nimpl Store for Pg {\n    fn save(&self, o: &Order) {\n        self.log(o.total());\n    }\n}\n\nimpl Pg {\n    fn log(&self, _n: u32) {}\n}\n",
    ),
];

fn build(files: &[(&str, &str)]) -> FrontendOutput {
    let packages = vec![
        Package {
            name: "shop-app".into(),
            dir: "crates/app".into(),
        },
        Package {
            name: "shop-core".into(),
            dir: "crates/core".into(),
        },
    ];
    let input: Vec<(SourceFile, String)> = files
        .iter()
        .map(|(p, s)| {
            (
                SourceFile {
                    path: (*p).into(),
                    package: usize::from(p.starts_with("crates/core")),
                },
                s.to_string(),
            )
        })
        .collect();
    let ws = WorkspaceInfo {
        root: ".".into(),
        packages,
        files: input.iter().map(|f| f.0.clone()).collect(),
    };
    RsFrontend.build(&input, &ws, &Config::default())
}

fn edges(out: &FrontendOutput, kind: &str) -> Vec<(String, String, String)> {
    let mut e: Vec<_> = (out.edges.iter().filter(|e| e.kind == kind))
        .map(|e| (e.from.clone(), e.to.clone(), e.confidence.clone()))
        .collect();
    e.sort();
    e.dedup();
    e
}

#[test]
fn nodes_types_methods_and_tests() {
    let out = build(SHOP);
    let nodes = |f: &str| -> Vec<(String, String, String)> {
        let facts = out.facts.iter().find(|x| x.file == f).unwrap();
        let p = |n: &ig_core::model::Node| n.parent.clone().unwrap_or_default();
        facts
            .nodes
            .iter()
            .map(|n| (n.id.clone(), n.kind.clone(), p(n)))
            .collect()
    };
    let m = "crates/core/src/model.rs";
    let s = |a: &str, b: &str, c: &str| (a.to_string(), b.to_string(), c.to_string());
    assert_eq!(
        nodes(m),
        [
            s(m, "module", ""),
            s(&format!("{m}#Order"), "class", m),
            s(&format!("{m}#Order.new"), "method", &format!("{m}#Order")),
            s(&format!("{m}#Order.empty"), "method", &format!("{m}#Order")),
            s(&format!("{m}#Order.total"), "method", &format!("{m}#Order")),
        ]
    );
    let price = out
        .facts
        .iter()
        .find(|x| x.file == "crates/core/src/price.rs")
        .unwrap();
    let adds = price.nodes.iter().find(|n| n.name == "adds").unwrap();
    assert_eq!(
        (adds.id.as_str(), adds.kind.as_str()),
        ("crates/core/src/price.rs#tests.adds", "test")
    );
    let add = price.nodes.iter().find(|n| n.name == "add").unwrap();
    assert_eq!(
        (add.metrics.complexity, add.exported),
        (3, false),
        "if and &&; not pub"
    );
    let lib = out
        .facts
        .iter()
        .find(|x| x.file == "crates/core/src/lib.rs")
        .unwrap();
    assert_eq!(lib.nodes[1].intents, ["SHOP-1"]);
}

#[test]
fn modules_uses_and_packages() {
    let out = build(SHOP);
    let r = |a: &str, b: &str| {
        (
            format!("crates/{a}"),
            format!("crates/{b}"),
            "resolved".to_string(),
        )
    };
    assert_eq!(
        edges(&out, "imports"),
        [
            r("app/src/db.rs", "core/src/lib.rs"),
            r("app/src/db.rs", "core/src/model.rs"),
            r("app/src/main.rs", "app/src/db.rs"),
            r("app/src/main.rs", "core/src/lib.rs"),
            r("app/src/main.rs", "core/src/model.rs"),
            r("app/src/main.rs", "core/src/price.rs"),
            r("core/src/lib.rs", "core/src/model.rs"),
            r("core/src/lib.rs", "core/src/price.rs"),
            r("core/src/price.rs", "core/src/model.rs"),
        ]
    );
    assert_eq!(
        out.packages,
        [("crates/app/src/main.rs".to_string(), "serde".to_string(), 3)]
    );
}

#[test]
fn calls_methods_and_impls() {
    let out = build(SHOP);
    let calls = edges(&out, "calls");
    let has = |from: &str, to: &str| {
        let (from, to) = (format!("crates/{from}"), format!("crates/{to}"));
        calls
            .iter()
            .any(|(f, t, c)| *f == from && *t == to && c == "resolved")
    };
    assert!(
        has("app/src/main.rs#main", "core/src/model.rs#Order.new"),
        "Type::f through a use: {calls:#?}"
    );
    assert!(
        has("app/src/main.rs#main", "core/src/price.rs#total"),
        "a pub use re-export"
    );
    assert!(
        has(
            "core/src/model.rs#Order.new",
            "core/src/model.rs#Order.empty"
        ),
        "Self::"
    );
    assert!(
        has("core/src/model.rs#Order.total", "core/src/price.rs#total"),
        "crate::"
    );
    assert!(
        has("app/src/db.rs#Pg.save", "app/src/db.rs#Pg.log"),
        "self."
    );
    let add = out.edges.iter().find(|e| e.to.ends_with("#add")).unwrap();
    assert!(add.in_loop);
    assert_eq!(
        edges(&out, "implements"),
        [(
            "crates/app/src/db.rs#Pg".into(),
            "crates/core/src/lib.rs#Store".into(),
            "resolved".into()
        )]
    );
}

#[test]
fn workspace_members_minus_excluded() {
    let f = MemFiles::with(&[
        (
            "Cargo.toml",
            "[workspace]\nmembers = [\"crates/*\"]\nexclude = [\"crates/old\"]\n",
        ),
        (
            "crates/core/Cargo.toml",
            "[package]\nname = \"shop-core\"\n",
        ),
        ("crates/app/Cargo.toml", "[package]\nname = \"shop-app\"\n"),
        ("crates/old/Cargo.toml", "[package]\nname = \"old\"\n"),
    ]);
    let got: Vec<(String, String)> = RsFrontend
        .packages(&f, &Config::default())
        .into_iter()
        .map(|p| (p.dir, p.name))
        .collect();
    assert_eq!(
        got,
        [
            ("crates/app".into(), "shop-app".into()),
            ("crates/core".into(), "shop-core".into())
        ]
    );
    let one = MemFiles::with(&[("Cargo.toml", "[package]\nname = \"tool\"\n")]);
    assert_eq!(
        RsFrontend.packages(&one, &Config::default()),
        [Package {
            name: "tool".into(),
            dir: ".".into()
        }]
    );
}

#[test]
fn guesses_stay_in_crates_the_caller_uses() {
    // core never uses app: `x.log()` in core is not app's `Pg.log`
    let mut files = SHOP.to_vec();
    files.push((
        "crates/core/src/audit.rs",
        "pub fn audit(x: u32) {\n    x.log();\n}\n",
    ));
    let out = build(&files);
    assert!(
        !out.edges
            .iter()
            .any(|e| e.from.starts_with("crates/core/") && e.to.starts_with("crates/app/"))
    );
    // app uses core: its guesses may land there
    assert!(
        edges(&out, "calls")
            .iter()
            .any(|(f, t, _)| f.starts_with("crates/app/") && t.starts_with("crates/core/"))
    );
}
