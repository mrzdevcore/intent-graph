use super::*;
use ig_core::testing::MemFiles;

#[test]
fn go_frontend_meets_the_contract() {
    ig_core::testing::frontend_contract(
        &GoFrontend,
        "app/a.go",
        "package app\n\nfunc F() int { return g() }\n\nfunc g() int { return 1 }\n",
    );
}

const IG_TEST: &[(&str, &str)] = &[
    (
        "go.mod",
        "module ig-test\n\ngo 1.23\n\nrequire (\n\tgithub.com/jackc/pgx/v5 v5.7.0\n)\n",
    ),
    (
        "cmd/api/main.go",
        r#"package main

import (
	"context"
	"log/slog"
	"net/http"

	"ig-test/internal/api"
	"ig-test/internal/store"
	"ig-test/internal/user"
)

func main() {
	var s user.Store
	pg, err := store.NewPostgres(context.Background(), "")
	if err != nil {
		slog.Error("connect", "err", err)
	}
	defer pg.Close()
	s = pg
	_ = http.ListenAndServe(":8080", api.New(s))
}
"#,
    ),
    (
        "cmd/cli/main.go",
        "package main\n\nimport (\n\t\"fmt\"\n\tc \"ig-test/internal/client\"\n)\n\nfunc main() {\n\tfor _, a := range []string{\"x\"} {\n\t\tfmt.Println(c.New(a).List())\n\t}\n}\n",
    ),
    (
        "cmd/mcp/main.go",
        "package main\n\nimport (\n\t\"github.com/modelcontextprotocol/go-sdk/mcp\"\n\t\"ig-test/internal/client\"\n)\n\nfunc main() {\n\t_ = mcp.NewServer(nil, nil)\n\t_ = client.New(\"\")\n}\n",
    ),
    (
        "cmd/mcp/main_test.go",
        "package main\n\nimport \"testing\"\n\nfunc TestTools(t *testing.T) {\n\tmain()\n}\n\nfunc helper() {}\n",
    ),
    (
        "internal/api/api.go",
        "package api\n\nimport (\n\t\"net/http\"\n\n\t\"ig-test/internal/user\"\n)\n\ntype handler struct {\n\tstore user.Store\n}\n\n// New serves the users. @intent USR-1\nfunc New(s user.Store) http.Handler {\n\th := &handler{store: s}\n\th.list()\n\treturn nil\n}\n\nfunc (h *handler) list() {\n\tif h != nil && h.store != nil {\n\t\twriteJSON()\n\t}\n}\n\nfunc writeJSON() {}\n",
    ),
    (
        "internal/api/api_test.go",
        "package api_test\n\nimport (\n\t\"testing\"\n\n\t\"ig-test/internal/api\"\n)\n\nfunc TestList(t *testing.T) {\n\tapi.New(nil)\n}\n",
    ),
    (
        "internal/client/client.go",
        "package client\n\nimport \"ig-test/internal/user\"\n\ntype Client struct{ base string }\n\nfunc New(base string) *Client { return &Client{base} }\n\nfunc (c *Client) List() []user.User { return nil }\n",
    ),
    (
        "internal/store/memory.go",
        "package store\n\nimport \"ig-test/internal/user\"\n\ntype Memory struct{ users []user.User }\n\nfunc NewMemory() *Memory { return &Memory{} }\n",
    ),
    (
        "internal/store/postgres.go",
        "package store\n\nimport (\n\t\"context\"\n\t_ \"embed\"\n\n\t\"github.com/jackc/pgx/v5\"\n\t\"github.com/jackc/pgx/v5/pgxpool\"\n\n\t\"ig-test/internal/user\"\n)\n\ntype Postgres struct {\n\tpool *pgxpool.Pool\n}\n\nfunc NewPostgres(ctx context.Context, url string) (*Postgres, error) {\n\tpool, err := pgxpool.New(ctx, url)\n\treturn &Postgres{pool: pool}, err\n}\n\nfunc (p *Postgres) Close() { p.pool.Close() }\n\nfunc (p *Postgres) List() ([]user.User, error) {\n\treturn pgx.CollectRows(nil, nil)\n}\n",
    ),
    (
        "internal/user/user.go",
        "package user\n\ntype (\n\tUser struct{ ID int64 }\n\tStore interface {\n\t\tList() ([]User, error)\n\t}\n)\n",
    ),
    ("web/package.json", r#"{"name": "web"}"#),
    ("web/src/main.ts", "export const a = 1;\n"),
];

fn ws_of(files: &[(&str, &str)]) -> WorkspaceInfo {
    let mem = MemFiles::with(files);
    let packages = GoFrontend.found(&mem, &Config::default());
    let package_of = |path: &str| {
        let dir = dir_of(path);
        (packages.iter().enumerate())
            .filter(|(_, p)| under(dir, &p.dir).is_some())
            .max_by_key(|(_, p)| p.dir.len())
            .map_or(0, |(i, _)| i)
    };
    WorkspaceInfo {
        root: ".".into(),
        files: (files.iter())
            .filter(|(p, _)| p.ends_with(".go"))
            .map(|(p, _)| SourceFile {
                path: (*p).into(),
                package: package_of(p),
            })
            .collect(),
        packages,
    }
}

fn build(files: &[(&str, &str)]) -> FrontendOutput {
    let ws = ws_of(files);
    let input: Vec<(SourceFile, String)> = (ws.files.iter())
        .map(|f| {
            let src = files.iter().find(|(p, _)| *p == f.path).unwrap().1;
            (f.clone(), src.to_string())
        })
        .collect();
    GoFrontend.build(&input, &ws, &Config::default())
}

fn edges(out: &FrontendOutput, kind: &str) -> Vec<(String, String, String)> {
    let mut e: Vec<_> = (out.edges.iter())
        .filter(|e| e.kind == kind)
        .map(|e| (e.from.clone(), e.to.clone(), e.confidence.clone()))
        .collect();
    e.sort();
    e
}

fn pairs(v: Vec<Package>) -> Vec<(String, String)> {
    v.into_iter().map(|p| (p.dir, p.name)).collect()
}

fn s(a: &str, b: &str) -> (String, String) {
    (a.into(), b.into())
}

fn t(a: &str, b: &str, c: &str) -> (String, String, String) {
    (a.into(), b.into(), c.into())
}

#[test]
fn a_package_per_folder_named_by_its_import_path() {
    let f = MemFiles::with(IG_TEST);
    assert_eq!(
        pairs(GoFrontend.found(&f, &Config::default())),
        [
            s("cmd/api", "ig-test/cmd/api"),
            s("cmd/cli", "ig-test/cmd/cli"),
            s("cmd/mcp", "ig-test/cmd/mcp"),
            s("internal/api", "ig-test/internal/api"),
            s("internal/client", "ig-test/internal/client"),
            s("internal/store", "ig-test/internal/store"),
            s("internal/user", "ig-test/internal/user"),
        ]
    );
}

#[test]
fn packages_skip_tests_testdata_vendor_hidden_and_ignored_folders() {
    let f = MemFiles::with(&[
        ("go.mod", "module example.com/shop // the shop\n"),
        ("main.go", "package main\n"),
        ("only/x_test.go", "package only\n"),
        ("pkg/testdata/t.go", "package t\n"),
        ("vendor/github.com/x/y/y.go", "package y\n"),
        (".tools/t.go", "package t\n"),
        ("_old/o.go", "package o\n"),
        ("gen/api/a.go", "package api\n"),
        ("pkg/a.go", "package pkg\n"),
    ]);
    let config = Config {
        ignore: vec!["gen/**".into()],
        ..Config::default()
    };
    assert_eq!(
        pairs(GoFrontend.found(&f, &config)),
        [s(".", "example.com/shop"), s("pkg", "example.com/shop/pkg")]
    );
}

#[test]
fn nested_modules_and_go_work() {
    let f = MemFiles::with(&[
        (
            "go.work",
            "go 1.23\n\nuse (\n\t./api // the service\n\t./lib\n)\nuse ./tools\nuse ../elsewhere\n",
        ),
        ("api/go.mod", "module github.com/acme/api\n"),
        ("api/main.go", "package main\n"),
        ("api/handlers/h.go", "package handlers\n"),
        ("lib/go.mod", "module \"github.com/acme/lib\"\n"),
        ("lib/lib.go", "package lib\n"),
        ("tools/go.mod", "module github.com/acme/tools\n"),
        ("tools/gen/gen.go", "package main\n"),
        ("a/b/c/d/go.mod", "module deep\n"),
        ("a/b/c/d/e/x.go", "package e\n"),
    ]);
    assert_eq!(
        pairs(GoFrontend.found(&f, &Config::default())),
        [
            s("a/b/c/d/e", "deep/e"),
            s("api", "github.com/acme/api"),
            s("api/handlers", "github.com/acme/api/handlers"),
            s("lib", "github.com/acme/lib"),
            s("tools/gen", "github.com/acme/tools/gen"),
        ]
    );
    assert_eq!(
        work_uses("use (\n\t./api // x\n\t\"./lib\"\n\t../out\n)\nuse .\n"),
        ["api", "lib", "."]
    );
}

#[test]
fn without_a_module_folders_and_deep_modules() {
    let f = MemFiles::with(&[("main.go", "package tool\n"), ("lib/l.go", "package lib\n")]);
    assert_eq!(
        pairs(GoFrontend.found(&f, &Config::default())),
        [s(".", "tool"), s("lib", "lib")]
    );
    let deep = MemFiles::with(&[
        ("a/b/c/d/svc/go.mod", "module example.com/deep\n"),
        ("a/b/c/d/svc/main.go", "package main\n"),
        ("a/b/c/d/svc/lib/lib.go", "package lib\n"),
    ]);
    assert_eq!(
        pairs(GoFrontend.found(&deep, &Config::default())),
        [
            s("a/b/c/d/svc", "example.com/deep"),
            s("a/b/c/d/svc/lib", "example.com/deep/lib")
        ]
    );
}

#[test]
fn build_ignored_and_generated_files_are_left_out() {
    let files: &[(&str, &str)] = &[
        ("go.mod", "module example.com/g\n"),
        (
            "cmd/app/main.go",
            "package main\n\nimport \"example.com/g/lib\"\n\nfunc main() { lib.Do() }\n",
        ),
        (
            "lib/a_gen.go",
            "//go:build ignore\n\npackage main\n\nfunc main() {}\n",
        ),
        (
            "lib/b.pb.go",
            "// Code generated by protoc-gen-go. DO NOT EDIT.\n\npackage lib\n\nfunc Gen() {}\n",
        ),
        ("lib/lib.go", "package lib\n\nfunc Do() {}\n"),
        (
            "lib/old.go",
            "// +build ignore\n\npackage lib\n\nfunc Old() {}\n",
        ),
    ];
    let out = build(files);
    assert_eq!(
        edges(&out, "calls"),
        [t("cmd/app/main.go#main", "lib/lib.go#Do", "resolved")]
    );
    for f in ["lib/a_gen.go", "lib/b.pb.go", "lib/old.go"] {
        let facts = out.facts.iter().find(|x| x.file == f).unwrap();
        assert_eq!(facts.nodes.len(), 1, "{f}: its module only");
    }
    let root = MemFiles::with(&[
        ("a_gen.go", "//go:build ignore\n\npackage main\n"),
        ("b.go", "package tool\n"),
    ]);
    assert_eq!(
        pairs(GoFrontend.found(&root, &Config::default())),
        [s(".", "tool")]
    );
    assert!(!left_out(
        "// Code generated\npackage x\n//go:build ignore\n"
    ));
}

#[test]
fn nodes_by_kind_and_id() {
    let out = build(IG_TEST);
    let ids = |file: &str| -> Vec<(String, String, String)> {
        let f = out.facts.iter().find(|f| f.file == file).unwrap();
        (f.nodes.iter())
            .map(|n| t(&n.id, &n.kind, n.parent.as_deref().unwrap_or_default()))
            .collect()
    };
    assert_eq!(
        ids("internal/api/api.go"),
        [
            t("internal/api/api.go", "module", ""),
            t(
                "internal/api/api.go#handler",
                "class",
                "internal/api/api.go"
            ),
            t("internal/api/api.go#New", "function", "internal/api/api.go"),
            t(
                "internal/api/api.go#handler.list",
                "method",
                "internal/api/api.go#handler"
            ),
            t(
                "internal/api/api.go#writeJSON",
                "function",
                "internal/api/api.go"
            ),
        ]
    );
    assert_eq!(
        ids("internal/user/user.go")[1..],
        [
            t(
                "internal/user/user.go#User",
                "class",
                "internal/user/user.go"
            ),
            t(
                "internal/user/user.go#Store",
                "class",
                "internal/user/user.go"
            ),
        ]
    );
    let api = (out.facts.iter())
        .find(|f| f.file == "internal/api/api.go")
        .unwrap();
    let new = &api.nodes[2];
    assert_eq!(new.intents, ["USR-1"]);
    assert_eq!(new.package.as_deref(), Some("ig-test/internal/api"));
    assert!(new.exported && !api.nodes[1].exported && !api.nodes[3].exported);
    assert_eq!(api.nodes[3].metrics.complexity, 3, "an if and an &&");
    let test = (out.facts.iter())
        .find(|f| f.file == "cmd/mcp/main_test.go")
        .unwrap();
    let kinds: Vec<&str> = test.nodes.iter().map(|n| n.kind.as_str()).collect();
    assert_eq!(kinds, ["module", "test", "function"]);
}

#[test]
fn imports_point_at_the_files_they_use_and_externals_by_module() {
    let out = build(IG_TEST);
    let got: Vec<(String, String)> = (edges(&out, "imports").into_iter())
        .map(|(f, to, c)| {
            assert_eq!(c, "resolved");
            (f, to)
        })
        .collect();
    assert_eq!(
        got,
        [
            s("cmd/api/main.go", "internal/api/api.go"),
            s("cmd/api/main.go", "internal/store/postgres.go"),
            s("cmd/api/main.go", "internal/user/user.go"),
            s("cmd/cli/main.go", "internal/client/client.go"),
            s("cmd/mcp/main.go", "internal/client/client.go"),
            s("internal/api/api.go", "internal/user/user.go"),
            s("internal/api/api_test.go", "internal/api/api.go"),
            s("internal/client/client.go", "internal/user/user.go"),
            s("internal/store/memory.go", "internal/user/user.go"),
            s("internal/store/postgres.go", "internal/user/user.go"),
        ]
    );
    let mut pkgs: Vec<(String, String)> = (out.packages.iter()).map(|(f, p, _)| s(f, p)).collect();
    pkgs.sort();
    pkgs.dedup();
    assert_eq!(
        pkgs,
        [
            s("cmd/mcp/main.go", "github.com/modelcontextprotocol/go-sdk"),
            s("internal/store/postgres.go", "github.com/jackc/pgx/v5"),
        ]
    );
}

#[test]
fn calls_through_imports_receivers_and_names() {
    let out = build(IG_TEST);
    let calls = edges(&out, "calls");
    let has = |from: &str, to: &str, conf: &str| calls.contains(&t(from, to, conf));
    let main = "cmd/api/main.go#main";
    assert!(
        has(main, "internal/store/postgres.go#NewPostgres", "resolved"),
        "{calls:#?}"
    );
    assert!(has(main, "internal/api/api.go#New", "resolved"));
    assert!(
        has(main, "internal/store/postgres.go#Postgres.Close", "guessed"),
        "a method on a local"
    );
    let cli = "cmd/cli/main.go#main";
    assert!(
        has(cli, "internal/client/client.go#New", "resolved"),
        "through an alias"
    );
    assert!(has(cli, "internal/client/client.go#Client.List", "guessed"));
    assert!(has(
        "internal/api/api.go#New",
        "internal/api/api.go#handler.list",
        "guessed"
    ));
    assert!(
        has(
            "internal/api/api.go#handler.list",
            "internal/api/api.go#writeJSON",
            "resolved"
        ),
        "same package"
    );
    assert!(has(
        "cmd/mcp/main_test.go#TestTools",
        "cmd/mcp/main.go#main",
        "resolved"
    ));
    assert!(
        !calls.iter().any(|(_, to, _)| to.ends_with("Postgres.List")),
        "pgx.CollectRows is not a method of ours"
    );
    let in_loop = (out.edges.iter())
        .find(|e| e.from == cli && e.to == "internal/client/client.go#New")
        .unwrap();
    assert!(in_loop.in_loop);
}

#[test]
fn dot_blank_and_member_calls() {
    let out = build(&[
        ("go.mod", "module github.com/acme/shop\n"),
        (
            "main.go",
            "package main\n\nimport (\n\t. \"github.com/acme/shop/price\"\n\t_ \"github.com/acme/shop/plugins\"\n\t\"github.com/acme/shop/v2x\"\n)\n\ntype Cart struct{}\n\nfunc (c Cart) Sum() int { return c.add(Total()) }\n\nfunc (c Cart) add(n int) int { return n }\n\nfunc main() { _ = v2x.Run }\n",
        ),
        (
            "price/price.go",
            "package price\n\nfunc Total() int { return 0 }\n",
        ),
        ("plugins/a.go", "package plugins\n"),
        ("plugins/b.go", "package plugins\n"),
    ]);
    let imports: Vec<(String, String)> = (edges(&out, "imports").into_iter())
        .map(|(f, to, _)| (f, to))
        .collect();
    assert_eq!(
        imports,
        [
            s("main.go", "plugins/a.go"),
            s("main.go", "plugins/b.go"),
            s("main.go", "price/price.go"),
        ]
    );
    assert!(
        out.packages.is_empty(),
        "an unread folder of our module is not a package"
    );
    assert_eq!(
        edges(&out, "calls"),
        [
            t("main.go#Cart.Sum", "main.go#Cart.add", "resolved"),
            t("main.go#Cart.Sum", "price/price.go#Total", "resolved"),
        ]
    );
}

#[test]
fn comments_and_strings_hold_no_imports_and_parse_errors_are_kept() {
    let out = build(&[
        ("go.mod", "module m\n"),
        (
            "a.go",
            "package a\n\n// import \"m/b\"\nvar s = `import \"m/b\"`\n\n/* func Ghost() {} */\nfunc Real() {}\n",
        ),
        ("b/b.go", "package b\n\nfunc F( {\n"),
    ]);
    assert!(edges(&out, "imports").is_empty());
    let a = out.facts.iter().find(|f| f.file == "a.go").unwrap();
    let names: Vec<&str> = a.nodes.iter().map(|n| n.name.as_str()).collect();
    assert_eq!(names, ["a.go", "Real"]);
    let b = out.facts.iter().find(|f| f.file == "b/b.go").unwrap();
    assert!((b.parse_error.as_deref()).is_some_and(|e| e.starts_with("syntax error")));
}

#[test]
fn third_party_modules_and_default_names() {
    for (path, module) in [
        ("github.com/jackc/pgx/v5/pgxpool", "github.com/jackc/pgx/v5"),
        ("github.com/lib/pq", "github.com/lib/pq"),
        ("golang.org/x/sync/errgroup", "golang.org/x/sync"),
        ("google.golang.org/grpc/codes", "google.golang.org/grpc"),
        ("gopkg.in/yaml.v3", "gopkg.in/yaml.v3"),
        (
            "go.mongodb.org/mongo-driver/v2/mongo",
            "go.mongodb.org/mongo-driver/v2",
        ),
        (
            "github.com/aws/aws-sdk-go-v2/service/s3/types",
            "github.com/aws/aws-sdk-go-v2/service/s3",
        ),
    ] {
        assert_eq!(third_party_module(path), module);
    }
    assert_eq!(default_name("github.com/jackc/pgx/v5"), "pgx");
    assert_eq!(default_name("github.com/mattn/go-sqlite3"), "sqlite3");
    assert_eq!(default_name("gopkg.in/yaml.v3"), "yaml");
}
