use super::*;

fn run(path: &str, src: &str) -> FileFacts {
    let tc = vec!["describe".to_string(), "it".to_string(), "test".to_string()];
    let input = FileInput {
        path,
        source: src,
        package: Some("p"),
        embedded: false,
        is_test: path.contains(".test."),
        test_calls: &tc,
        http_calls: &[],
    };
    extract(&input, &|_| None)
}

fn ids(f: &FileFacts) -> Vec<&str> {
    f.nodes.iter().map(|n| n.id.as_str()).collect()
}

#[test]
fn node_ids() {
    let f = run(
        "a.ts",
        "export function f() { function inner() {} }\nfunction f() {}\nconst o = { m() {}, n: () => 1, x: 1 };\nclass C { constructor() {} get v() { return 1 } set v(x) {} }\nexport default () => 1;\nconst w = wrap(() => 1);\n",
    );
    assert_eq!(
        ids(&f),
        [
            "a.ts",
            "a.ts#f",
            "a.ts#f.inner",
            "a.ts#f~2",
            "a.ts#o.m",
            "a.ts#o.n",
            "a.ts#C",
            "a.ts#C.constructor",
            "a.ts#C.v",
            "a.ts#C.v~2",
            "a.ts#default",
            "a.ts#w"
        ]
    );
    assert!(f.nodes[1].exported && !f.nodes[3].exported && f.nodes[10].exported);
    assert_eq!(f.nodes[2].parent.as_deref(), Some("a.ts#f"));
    assert_eq!(f.nodes[4].parent.as_deref(), Some("a.ts"));
}

#[test]
fn test_ids_and_intents() {
    let f = run(
        "a.test.ts",
        "describe('s', () => {\n  it('x', () => {\n    // @intent A-1, B-2\n    go();\n  });\n  it('x', () => {});\n});\n",
    );
    assert_eq!(
        ids(&f),
        [
            "a.test.ts",
            "a.test.ts#s",
            "a.test.ts#s > x",
            "a.test.ts#s > x~2"
        ]
    );
    assert_eq!(f.nodes[2].intents, ["A-1", "B-2"]);
    assert_eq!(
        f.nodes[2].range,
        Range {
            start_line: 2,
            end_line: 5
        }
    );
}

#[test]
fn hashes_ignore_whitespace_and_leading_comments() {
    let a = run(
        "a.ts",
        "// @intent X-1\nconst f = (a) => {\n  return a + 1;\n};\n",
    );
    let b = run("a.ts", "const f = (a) =>   {   return a + 1; };");
    assert_eq!(a.nodes[1].hash, b.nodes[1].hash);
    assert_eq!(a.nodes[1].intents, ["X-1"]);
    assert_eq!(
        a.nodes[1].range,
        Range {
            start_line: 2,
            end_line: 4
        }
    );
    let c = run("a.ts", "const f = (a) => { return a + 2; };");
    assert_ne!(a.nodes[1].hash, c.nodes[1].hash);
    assert_eq!(a.nodes[0].range.end_line, 4);
}

#[test]
fn complexity_counts_innermost() {
    let f = run(
        "a.ts",
        "function f(a) { if (a && b) {} const g = () => a ?? (b ? 1 : 2); switch (a) { case 1: case 2: default: } }\n",
    );
    let by = |id: &str| {
        f.nodes
            .iter()
            .find(|n| n.id == id)
            .unwrap()
            .metrics
            .complexity
    };
    assert_eq!(by("a.ts#f"), 1 + 2 + 2);
    assert_eq!(by("a.ts#f.g"), 1 + 2);
}

#[test]
fn callees() {
    let f = run(
        "a.ts",
        "import { x } from './x';\nimport * as ns from './ns';\nfunction local() {}\nfunction f(p) { local(); x(); ns.y(); p(); obj.m(); }\nclass A extends B { m() { this.n(); super.o(); } n() {} }\n",
    );
    let c: Vec<String> = f
        .refs
        .iter()
        .map(|r| format!("{} {} {:?}", r.from, r.kind, r.callee))
        .collect();
    let has = |s: &str| assert!(c.iter().any(|x| x.contains(s)), "missing {s} in {c:#?}");
    has(r#"a.ts#f calls Node("a.ts#local")"#);
    has(r#"a.ts#f calls Import("./x", Named("x"))"#);
    has(r#"a.ts#f calls Import("./ns", Named("y"))"#);
    has(r#"a.ts#f calls Guess("p")"#);
    has(r#"a.ts#f calls Guess("m")"#);
    has(r#"a.ts#A.m calls Member { class: "a.ts#A", name: "n", skip_self: false }"#);
    has(r#"a.ts#A.m calls Member { class: "a.ts#A", name: "o", skip_self: true }"#);
    has(r#"a.ts#A extends Free("B")"#);
}
