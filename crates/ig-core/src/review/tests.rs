use super::module_graph::tarjan;
use super::*;
use crate::config::Config;
use crate::model::{Edge, Metrics, Range};

/// Modules "m/x.ts" with members "m/x.ts#f"; edges as (from node, to node, kind).
fn graph(nodes: &[(&str, u32)], edges: &[(&str, &str, &str)]) -> Graph {
    let mut g = Graph::empty();
    for (id, cx) in nodes {
        let (file, member) = id
            .split_once('#')
            .map(|(f, m)| (f, Some(m)))
            .unwrap_or((id, None));
        g.nodes.push(Node {
            id: id.to_string(),
            kind: if member.is_some() {
                "function"
            } else {
                "module"
            }
            .into(),
            name: member.unwrap_or(file).into(),
            file: file.into(),
            range: Range {
                start_line: 1,
                end_line: 1,
            },
            parent: member.map(|_| file.to_string()),
            package: None,
            exported: member.is_some(),
            hash: String::new(),
            metrics: Metrics {
                loc: if member.is_some() { 1 } else { *cx },
                complexity: *cx,
            },
            intents: vec![],
        });
    }
    for (f, t, k) in edges {
        g.edges.push(Edge {
            from: f.to_string(),
            to: t.to_string(),
            kind: k.to_string(),
            confidence: "resolved".into(),
            file: f.split('#').next().unwrap().into(),
            line: 1,
            namespace: false,
            reexport: false,
        });
    }
    g
}

fn with_ctx<T>(cfg: &str, f: impl FnOnce(&Ctx) -> T) -> T {
    let config = Config::parse(cfg).unwrap();
    let tests = crate::workspace::globset(&config.tests).unwrap();
    let eps = crate::workspace::globset(&config.entry_points).unwrap();
    let pkgs = [Package {
        name: "p".into(),
        dir: ".".into(),
        tsconfig: None,
        source_entry: None,
    }];
    f(&Ctx {
        config: &config,
        tests: &tests,
        packages: &pkgs,
        entry_points: &eps,
    })
}

fn rules(items: &[ReviewItem]) -> Vec<&str> {
    items.iter().map(|i| i.rule_id.as_str()).collect()
}

#[test]
fn cycle_and_unused_and_oversized() {
    let g = graph(
        &[
            ("a.ts", 600),
            ("a.ts#f", 30),
            ("a.ts#g", 1),
            ("b.ts", 1),
            ("b.ts#h", 1),
            ("b.test.ts", 1),
        ],
        &[
            ("a.ts", "b.ts", "imports"),
            ("b.ts", "a.ts", "imports"),
            ("b.ts#h", "a.ts#f", "calls"),
            ("b.test.ts", "a.ts#g", "calls"),
        ],
    );
    let items = with_ctx("{}", |ctx| review(&g, ctx, &[]));
    let cyc = items.iter().find(|i| i.rule_id == "struct/cycle").unwrap();
    assert_eq!(cyc.evidence, "a.ts → b.ts → a.ts");
    let unused = items
        .iter()
        .find(|i| i.rule_id == "struct/unused-export" && i.modules == ["a.ts"])
        .unwrap();
    assert_eq!(unused.evidence, "unused: g; used only by tests: g");
    let big = items
        .iter()
        .find(|i| i.rule_id == "struct/oversized")
        .unwrap();
    assert_eq!(big.evidence, "600 lines; f has complexity 30");
}

#[test]
fn hubs_and_regressions() {
    // h is used by 5 modules and uses 5.
    let mut nodes = vec![("h.ts", 1), ("h.ts#h", 1)];
    let names: Vec<String> = (0..10).map(|i| format!("m{i}.ts")).collect();
    let fns: Vec<String> = names.iter().map(|n| format!("{n}#f")).collect();
    for (n, f) in names.iter().zip(&fns) {
        nodes.push((n, 1));
        nodes.push((f, 1));
    }
    let mut edges = vec![];
    for f in &fns[..5] {
        edges.push((f.as_str(), "h.ts#h", "calls"));
    }
    for f in &fns[5..] {
        edges.push(("h.ts#h", f.as_str(), "calls"));
    }
    let head = graph(&nodes, &edges);
    let base = graph(&nodes, &edges[..4]);
    with_ctx("review: { hubMin: 5, hubPercentile: 0.5 }", |ctx| {
        let items = review(&head, ctx, &[]);
        assert!(rules(&items).contains(&"struct/hub"));
        let reg = regressions(&base, &head, ctx);
        assert_eq!(
            reg.iter().map(|f| f.rule_id.as_str()).collect::<Vec<_>>(),
            ["struct/new-hub"]
        );
        assert_eq!(
            reg[0].fingerprint,
            fingerprint("struct/new-hub", &["h.ts".to_string()])
        );
    });
}

