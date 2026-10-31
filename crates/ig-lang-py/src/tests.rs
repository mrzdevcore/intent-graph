use super::*;
use ig_core::testing::MemFiles;

#[test]
fn py_frontend_meets_the_contract() {
    ig_core::testing::frontend_contract(
        &PyFrontend,
        "app/a.py",
        "def f():\n    return g()\n\ndef g():\n    return 1\n",
    );
}

/// The tree `files` (path, source) as one root package, built.
fn build(files: &[(&str, &str)]) -> FrontendOutput {
    let ws = WorkspaceInfo {
        root: ".".into(),
        packages: vec![Package {
            name: "shop".into(),
            dir: ".".into(),
        }],
        files: files
            .iter()
            .map(|(p, _)| SourceFile {
                path: (*p).into(),
                package: 0,
            })
            .collect(),
    };
    let input: Vec<(SourceFile, String)> = ws
        .files
        .iter()
        .cloned()
        .zip(files.iter().map(|f| f.1.to_string()))
        .collect();
    PyFrontend.build(&input, &ws, &Config::default())
}

fn edges(out: &FrontendOutput, kind: &str) -> Vec<(String, String, String)> {
    let mut e: Vec<_> = (out.edges.iter())
        .filter(|e| e.kind == kind)
        .map(|e| (e.from.clone(), e.to.clone(), e.confidence.clone()))
        .collect();
    e.sort();
    e
}

const SHOP: &[(&str, &str)] = &[
    ("shop/__init__.py", "from .orders import place\n"),
    (
        "shop/orders.py",
        "import psycopg2\nimport os.path\nfrom . import pricing\nfrom .models import Order\n\n\
         class Repo(Base):\n    def save(self, o):\n        return self.conn().run(o)\n\n\
         def place(items):\n    \"\"\"@intent SHOP-1\"\"\"\n    total = pricing.total(items)\n    for i in items:\n        check(i)\n    return Repo().save(Order(total))\n\n\
         def check(i):\n    return i if i else None\n",
    ),
    (
        "shop/pricing.py",
        "def total(items):\n    return sum(i.price for i in items)\n",
    ),
    (
        "shop/models.py",
        "class Base:\n    def conn(self):\n        return 1\n\nclass Order(Base):\n    pass\n",
    ),
    (
        "main.py",
        "from shop import place\nimport shop.pricing\n\nplace([])\nshop.pricing.total([])\n",
    ),
];

#[test]
fn nodes_by_kind_and_id() {
    let out = build(SHOP);
    let orders = out
        .facts
        .iter()
        .find(|f| f.file == "shop/orders.py")
        .unwrap();
    let got: Vec<(&str, &str)> = orders
        .nodes
        .iter()
        .map(|n| (n.id.as_str(), n.kind.as_str()))
        .collect();
    assert_eq!(
        got,
        [
            ("shop/orders.py", "module"),
            ("shop/orders.py#Repo", "class"),
            ("shop/orders.py#Repo.save", "method"),
            ("shop/orders.py#place", "function"),
            ("shop/orders.py#check", "function"),
        ]
    );
    let place = &orders.nodes[3];
    assert_eq!(place.intents, ["SHOP-1"]);
    assert!(place.exported && !orders.nodes[2].exported);
    assert_eq!(
        orders.nodes[4].metrics.complexity, 2,
        "one conditional expression"
    );
}

#[test]
fn imports_resolve_absolute_relative_and_packages() {
    let out = build(SHOP);
    assert_eq!(
        edges(&out, "imports"),
        [
            (
                "main.py".into(),
                "shop/__init__.py".into(),
                "resolved".into()
            ),
            (
                "main.py".into(),
                "shop/pricing.py".into(),
                "resolved".into()
            ),
            (
                "shop/__init__.py".into(),
                "shop/orders.py".into(),
                "resolved".into()
            ),
            (
                "shop/orders.py".into(),
                "shop/models.py".into(),
                "resolved".into()
            ),
            (
                "shop/orders.py".into(),
                "shop/pricing.py".into(),
                "resolved".into()
            ),
        ]
    );
    // os is Python's own; psycopg2 is a package
    assert_eq!(
        out.packages,
        [("shop/orders.py".to_string(), "psycopg2".to_string(), 1)]
    );
}

#[test]
fn calls_through_imports_self_and_bases() {
    let out = build(SHOP);
    let calls = edges(&out, "calls");
    let has = |from: &str, to: &str| {
        calls
            .iter()
            .any(|(f, t, c)| f == from && t == to && c == "resolved")
    };
    assert!(
        has("shop/orders.py#place", "shop/pricing.py#total"),
        "module attribute: {calls:#?}"
    );
    assert!(
        has("shop/orders.py#place", "shop/orders.py#check"),
        "same file"
    );
    assert!(
        has("shop/orders.py#place", "shop/orders.py#Repo"),
        "a class called"
    );
    assert!(
        has("shop/orders.py#place", "shop/models.py#Order"),
        "from-import"
    );
    assert!(
        has("shop/orders.py#Repo.save", "shop/models.py#Base.conn"),
        "self. through a base: {calls:#?}"
    );
    assert!(
        has("main.py", "shop/orders.py#place"),
        "re-exported by __init__"
    );
    assert!(has("main.py", "shop/pricing.py#total"), "dotted import");
    assert_eq!(
        edges(&out, "extends"),
        [
            (
                "shop/models.py#Order".into(),
                "shop/models.py#Base".into(),
                "resolved".into()
            ),
            (
                "shop/orders.py#Repo".into(),
                "shop/models.py#Base".into(),
                "guessed".into()
            ),
        ]
    );
    let in_loop = out
        .edges
        .iter()
        .find(|e| e.to == "shop/orders.py#check")
        .unwrap();
    assert!(in_loop.in_loop);
}

#[test]
fn tests_and_parse_errors() {
    let out = build(&[
        (
            "tests/test_orders.py",
            "def test_place():\n    assert True\n\nclass TestRepo:\n    def test_save(self):\n        pass\n",
        ),
        ("bad.py", "def f(:\n    pass\n"),
    ]);
    let t = out
        .facts
        .iter()
        .find(|f| f.file == "tests/test_orders.py")
        .unwrap();
    let kinds: Vec<&str> = t.nodes.iter().map(|n| n.kind.as_str()).collect();
    assert_eq!(kinds, ["module", "test", "class", "test"]);
    let bad = out.facts.iter().find(|f| f.file == "bad.py").unwrap();
    assert_eq!(bad.parse_error.as_deref(), Some("syntax error at line 1"));
}

#[test]
fn packages_from_manifests() {
    let f = MemFiles::with(&[
        (
            "pyproject.toml",
            "[tool.ruff]\nname = \"no\"\n[project]\nname = \"platform\"\n",
        ),
        (
            "services/api/pyproject.toml",
            "[tool.poetry]\nname = \"api\"\n",
        ),
        (
            "services/worker/setup.py",
            "setup(name='worker', version='1')\n",
        ),
        ("services/bare/setup.py", "setup()\n"),
        ("a/b/c/pyproject.toml", ""),
    ]);
    let got: Vec<(String, String)> = PyFrontend
        .packages(&f)
        .into_iter()
        .map(|p| (p.dir, p.name))
        .collect();
    assert_eq!(
        got,
        [
            (".".into(), "platform".into()),
            ("services/api".into(), "api".into()),
            ("services/bare".into(), "bare".into()),
            ("services/worker".into(), "worker".into()),
        ]
    );
}
