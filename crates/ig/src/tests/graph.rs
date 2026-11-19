//! The graph build over the real tree with the TS frontend.

use super::{build, load, tmp_with};
use ig_core::config::Config;

/// Each package resolves with its own tsconfig (a shared resolver cache once made every package
/// use whichever tsconfig loaded first).
#[test]
fn per_package_tsconfig_paths() {
    let dir = std::env::temp_dir().join(format!("ig-paths-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let w = |p: &str, s: &str| {
        let f = dir.join(p);
        std::fs::create_dir_all(f.parent().unwrap()).unwrap();
        std::fs::write(f, s).unwrap();
    };
    w("pnpm-workspace.yaml", "packages: ['pkgs/*']\n");
    for (p, alias) in [("a", "@x"), ("b", "@y")] {
        w(
            &format!("pkgs/{p}/package.json"),
            &format!(r#"{{"name":"{p}"}}"#),
        );
        w(
            &format!("pkgs/{p}/tsconfig.json"),
            &format!(
                r#"{{"extends":"./.nuxt/missing.json","compilerOptions":{{"paths":{{"{alias}/*":["./src/*"]}}}}}}"#
            ),
        );
        w(&format!("pkgs/{p}/src/util.ts"), "export const u = 1;\n");
        w(
            &format!("pkgs/{p}/src/main.ts"),
            &format!("import {{ u }} from '{alias}/util';\n"),
        );
    }
    let g = build(&dir, &Config::default(), None).unwrap().graph;
    for p in ["a", "b"] {
        let from = format!("pkgs/{p}/src/main.ts");
        assert!(
            g.edges
                .iter()
                .any(|e| e.from == from && e.to == format!("pkgs/{p}/src/util.ts")),
            "{p} alias unresolved"
        );
    }
    assert_eq!(g.parse_errors.len(), 2, "{:?}", g.parse_errors);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn typed_calls_through_injection_and_interfaces() {
    let dir = std::env::temp_dir().join(format!("ig-typed-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let w = |p: &str, s: &str| {
        let f = dir.join(p);
        std::fs::create_dir_all(f.parent().unwrap()).unwrap();
        std::fs::write(f, s).unwrap();
    };
    w("package.json", r#"{"name":"app"}"#);
    w(
        "src/domain/repo.ts",
        "export interface Repo { find(id: string): string }\n",
    );
    w(
        "src/infra/pg-repo.ts",
        "import type { Repo } from '../domain/repo';\nexport class PgRepo implements Repo {\n  find(id: string) { return id; }\n}\n",
    );
    w(
        "src/use-cases/get.ts",
        "import type { Repo } from '../domain/repo';\nexport class GetUseCase {\n  constructor(private readonly repo: Repo) {}\n  execute(id: string) { return this.repo.find(id); }\n}\n",
    );
    w(
        "src/routes.ts",
        "import { GetUseCase } from './use-cases/get';\nimport { PgRepo } from './infra/pg-repo';\nexport function routes() {\n  const useCase = new GetUseCase(new PgRepo());\n  return [{ method: 'GET', path: '/x', handler: useCase.execute.bind(useCase) }];\n}\n",
    );
    let g = build(&dir, &Config::default(), None).unwrap().graph;
    let edge = |from: &str, to: &str| {
        g.edges
            .iter()
            .find(|e| e.from == from && e.to == to)
            .map(|e| e.confidence.clone())
    };
    let dispatch = g.edges.iter().find(|e| {
        e.from == "src/use-cases/get.ts#GetUseCase.execute"
            && e.to == "src/infra/pg-repo.ts#PgRepo.find"
    });
    assert_eq!(
        dispatch.map(|e| (e.kind.as_str(), e.confidence.as_str())),
        Some(("dispatch", "resolved")),
        "{:#?}",
        g.edges
    );
    assert_eq!(
        edge(
            "src/routes.ts#routes",
            "src/use-cases/get.ts#GetUseCase.execute"
        )
        .as_deref(),
        Some("resolved")
    );
    // Interfaces are not nodes, and edges to them are dropped.
    assert!(!g.nodes.iter().any(|n| n.id == "src/domain/repo.ts#Repo"));
    let _ = std::fs::remove_dir_all(&dir);
}

// `this.db.knex(...)` where `knex` is a property of the injected class reaches the class.
#[test]
fn typed_field_property_reaches_class() {
    let dir = tmp_with(&[
        ("db.ts", "export class Db { knex = 1 }\n"),
        (
            "repo.ts",
            "import { Db } from './db';\nexport class Repo { constructor(private db: Db) {} find() { return this.db.knex('t') } }\n",
        ),
    ]);
    let g = build(&dir, &Config::default(), None).unwrap().graph;
    let e = g
        .edges
        .iter()
        .find(|e| e.from == "repo.ts#Repo.find" && e.to == "db.ts#Db");
    assert_eq!(
        e.map(|e| (e.kind.as_str(), e.confidence.as_str())),
        Some(("calls", "resolved")),
        "{:#?}",
        g.edges
    );
    let _ = std::fs::remove_dir_all(&dir);
}

// A base-class receiver reaches a method only a subclass has (dispatch); a method of the
// class itself is a call to it; a class from node_modules gives no edge.
#[test]
fn typed_base_class_and_external_receivers() {
    let dir = tmp_with(&[
        (
            "node_modules/ext/package.json",
            r#"{"name":"ext","types":"index.d.ts"}"#,
        ),
        (
            "node_modules/ext/index.d.ts",
            "export declare class Ext { go(): void }\n",
        ),
        (
            "animal.ts",
            "export class Animal { eat() {} }\nexport class Dog extends Animal { bark() {} }\n",
        ),
        (
            "pet.ts",
            "import { Animal } from './animal';\nimport { Ext } from 'ext';\nexport function pet(a: Animal) { a.bark(); }\nexport function feed(a: Animal) { a.eat(); }\nexport function ext(x: Ext) { x.go(); }\n",
        ),
    ]);
    let g = build(&dir, &Config::default(), None).unwrap().graph;
    let kind = |from: &str, to: &str| {
        g.edges
            .iter()
            .find(|e| e.from == from && e.to == to)
            .map(|e| (e.kind.clone(), e.confidence.clone()))
    };
    assert_eq!(
        kind("pet.ts#pet", "animal.ts#Dog.bark"),
        Some(("dispatch".into(), "resolved".into())),
        "{:#?}",
        g.edges
    );
    assert_eq!(kind("pet.ts#pet", "animal.ts#Animal"), None);
    assert_eq!(
        kind("pet.ts#feed", "animal.ts#Animal.eat"),
        Some(("calls".into(), "resolved".into()))
    );
    assert!(
        !g.edges.iter().any(|e| e.from == "pet.ts#ext"),
        "{:#?}",
        g.edges
    );
    let _ = std::fs::remove_dir_all(&dir);
}

// `nock(h).post()` or `axios.post()` on a package import never guesses `post` by name; a chain
// from a local import still does.
#[test]
fn package_receivers_never_guess() {
    let dir = tmp_with(&[
        (
            "gateway.ts",
            "export class Gateway { post() {} put() {} }\n",
        ),
        ("make.ts", "export const make = () => ({});\n"),
        (
            "sdk.spec.ts",
            "import nock from 'nock';\nimport axios from 'axios';\nimport { make } from './make';\nexport function t() { nock('h').post('/x'); axios.post('/x'); make().put(); }\n",
        ),
    ]);
    let g = build(&dir, &Config::default(), None).unwrap().graph;
    let to = |n: &str| {
        g.edges
            .iter()
            .any(|e| e.from == "sdk.spec.ts#t" && e.to == n)
    };
    assert!(!to("gateway.ts#Gateway.post"), "{:#?}", g.edges);
    assert!(to("gateway.ts#Gateway.put"), "{:#?}", g.edges);
    let _ = std::fs::remove_dir_all(&dir);
}

// A page calling an auto-imported composable resolves.
#[test]
fn auto_imported_calls_resolve() {
    let dir = tmp_with(&[
        (
            "ig.yaml",
            "embedded: ['.vue']\nautoImports: ['.nuxt/imports.d.ts']\n",
        ),
        (
            ".nuxt/imports.d.ts",
            "export { useCatalog } from '../src/composables/catalog';\n",
        ),
        (
            "src/composables/catalog.ts",
            "export const useCatalog = () => 1;\n",
        ),
        (
            "src/pages/index.vue",
            "<script setup lang=\"ts\">\nconst c = useCatalog();\n</script>\n",
        ),
    ]);
    let g = build(&dir, &load(&dir).unwrap(), None).unwrap().graph;
    assert!(
        g.edges
            .iter()
            .any(|e| e.from.starts_with("src/pages/index.vue")
                && e.to == "src/composables/catalog.ts#useCatalog"
                && e.kind == "calls"
                && e.confidence == "resolved"),
        "{:#?}",
        g.edges
    );
}

// Only a free identifier is an auto-import: not a member, not a parameter; a package-sourced
// name stays unresolved.
#[test]
fn only_free_identifiers_are_auto_imports() {
    let dir = tmp_with(&[
        (
            "ig.yaml",
            "embedded: ['.vue']\nautoImports: ['.nuxt/imports.d.ts']\n",
        ),
        (
            ".nuxt/imports.d.ts",
            "export { useCatalog } from '../src/composables/catalog';\nexport { useI18n } from 'vue-i18n';\n",
        ),
        (
            "src/composables/catalog.ts",
            "export const useCatalog = () => 1;\n",
        ),
        (
            "src/a.ts",
            "export function member(obj: any) { obj.useCatalog(); }\nexport function param(useCatalog: any) { useCatalog(); }\nexport function bare() { useCatalog(); useI18n(); }\n",
        ),
    ]);
    let g = build(&dir, &load(&dir).unwrap(), None).unwrap().graph;
    let to_catalog = |from: &str| {
        g.edges.iter().any(|e| {
            e.from == from
                && e.to == "src/composables/catalog.ts#useCatalog"
                && e.confidence == "resolved"
        })
    };
    assert!(to_catalog("src/a.ts#bare"), "{:#?}", g.edges);
    assert!(!to_catalog("src/a.ts#member"), "{:#?}", g.edges);
    assert!(!to_catalog("src/a.ts#param"), "{:#?}", g.edges);
    assert!(
        !g.edges.iter().any(|e| e.from == "src/a.ts#bare"
            && e.resolved()
            && e.to != "src/composables/catalog.ts#useCatalog"),
        "{:#?}",
        g.edges
    );
}

// An auto-imported component tag renders the component; an unknown tag adds nothing.
#[test]
fn auto_imported_components_render() {
    let dir = tmp_with(&[
        (
            "ig.yaml",
            "embedded: ['.vue']\nautoImports: ['.nuxt/components.d.ts']\n",
        ),
        (
            ".nuxt/components.d.ts",
            "export const DashboardReport: typeof import(\"../src/components/DashboardReport.vue\")['default']\n",
        ),
        (
            "src/components/DashboardReport.vue",
            "<template><div/></template>\n",
        ),
        (
            "src/pages/index.vue",
            "<template><dashboard-report /><other-thing /></template>\n",
        ),
    ]);
    let g = build(&dir, &load(&dir).unwrap(), None).unwrap().graph;
    let from_page: Vec<_> = g
        .edges
        .iter()
        .filter(|e| e.from == "src/pages/index.vue")
        .collect();
    assert!(
        from_page
            .iter()
            .any(|e| e.to == "src/components/DashboardReport.vue"
                && e.kind == "renders"
                && e.confidence == "resolved"),
        "{from_page:#?}"
    );
    assert_eq!(from_page.len(), 1, "{from_page:#?}");
}

#[test]
fn code_never_resolves_into_tests() {
    let dir = tmp_with(&[
        ("ig.yaml", "tests: ['**/test/**']\n"),
        (
            "src/a.ts",
            "import { useI18n } from '../test/stub';\nexport function a() { useI18n(); }\n",
        ),
        ("src/b.ts", "export function b() { return 1; }\n"),
        ("test/stub.ts", "export const useI18n = () => 1;\n"),
        (
            "test/a.test.ts",
            "import { b } from '../src/b';\nexport function t() { b(); }\n",
        ),
    ]);
    let g = build(&dir, &load(&dir).unwrap(), None).unwrap().graph;
    assert!(
        !g.edges.iter().any(|e| e.from.starts_with("src/")
            && e.to.starts_with("test/")
            && e.kind != "imports"),
        "{:#?}",
        g.edges
    );
    // The import itself stays: code importing a test file is a smell arch rules can flag.
    assert!(
        g.edges
            .iter()
            .any(|e| e.from == "src/a.ts" && e.to == "test/stub.ts" && e.kind == "imports"),
        "{:#?}",
        g.edges
    );
    assert!(
        g.edges
            .iter()
            .any(|e| e.from.starts_with("test/a.test.ts") && e.to.starts_with("src/b.ts")),
        "{:#?}",
        g.edges
    );
}

// A name missing from a stale declaration file resolves from the directories it covers.
#[test]
fn stale_declarations_resolve_from_covered_dirs() {
    let dir = tmp_with(&[
        (
            "ig.yaml",
            "embedded: ['.vue']\nautoImports: ['.nuxt/imports.d.ts']\n",
        ),
        (
            ".nuxt/imports.d.ts",
            "export { useA } from '../src/composables/a';\n",
        ),
        ("src/composables/a.ts", "export const useA = () => 1;\n"),
        ("src/composables/b.ts", "export const useB = () => 2;\n"),
        (
            "src/pages/index.vue",
            "<script setup lang=\"ts\">\nconst b = useB();\n</script>\n",
        ),
    ]);
    let g = build(&dir, &load(&dir).unwrap(), None).unwrap().graph;
    assert!(
        g.edges
            .iter()
            .any(|e| e.from.starts_with("src/pages/index.vue")
                && e.to == "src/composables/b.ts#useB"
                && e.confidence == "resolved"),
        "{:#?}",
        g.edges
    );
    let notices: Vec<_> = g
        .parse_errors
        .iter()
        .filter(|e| e.message.contains("resolved from its directories"))
        .collect();
    assert_eq!(notices.len(), 1, "{:#?}", g.parse_errors);
    assert_eq!(
        notices[0].message,
        "autoImports: 1 names not in .nuxt/imports.d.ts resolved from its directories (run nuxt prepare): useB"
    );
}

// A package-sourced auto-import never falls back to a by-name guess.
#[test]
fn package_names_never_guess() {
    let dir = tmp_with(&[
        (
            "ig.yaml",
            "embedded: ['.vue']\nautoImports: ['.nuxt/imports.d.ts']\n",
        ),
        (
            ".nuxt/imports.d.ts",
            "export { useI18n } from 'vue-i18n';\n",
        ),
        ("test/stub.ts", "export const useI18n = () => 1;\n"),
        (
            "src/pages/index.vue",
            "<script setup lang=\"ts\">\nconst t = useI18n();\n</script>\n",
        ),
    ]);
    let g = build(&dir, &load(&dir).unwrap(), None).unwrap().graph;
    assert!(
        !g.edges
            .iter()
            .any(|e| e.from.starts_with("src/pages/") && e.to.starts_with("test/")),
        "{:#?}",
        g.edges
    );
}

// A name declared from an unresolvable path (Nuxt's `../node_modules/...` module composables)
// is external too: never guessed.
#[test]
fn node_modules_names_never_guess() {
    let dir = tmp_with(&[
        (
            "ig.yaml",
            "embedded: ['.vue']\nautoImports: ['.nuxt/imports.d.ts']\n",
        ),
        (
            ".nuxt/imports.d.ts",
            "export { useX } from '../node_modules/pkg/x';\n",
        ),
        ("test/stub.ts", "export const useX = () => 1;\n"),
        (
            "src/pages/index.vue",
            "<script setup lang=\"ts\">\nconst x = useX();\n</script>\n",
        ),
    ]);
    let g = build(&dir, &load(&dir).unwrap(), None).unwrap().graph;
    assert!(
        !g.edges.iter().any(|e| e.from.starts_with("src/pages/")),
        "{:#?}",
        g.edges
    );
}

// Two same-named exports across covered dirs: no stale resolve. Test files calling a stale
// name don't count towards the notice.
#[test]
fn stale_ambiguous_and_test_callers() {
    let dir = tmp_with(&[
        (
            "ig.yaml",
            "embedded: ['.vue']\nautoImports: ['.nuxt/imports.d.ts']\ntests: ['**/*.spec.ts']\n",
        ),
        (
            ".nuxt/imports.d.ts",
            "export { useA } from '../src/a/a';\nexport { useC } from '../src/c/c';\n",
        ),
        ("src/a/a.ts", "export const useA = () => 1;\n"),
        (
            "src/a/b.ts",
            "export const useB = () => 2;\nexport const useD = () => 4;\n",
        ),
        ("src/c/c.ts", "export const useC = () => 3;\n"),
        ("src/c/b.ts", "export const useB = () => 2;\n"),
        (
            "src/pages/index.vue",
            "<script setup lang=\"ts\">\nconst b = useB();\n</script>\n",
        ),
        ("src/x.spec.ts", "export function t() { useD(); }\n"),
    ]);
    let g = build(&dir, &load(&dir).unwrap(), None).unwrap().graph;
    assert!(
        !g.edges
            .iter()
            .any(|e| e.from.starts_with("src/pages/") && e.resolved()),
        "{:#?}",
        g.edges
    );
    assert!(
        !g.parse_errors
            .iter()
            .any(|e| e.message.contains("resolved from its directories")),
        "{:#?}",
        g.parse_errors
    );
}

// `app.get(BASE_PATH, h)` with `const BASE_PATH = '/p'` is the route `GET /p`.
#[test]
fn bare_constant_route_path() {
    let dir = tmp_with(&[
        (
            "ig.yaml",
            "patterns:\n  - kind: route\n    match: \"app.get($PATH, $$$REST)\"\n    id: \"GET {PATH}\"\nblueprint:\n  http: { calls: [fetch] }\n",
        ),
        (
            "src/server.ts",
            "const BASE_PATH = '/p';\nexport function h() {}\napp.get(BASE_PATH, h);\n",
        ),
        (
            "src/page.ts",
            "const q = (u: string, x?: number) => (x ? `${u}?x=${x}` : u);\nexport function load() { return fetch(`/p`); }\nexport function wrapped() { return fetch(q('/p', 1)); }\nexport function joined(id: string) { return fetch(join('/p', 'users', id)); }\nexport function joinedT(id: string) { return fetch(join('/p', `${id}`)); }\n",
        ),
    ]);
    let g = build(&dir, &load(&dir).unwrap(), None).unwrap().graph;
    let route = g
        .nodes
        .iter()
        .find(|n| n.kind == "route")
        .expect("route node");
    assert_eq!(route.name, "GET /p");
    assert!(
        g.edges
            .iter()
            .any(|e| e.from == "src/page.ts#load" && e.kind == "http" && e.to == route.id),
        "{:#?}",
        g.edges
    );
    // A URL passed through a local helper (`withQuery('/p', id)`) is still that URL.
    assert!(
        g.edges
            .iter()
            .any(|e| e.from == "src/page.ts#wrapped" && e.kind == "http" && e.to == route.id),
        "{:#?}",
        g.edges
    );
    // A helper given more string segments (`join('/p', 'users', id)`) builds another URL.
    assert!(
        !g.edges.iter().any(|e| (e.from == "src/page.ts#joined"
            || e.from == "src/page.ts#joinedT")
            && e.kind == "http"
            && e.to == route.id),
        "{:#?}",
        g.edges
    );
}

#[test]
fn blueprint_links_http_and_systems() {
    let dir = std::env::temp_dir().join(format!("ig-blueprint-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let w = |p: &str, s: &str| {
        let f = dir.join(p);
        std::fs::create_dir_all(f.parent().unwrap()).unwrap();
        std::fs::write(f, s).unwrap();
    };
    w("package.json", r#"{"name":"app"}"#);
    w(
        "src/client.ts",
        "export class Notes {\n  constructor(private readonly fetch: any) {}\n  async add(line: number, batch: string) {\n    return this.fetch(`/api/lines/${line}/batches/${batch}/notes`, { method: 'POST' });\n  }\n  async list(line: number) {\n    let url = `/api/lines/${line}/batches`;\n    url += '?x=1';\n    return this.fetch(url, {});\n  }\n}\n",
    );
    w(
        "src/routes.ts",
        "import knex from 'knex';\nexport const routes = [\n  { method: 'POST', path: '/api/lines/{line}/batches/{batch}/notes', handler: () => knex('notes') },\n  { method: 'GET', path: '/api/lines/{line}/batches', handler: () => 1 },\n];\n",
    );
    let cfg = Config::parse(
        "patterns:\n  - kind: route\n    match: \"{ method: $METHOD, path: $PATH, $$$REST }\"\n    id: \"{METHOD} {PATH}\"\nblueprint:\n  http: { calls: [fetch] }\n  systems:\n    PostgreSQL: [knex, pg]\n",
    )
    .unwrap();
    let g = build(&dir, &cfg, None).unwrap().graph;
    let has = |from: &str, kind: &str, to: &str| {
        g.edges
            .iter()
            .any(|e| e.from == from && e.kind == kind && e.to == to)
    };
    assert!(
        has(
            "src/client.ts#Notes.add",
            "http",
            "src/routes.ts#POST /api/lines/{line}/batches/{batch}/notes"
        ),
        "{:#?}",
        g.edges
    );
    assert!(has(
        "src/client.ts#Notes.list",
        "http",
        "src/routes.ts#GET /api/lines/{line}/batches"
    ));
    assert!(has("src/routes.ts", "uses", "system:PostgreSQL"));
    assert!(
        g.nodes
            .iter()
            .any(|n| n.id == "system:PostgreSQL" && n.kind == "system")
    );
    let _ = std::fs::remove_dir_all(&dir);
}
