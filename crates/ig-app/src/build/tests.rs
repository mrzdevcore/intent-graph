use super::*;
use ig_core::model::{Metrics, Range};
use ig_core::ports::FrontendOutput;
use ig_core::testing::MemFiles;

fn node(id: &str, file: &str) -> Node {
    Node {
        id: id.into(),
        kind: if id == file { "module" } else { "function" }.into(),
        name: id.rsplit('#').next().unwrap().into(),
        file: file.into(),
        range: Range {
            start_line: 1,
            end_line: 1,
        },
        parent: None,
        package: None,
        exported: false,
        hash: String::new(),
        metrics: Metrics {
            loc: 1,
            complexity: 1,
        },
        intents: vec![],
    }
}

fn edge(from: &str, kind: &str, to: &str, confidence: &str, line: u32) -> Edge {
    Edge {
        from: from.into(),
        to: to.into(),
        kind: kind.into(),
        confidence: confidence.into(),
        file: "src/a.ts".into(),
        line,
        namespace: false,
        reexport: false,
        in_loop: false,
    }
}

/// One node per file plus `src/a.ts#f`, and canned edges.
struct Fake;

impl LanguageFrontend for Fake {
    fn handles(&self, path: &str, _: &Config) -> bool {
        path.ends_with(".ts")
    }
    fn build(
        &self,
        files: &[(SourceFile, String)],
        _: &WorkspaceInfo,
        _: &Config,
    ) -> FrontendOutput {
        let mut facts: Vec<FileFacts> = files
            .iter()
            .map(|(f, src)| FileFacts {
                file: f.path.clone(),
                nodes: vec![node(&f.path, &f.path)],
                parse_error: (src == "broken").then(|| "bad".into()),
                ..FileFacts::default()
            })
            .collect();
        facts[0].nodes.push(node("src/a.ts#f", "src/a.ts"));
        FrontendOutput {
            facts,
            edges: vec![
                edge("src/a.ts#f", "calls", "src/b.ts", "guessed", 3),
                edge("src/a.ts#f", "calls", "src/b.ts", "resolved", 2),
                edge("src/a.ts", "imports", "test/stub.ts", "resolved", 1),
                edge("src/a.ts#f", "calls", "test/stub.ts", "resolved", 1),
                edge("src/a.ts#f", "calls", "src/a.ts#Iface", "resolved", 1),
            ],
            parse_errors: vec![],
            packages: vec![
                ("src/a.ts".into(), "knex".into(), 1),
                ("src/a.ts".into(), "knex".into(), 2),
                ("test/stub.ts".into(), "knex".into(), 1),
            ],
            notices: vec![],
        }
    }
    fn prepare_checkout(
        &self,
        _: &WorkspaceInfo,
        _: &std::path::Path,
        _: &Config,
        _: bool,
    ) -> Result<(), PortError> {
        Ok(())
    }
}

#[test]
fn assembles_merges_and_filters() {
    let files = MemFiles::with(&[
        ("src/a.ts", ""),
        ("src/b.ts", "broken"),
        ("test/stub.ts", ""),
    ]);
    let config =
        Config::parse("tests: ['test/**']\nblueprint:\n  systems:\n    PostgreSQL: [knex]\n")
            .unwrap();
    let b = build_graph(&[&Fake], None, &files, &config).unwrap();
    let edges: Vec<String> = b
        .graph
        .edges
        .iter()
        .map(|e| format!("{} {} {} {} {}", e.from, e.kind, e.to, e.confidence, e.line))
        .collect();
    // Merged (resolved wins, first line), imports into tests kept, other edges into tests
    // and edges to non-nodes dropped; `uses` edges to the system node.
    assert_eq!(
        edges,
        [
            "src/a.ts imports test/stub.ts resolved 1",
            "src/a.ts uses system:PostgreSQL resolved 1",
            "src/a.ts#f calls src/b.ts resolved 2",
            "test/stub.ts uses system:PostgreSQL resolved 1",
        ]
    );
    assert!(b.graph.nodes.iter().any(|n| n.id == "system:PostgreSQL"));
    assert_eq!(b.graph.parse_errors.len(), 1);
    assert_eq!(b.graph.parse_errors[0].file, "src/b.ts");
    // Test files don't count towards packages.
    assert_eq!(b.extras.packages.get("knex"), Some(&1));
}

/// A reader that keeps the paths it was given, as one table each.
struct Paths;
impl SchemaReader for Paths {
    fn wants(&self, path: &str) -> bool {
        path.ends_with(".sql")
    }
    fn read(&self, files: &[(String, String)]) -> (Schema, Vec<String>) {
        let tables = (files.iter())
            .map(|(p, _)| ig_core::model::Table {
                name: p.clone(),
                kind: "table".into(),
                source: "sql".into(),
                file: p.clone(),
                range: ig_core::model::Range {
                    start_line: 1,
                    end_line: 1,
                },
                columns: vec![],
                reads: vec![],
            })
            .collect();
        let dialect = Some("postgresql".into());
        (Schema { dialect, tables }, vec![])
    }
}

#[test]
fn the_schema_is_read_outside_tests_ignored_build_output_and_dot_folders() {
    let files = MemFiles::with(&[
        ("src/a.ts", ""),
        ("db/V1__init.sql", ""),
        ("dist/db/V1__init.sql", ""),
        ("test/fixture.sql", ""),
        ("vendor/x.sql", ""),
        (".claude/worktrees/b/db/V1__init.sql", ""),
    ]);
    let config = Config::parse("tests: ['test/**']\nignore: ['vendor/**']\n").unwrap();
    let b = build_graph(&[&Fake], Some(&Paths), &files, &config).unwrap();
    let read: Vec<&str> = b.schema.tables.iter().map(|t| t.name.as_str()).collect();
    assert_eq!(read, ["db/V1__init.sql"]);
    // no database imported: one named for the dialect holds the tables
    assert!(b.graph.nodes.iter().any(|n| n.id == "system:PostgreSQL"));
}

#[test]
fn system_categories() {
    let p = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    assert_eq!(system_category("PostgreSQL", &p(&["pg"])), "database");
    assert_eq!(system_category("MongoDB", &p(&["mongoose"])), "database");
    assert_eq!(
        system_category("Elastic APM", &p(&["elastic-apm-node"])),
        "monitoring"
    );
    assert_eq!(
        system_category("Search", &p(&["@elastic/elasticsearch"])),
        "search"
    );
    assert_eq!(
        system_category("Auth0", &p(&["@auth0/auth0-react"])),
        "auth"
    );
    assert_eq!(system_category("Contentful", &p(&["contentful"])), "cms");
    assert_eq!(
        system_category("Mixpanel", &p(&["mixpanel-browser"])),
        "analytics"
    );
    assert_eq!(system_category("Shop BFF", &p(&["@shop/sdk"])), "service");
}
