use super::*;

#[test]
fn scoping() {
    use ig_core::model::{Edge, Metrics, Node, Range};
    let node = |id: &str, file: &str, hash: &str| Node {
        id: id.into(),
        kind: "function".into(),
        name: id.into(),
        file: file.into(),
        range: Range {
            start_line: 1,
            end_line: 1,
        },
        parent: None,
        package: None,
        exported: false,
        hash: hash.into(),
        metrics: Metrics {
            loc: 1,
            complexity: 1,
        },
        intents: vec![],
    };
    let edge = |f: &str, t: &str| Edge {
        from: f.into(),
        to: t.into(),
        kind: "calls".into(),
        confidence: "resolved".into(),
        file: "a.ts".into(),
        line: 1,
        namespace: false,
        reexport: false,
    };
    let mut base = Graph::empty();
    base.nodes = vec![
        node("a", "a.ts", "1"),
        node("b", "b.ts", "1"),
        node("c", "c.ts", "1"),
    ];
    base.edges = vec![edge("a", "b")];
    let mut head = base.clone();
    head.nodes[1].hash = "2".into();
    head.edges.push(edge("c", "a"));
    let d = GraphDiff::new(Some(&base), &head);
    let f = |file: &str| Finding::new("r", Severity::Error, "t", file, 1, "m".into());
    assert!(in_scope(&f("a.ts").edge("c", "a"), &d), "new edge");
    assert!(
        !in_scope(&f("b.ts").node("b").edge("a", "b"), &d),
        "old edge wins over changed node"
    );
    assert!(in_scope(&f("b.ts").node("b"), &d), "changed node");
    assert!(!in_scope(&f("a.ts").node("a"), &d), "unchanged node");
    assert!(in_scope(&f("b.ts"), &d), "file with a changed node");
    assert!(!in_scope(&f("c.ts"), &d), "untouched file");
}

#[test]
fn snippet_marks_line() {
    let lines = ["a", "b", "c", "d", "e"];
    assert_eq!(snippet(&lines, 1).unwrap(), "> 1 | a\n  2 | b\n  3 | c");
    assert!(snippet(&lines, 9).is_none());
}

mod fakes {
    use super::*;
    use crate::testing::{Fake, env};
    use ig_core::ports::{CheckerOutput, Files};
    use ig_core::testing::{FakeRunner, FakeVcs, MemFiles};

    const CONFIG: &str = "layers:\n  core: ['core/**']\n  ui: ['ui/**']\nrules:\n  - forbid: \"core -> ui\"\ncheckers:\n  - name: lint\n    format: lines\n    run: \"lint {files}\"\n";

    #[test]
    fn reports_a_new_arch_violation_and_checker_findings_against_the_base() {
        // The base (checked out under `.wt/`) has no core -> ui import; the working tree adds one.
        let files = MemFiles::with(&[
            ("ig.yaml", CONFIG),
            ("core/a.ts", "use ui/b.ts\nfn run\n"),
            ("ui/b.ts", "fn b\n"),
            (".wt/ig.yaml", CONFIG),
            (".wt/core/a.ts", "fn run\n"),
            (".wt/ui/b.ts", "fn b\n"),
        ]);
        let vcs = FakeVcs {
            head: Some("h".into()),
            base: Some("b".into()),
            commits: ["h".to_string(), "b".to_string()].into(),
            checkout_root: Some(".wt".into()),
            changed: vec!["core/a.ts".into(), "gone.ts".into()],
            ..FakeVcs::default()
        };
        let lint = |line| {
            Finding::new(
                "lint",
                Severity::Warning,
                "lint",
                "core/a.ts",
                line,
                "x".into(),
            )
        };
        let runner = FakeRunner {
            outputs: [(
                "lint".to_string(),
                CheckerOutput {
                    findings: vec![lint(1), lint(2)],
                    ..CheckerOutput::default()
                },
            )]
            .into(),
            ..FakeRunner::default()
        };
        let fe = Fake::default();
        let run = run(&env(&files, Some(&vcs), &runner, &fe), &Opts::default()).unwrap();
        let r = &run.result;
        assert_eq!(
            (r.base.as_deref(), r.head.as_deref()),
            (Some("b"), Some("h"))
        );
        let arch: Vec<&Finding> = r
            .findings
            .iter()
            .filter(|f| f.rule_id == "ig/arch")
            .collect();
        assert_eq!(arch.len(), 1, "{:?}", r.findings);
        assert!(arch[0].message.contains("layer core must not depend on ui"));
        assert_eq!(
            arch[0].snippet.as_deref(),
            Some("> 1 | use ui/b.ts\n  2 | fn run")
        );
        assert!(r.blocking);
        assert_eq!(r.edge_changes.len(), 1);
        // `{files}` got the changed files that exist. Findings are pinned to the innermost node:
        // the changed module's counts, the unchanged function's is out of scope.
        let runs = runner.runs.lock().unwrap();
        assert_eq!(runs[0], ("lint".into(), Some(vec!["core/a.ts".into()])));
        let lint: Vec<_> = r.findings.iter().filter(|f| f.rule_id == "lint").collect();
        assert_eq!(lint.len(), 1);
        assert_eq!(lint[0].node_id.as_deref(), Some("core/a.ts"));
        // State for the viewer, `ig except` and the hook.
        assert!(files.exists(".ig/graph.json") && files.exists(".ig/findings.json"));
        let keys = files.files.lock().unwrap();
        assert!(
            keys.keys().any(|k| k.starts_with(".ig/cache/b-")),
            "base graph cached"
        );
    }

    #[test]
    fn outside_a_repository_everything_is_added_and_head_needs_one() {
        let files = MemFiles::with(&[
            ("ig.yaml", CONFIG),
            ("core/a.ts", "use ui/b.ts\n"),
            ("ui/b.ts", ""),
        ]);
        let (runner, fe) = (FakeRunner::default(), Fake::default());
        let e = env(&files, None, &runner, &fe);
        let opts = Opts {
            all: true,
            ..Opts::default()
        };
        let r = run(&e, &opts).unwrap().result;
        assert_eq!(
            r.notices,
            ["not a git repository: every node counts as added"]
        );
        assert_eq!(r.counts.added, 2);
        assert!(r.findings.iter().any(|f| f.rule_id == "ig/arch"));
        let head = Opts {
            head: Some("x".into()),
            ..Opts::default()
        };
        let err = run(&e, &head).err().unwrap().to_string();
        assert_eq!(err, "--base/--head need a git repository");
    }

    #[test]
    fn head_without_the_ig_root_keeps_the_old_message() {
        let files = MemFiles::with(&[("core/a.ts", "")]);
        let vcs = FakeVcs {
            head: Some("h".into()),
            commits: ["h".to_string(), "old".to_string()].into(),
            checkout_root: None,
            ..FakeVcs::default()
        };
        let (runner, fe) = (FakeRunner::default(), Fake::default());
        let opts = Opts {
            head: Some("old".into()),
            ..Opts::default()
        };
        let err = run(&env(&files, Some(&vcs), &runner, &fe), &opts)
            .err()
            .unwrap();
        assert!(err.downcast_ref::<HeadRootMissing>().is_some());
        assert_eq!(
            err.to_string(),
            std::io::Error::from_raw_os_error(2).to_string()
        );
    }
}