#[test]
fn new_cycle_only_when_not_in_base_scc() {
    let n = [("a.ts", 1), ("b.ts", 1), ("c.ts", 1)];
    let base = graph(
        &n,
        &[("a.ts", "b.ts", "imports"), ("b.ts", "a.ts", "imports")],
    );
    let same = graph(
        &n,
        &[
            ("a.ts", "b.ts", "imports"),
            ("b.ts", "a.ts", "imports"),
            ("c.ts", "a.ts", "imports"),
        ],
    );
    let grown = graph(
        &n,
        &[
            ("a.ts", "b.ts", "imports"),
            ("b.ts", "c.ts", "imports"),
            ("c.ts", "a.ts", "imports"),
            ("b.ts", "a.ts", "imports"),
        ],
    );
    with_ctx("{}", |ctx| {
        assert!(regressions(&base, &same, ctx).is_empty());
        assert_eq!(
            regressions(&base, &grown, ctx)[0].rule_id,
            "struct/new-cycle"
        );
    });
}

#[test]
fn cohesion_groups() {
    // Members a,b call each other; c,d share an outside caller; e alone.
    let g = graph(
        &[
            ("m.ts", 1),
            ("m.ts#a", 1),
            ("m.ts#b", 1),
            ("m.ts#c", 1),
            ("m.ts#d", 1),
            ("m.ts#e", 1),
            ("o.ts", 1),
            ("o.ts#x", 1),
        ],
        &[
            ("m.ts#a", "m.ts#b", "calls"),
            ("o.ts#x", "m.ts#c", "calls"),
            ("o.ts#x", "m.ts#d", "calls"),
            ("m.ts", "m.ts#e", "calls"),
        ],
    );
    with_ctx("{}", |ctx| {
        let mg = ModuleGraph::new(&g, ctx);
        assert_eq!(mg.cohesion("m.ts", ctx), (5, 2));
    });
    let items = with_ctx(
        "review: { cohesionGroups: 2, cohesionMinMembers: 5 }",
        |ctx| review(&g, ctx, &[]),
    );
    assert!(rules(&items).contains(&"struct/low-cohesion"));
}

#[test]
fn unstable_dependency() {
    // Dir "s" is stable (5 dirs depend on it) and depends on dir "u", which only depends outward.
    let mut nodes = vec![("src/s/a.ts", 1), ("src/u/b.ts", 1), ("src/z/z.ts", 1)];
    let users: Vec<String> = (0..5).map(|i| format!("src/d{i}/x.ts")).collect();
    for u in &users {
        nodes.push((u, 1));
    }
    let mut edges = vec![
        ("src/s/a.ts", "src/u/b.ts", "imports"),
        ("src/u/b.ts", "src/z/z.ts", "imports"),
    ];
    for u in &users {
        edges.push((u.as_str(), "src/s/a.ts", "imports"));
    }
    let g = graph(&nodes, &edges);
    let items = with_ctx("{}", |ctx| review(&g, ctx, &[]));
    let it = items
        .iter()
        .find(|i| i.rule_id == "struct/unstable-dependency")
        .unwrap();
    assert_eq!(it.title, "Stable src/s depends on unstable src/u");
}

#[test]
fn change_coupling_from_history() {
    let history: Vec<Vec<String>> = (0..5)
        .map(|_| vec!["src/a/x.ts".to_string(), "src/b/y.ts".to_string()])
        .collect();
    let g = graph(&[("src/a/x.ts", 1), ("src/b/y.ts", 1)], &[]);
    let items = with_ctx("{}", |ctx| review(&g, ctx, &history));
    let it = items
        .iter()
        .find(|i| i.rule_id == "struct/change-coupling")
        .unwrap();
    assert_eq!(
        it.evidence,
        "5 shared commits out of 5, with no dependency between them"
    );
}

#[test]
fn tarjan_finds_cycles() {
    let adj = vec![vec![1], vec![2], vec![0], vec![0]];
    let c: Vec<Vec<usize>> = tarjan(&adj).into_iter().filter(|c| c.len() > 1).collect();
    assert_eq!(c.len(), 1);
    assert_eq!(c[0].len(), 3);
}

#[test]
fn rank_caps_per_rule() {
    let mk = |rule: &str, score| ReviewItem {
        rule_id: rule.into(),
        title: String::new(),
        evidence: String::new(),
        why: String::new(),
        fix: String::new(),
        modules: vec![],
        score,
        fingerprint: String::new(),
        fan_in: 0,
    };
    let items = vec![mk("a", 9), mk("a", 8), mk("a", 7), mk("a", 6), mk("b", 1)];
    let (head, rest) = rank(items, 10);
    assert_eq!(head.len(), 4);
    assert_eq!(rest.len(), 1);
    assert_eq!(rest[0].score, 6);
}
