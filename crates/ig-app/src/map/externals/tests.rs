use crate::build::build_graph;
use crate::map::{ArrowKind, BoxKind, FactBox, Level, arrows, facts};
use ig_core::config::Config;
use ig_core::model::{Metrics, Node, Range};
use ig_core::ports::{FileFacts, FrontendOutput, LanguageFrontend, PortError};
use ig_core::testing::MemFiles;
use ig_core::workspace::{SourceFile, WorkspaceInfo};

/// A module node per file; each source line names a package the file imports.
struct Imports;

impl LanguageFrontend for Imports {
    fn handles(&self, path: &str, _: &Config) -> bool {
        path.ends_with(".ts")
    }
    fn build(
        &self,
        files: &[(SourceFile, String)],
        _: &WorkspaceInfo,
        _: &Config,
    ) -> FrontendOutput {
        FrontendOutput {
            facts: files
                .iter()
                .map(|(f, _)| FileFacts {
                    file: f.path.clone(),
                    nodes: vec![Node {
                        id: f.path.clone(),
                        kind: "module".into(),
                        name: f.path.clone(),
                        file: f.path.clone(),
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
                    }],
                    ..FileFacts::default()
                })
                .collect(),
            edges: vec![],
            parse_errors: vec![],
            packages: files
                .iter()
                .flat_map(|(f, src)| src.lines().map(|p| (f.path.clone(), p.to_string(), 1)))
                .collect(),
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

/// Containers of the repo `files` (with `ig.yaml` text `yaml`), and the arrows between them.
fn map(files: &[(&str, &str)], yaml: &str) -> (Vec<FactBox>, Vec<crate::map::MapArrow>) {
    let files = MemFiles::with(files);
    let cfg = Config::parse(yaml).unwrap();
    let b = build_graph(&[&Imports], &files, &cfg).unwrap();
    let boxes = facts(&b.workspace, &b.graph, &cfg);
    let a = arrows(&boxes, &b.graph);
    (boxes, a)
}

fn containers(boxes: &[FactBox]) -> Vec<(&str, &str, BoxKind)> {
    boxes
        .iter()
        .filter(|b| b.level == Level::Container && b.package.is_none())
        .map(|b| (b.id.as_str(), b.guess.title.as_str(), b.guess.kind))
        .collect()
}

const PKG: (&str, &str) = ("package.json", r#"{"name":"app"}"#);

#[test]
fn detects_postgres_from_pg() {
    let (b, a) = map(&[PKG, ("src/db/repo.ts", "pg\nlodash")], "");
    assert_eq!(containers(&b), [("postgres", "PostgreSQL", BoxKind::Store)]);
    let pg = b.iter().find(|b| b.id == "postgres").unwrap();
    assert_eq!(pg.guess.logo.as_deref(), Some("postgresql"));
    let arrow = a.iter().find(|a| a.to == "postgres").unwrap();
    assert_eq!((arrow.from.as_str(), arrow.kind), ("app", ArrowKind::Data));
}

#[test]
fn declared_system_wins() {
    let yaml = "blueprint:\n  systems:\n    Postgres: [pg]\n";
    let (b, a) = map(&[PKG, ("src/a.ts", "pg"), ("src/b.ts", "postgres")], yaml);
    assert_eq!(containers(&b), [("postgres", "Postgres", BoxKind::Store)]);
    // `postgres` (not declared) stands for the same system: it joins the declared one.
    assert_eq!(a.iter().find(|a| a.to == "postgres").unwrap().count, 2);
}

#[test]
fn prefix_packages() {
    let (b, a) = map(&[PKG, ("src/a.ts", "@sentry/node")], "");
    assert_eq!(containers(&b), [("sentry", "Sentry", BoxKind::External)]);
    assert_eq!(a[0].kind, ArrowKind::Network);
}

#[test]
fn only_tests_import_it() {
    let (b, _) = map(&[PKG, ("test/a.ts", "redis")], "tests: ['test/**']\n");
    assert_eq!(containers(&b), []);
}

#[test]
fn prisma_resolves_provider() {
    let schema = "datasource db {\n  provider = \"postgresql\"\n  url = env(\"DB\")\n}\n";
    let (b, _) = map(
        &[
            PKG,
            ("src/a.ts", "@prisma/client"),
            ("prisma/schema.prisma", schema),
        ],
        "",
    );
    assert_eq!(containers(&b), [("postgres", "PostgreSQL", BoxKind::Store)]);
    let (b, _) = map(&[PKG, ("src/a.ts", "@prisma/client")], "");
    assert_eq!(
        containers(&b),
        [("prisma", "Database (Prisma)", BoxKind::Store)]
    );
}
