# Call Chain Implementation Plan (Design workspace, plan 1 of 3)

**Goal:** From a page or a function, show what it calls all the way down to the database, as a sequence with one lifeline per lane or layer, replacing today's sequence diagram (which only shows a file calling itself).

**Architecture:**
- **Rust (graph resolution).** Today's diagram shows almost nothing for a Nuxt page, for two reasons:
  - auto-imported composables and components (no `import` statement) are left as `guessed` edges, which the sequence drops;
  - a test stub can be the resolution target for real code.
  
  Two changes fix that: an `autoImports` config, which reads framework declaration files such as Nuxt's `.nuxt/imports.d.ts` and `components.d.ts`, and a rule that non-test code never resolves into test files.
- **TypeScript (viewer).** A new pure `callChain` model replaces `sequence`:
  - it folds calls inside the same file into their caller and follows calls across modules;
  - it crosses `http` edges to routes, then goes down to `uses` edges into systems;
  - it groups participants by lane (blueprint lanes, else layers).
  
  The side panel and overlay show it as a Mermaid sequence plus a numbered, clickable step list.

**Tech Stack:** Rust 2024 (existing crates), TypeScript + Lit viewer in `viewer/` (bun, Vite, Vitest).

**Spec:** `docs/specs/2026-10-25-ig-design-workspace-design.md` (section "Call chain view"). Plans 2 (the Tauri app with ACP chat) and 3 (the Design canvases) follow.

## Global Constraints

- No new Rust crates. Viewer npm additions are not needed for this plan.
- Existing behaviour is unchanged when `autoImports` is absent, apart from the test-target rule, which applies always. Golden tests may change only where the test-target rule removes an edge; review that diff.
- Bump `GRAPH_REVISION` in `src/git.rs` (currently 10 → 11), because extraction and resolution output changes.
- Base-graph worktrees must see the `autoImports` files even when they are gitignored (`.nuxt/` usually is): link them the same way `link_missing_extends` links missing tsconfig `extends` targets.
- Node ids: module `file`, member `file#Name`, route `file#METHOD /path`, system `system:<Name>`.
- `cargo test` and `make test` pass. Clippy adds no new errors; there are 7 known pre-existing errors. `bun run typecheck` is clean.
- Commits: plain messages, no AI attribution, explicit paths, never push. Never stage the screenshot files at the repo root.

## Review Focus

1. **Stale declarations.** An `autoImports` file that is missing or unreadable, for example `.nuxt` before `nuxt prepare`, gives one parse-error notice and graph building continues. *(Task 1)*
2. **A name auto-imported from a package** (`useI18n` from `vue-i18n`) resolves to nothing in the repo. The call stays a guess and is never pointed at an unrelated repo file of the same name. *(Task 1)*
3. **Recursion and cycles.** A → B → A, or a composable calling itself, must terminate. Each node is walked once and the chain is capped. *(Task 3)*
4. **Fan-out.** Pages that call 30+ things stay readable: a cap of 60 messages, plus a "+N more" marker in the step list. *(Task 3)*
5. **A start with no outgoing calls** shows "No calls from here" instead of an empty diagram. *(Task 4)*

---

## File Structure

- `src/config.rs`: the new `autoImports: Vec<String>` key (globs or paths).
- `src/autoimports.rs` (new): parses declaration files into `name → (file, export name)`.
- `src/graph.rs`: resolves `Callee::Guess` through the auto-import map; applies the test-target rule.
- `src/git.rs`: links `autoImports` files into base worktrees; bumps `GRAPH_REVISION`.
- `assets/skills/setup.md`: tells the agent to set `autoImports` for Nuxt and similar frameworks.
- `viewer/src/model/callchain.ts` (new): `callChain(ix, start, opts)`, plus Mermaid and PlantUML output.
- `viewer/src/model/sequence.ts`: deleted, once its callers move to `callchain.ts`.
- `viewer/src/components/ig-side-panel.ts`, `ig-overlay.ts`: the Call chain section and the overlay with its step list.

---

### Task 1: `autoImports`: resolve framework auto-imports

**Files:**
- Create: `src/autoimports.rs`
- Modify: `src/config.rs`, `src/graph.rs` (the `Callee::Guess` arm, about line 684), `src/git.rs`, `src/main.rs` (`mod autoimports;`), `assets/skills/setup.md`, `README.md`, `docs/specs/design.md`
- Test: `src/autoimports.rs` tests, `src/graph.rs` tests

**Interfaces:**
- Produces:
  - `pub struct AutoImports { names: HashMap<String, (String /*repo-relative file*/, String /*export name*/)> }`
  - `pub fn load(root: &Path, patterns: &[String], ws: &Workspace) -> (AutoImports, Vec<ParseError>)`
  - `pub fn get(&self, name: &str) -> Option<(&str, &str)>`
- Config:
  - `autoImports: [".nuxt/imports.d.ts", ".nuxt/components.d.ts"]`, with paths relative to the root;
  - globs are allowed (for example `"**/.nuxt/{imports,components}.d.ts"` in a monorepo);
  - default empty.
  
  `graph_key_input` must include it.
- **Parsing** (line-based regex; no TypeScript parse needed). Three forms:
  1. `export { a, b as c } from '<spec>'` maps `a → (spec, a)` and `c → (spec, b)`.
  2. `const a: typeof import('<spec>').a` and `typeof import('<spec>')['a']` map `a → (spec, a)`.
  3. In `components.d.ts`: `'Name': typeof import("<spec>")['default']` and `Name: typeof import(...)['default']` map `Name → (spec, default)`.
  
  A `<spec>` is resolved relative to the declaration file's directory, with the same extension probing the resolver uses (`.ts`, `.tsx`, `.vue`, `.js`, `/index.*`). A spec that resolves outside the workspace, such as `node_modules` or a bare package name, produces no entry (Review Focus 2).
- **Resolution:** in `graph.rs`, a `Callee::Guess(name)` (and a renders guess for a component tag) with `get(name) == Some((file, export))` resolves to the node exported as `export` from `file`, through the same export table used for named imports (`default` → the default export). The confidence is `resolved`. Otherwise today's guess behaviour stays.

- [ ] **Step 1: Write the failing tests**

```rust
// src/autoimports.rs
#[test]
fn parses_nuxt_forms() {
    let dir = tmp_with(&[
        (".nuxt/imports.d.ts", "export { useCatalog, useA as useB } from '../src/composables/catalog';\nexport { useI18n } from 'vue-i18n';\n"),
        (".nuxt/types/imports.d.ts", "  const usePage: typeof import('../../src/composables/page')['usePage']\n"),
        (".nuxt/components.d.ts", "  'DashboardReport': typeof import(\"../src/components/DashboardReport.vue\")['default']\n"),
        ("src/composables/catalog.ts", "export const useCatalog = () => 1; export const useA = () => 2;\n"),
        ("src/composables/page.ts", "export function usePage() {}\n"),
        ("src/components/DashboardReport.vue", "<script setup lang=\"ts\"></script>\n"),
    ]);
    let ws = workspace_of(&dir);
    let (ai, errs) = load(&dir, &[".nuxt/**/*.d.ts".into()], &ws);
    assert!(errs.is_empty());
    assert_eq!(ai.get("useCatalog"), Some(("src/composables/catalog.ts", "useCatalog")));
    assert_eq!(ai.get("useB"), Some(("src/composables/catalog.ts", "useA")));
    assert_eq!(ai.get("usePage"), Some(("src/composables/page.ts", "usePage")));
    assert_eq!(ai.get("DashboardReport"), Some(("src/components/DashboardReport.vue", "default")));
    assert_eq!(ai.get("useI18n"), None);                       // Review Focus 2
}

#[test]
fn missing_declaration_file_is_a_notice() {                     // Review Focus 1
    let dir = tmp_with(&[("src/a.ts", "export const a = 1;\n")]);
    let (ai, errs) = load(&dir, &[".nuxt/imports.d.ts".into()], &workspace_of(&dir));
    assert!(ai.get("a").is_none());
    assert_eq!(errs.len(), 1);
}
```

```rust
// src/graph.rs — a page calling an auto-imported composable resolves
#[test]
fn auto_imported_calls_resolve() {
    let dir = tmp_with(&[
        ("ig.yaml", "embedded: ['.vue']\nautoImports: ['.nuxt/imports.d.ts']\n"),
        (".nuxt/imports.d.ts", "export { useCatalog } from '../src/composables/catalog';\n"),
        ("src/composables/catalog.ts", "export const useCatalog = () => 1;\n"),
        ("src/pages/index.vue", "<script setup lang=\"ts\">\nconst c = useCatalog();\n</script>\n"),
    ]);
    let g = build(&dir, &Config::load(&dir).unwrap(), None).unwrap().graph;
    assert!(g.edges.iter().any(|e| e.from.starts_with("src/pages/index.vue") && e.to == "src/composables/catalog.ts#useCatalog" && e.kind == "calls" && e.confidence == "resolved"));
}
```

Use the existing test helpers in `graph.rs` for temporary repos, or add `tmp_with` and `workspace_of` as `#[cfg(test)]` helpers.

- [ ] **Step 2: Run** `cargo test autoimports auto_imported_calls_resolve`. Expected: FAIL (module missing).

- [ ] **Step 3: Implement.**
  - `src/autoimports.rs` and the config key.
  - Load once per build in `graph::build`, adding any errors to `parse_errors`.
  - The resolution in the `Guess` arm.
  - In `git.rs`, link the matched `autoImports` files into the base worktree when missing, as `link_missing_extends` does.
  - Bump `GRAPH_REVISION` to 11.

- [ ] **Step 4: Update the setup skill.** In `assets/skills/setup.md`: when the repo is Nuxt (a `nuxt.config.*` file, or `.nuxt/` present), set `autoImports` to the app's `.nuxt/imports.d.ts` and `.nuxt/components.d.ts` (one pair per Nuxt app in a monorepo). Note that `nuxt prepare` must have run. The same applies to `unplugin-auto-import` and `unplugin-vue-components`, using their generated `auto-imports.d.ts` and `components.d.ts`. Document the key in the README and in `docs/specs/design.md`.

- [ ] **Step 5: Run** `cargo test` and `cargo clippy --all-targets -- -D warnings` (no new errors). Expected: PASS. Commit:

```bash
git add src/autoimports.rs src/config.rs src/graph.rs src/git.rs src/main.rs assets/skills/setup.md README.md docs/specs/design.md
git commit -m "autoImports: resolve framework auto-imports (Nuxt imports.d.ts, components.d.ts)"
```

---

### Task 2: Non-test code never resolves into test files

**Files:**
- Modify: `src/graph.rs` (edge resolution)
- Test: `src/graph.rs` tests; golden files if they change

**Interfaces:**
- Consumes: `config.tests` globs (`workspace::globset`).
- Produces the rule. During resolution, an edge whose `from` file is not a test file and whose `to` file matches the test globs is dropped. That covers calls, renders, imports, extends, implements and dispatch. Test-to-test and test-to-code edges are unchanged. A dropped edge is not an error; it simply isn't emitted.
- `setup.md` also gets one line: include `**/test/**` and `**/__mocks__/**` in `tests` when stub files live there.

- [ ] **Step 1: Write the failing test** `code_never_resolves_into_tests`. Make a repo with `src/a.ts` (`import { useI18n } from '../test/stub'; useI18n();`), `test/stub.ts` (`export const useI18n = () => 1;`), and `ig.yaml` with `tests: ["**/test/**"]`. Assert:
  - no edge from `src/a.ts…` targets `test/stub.ts…`;
  - an edge from a test file to `src/` code is kept (add `test/a.test.ts` calling an exported `src` function).

- [ ] **Step 2: Run** `cargo test code_never_resolves_into_tests`. Expected: FAIL.

- [ ] **Step 3: Implement** the filter where edges are emitted in `graph.rs`, using one compiled globset per build.

- [ ] **Step 4: Run the tests.** `cargo test`. If golden files change, run `UPDATE_GOLDEN=1 cargo test --test golden`, inspect the diff (only edges into test files may disappear), and include the reason in the commit message. Expected: PASS.

- [ ] **Step 5: Commit:** `git commit -m "Code never resolves into test files"` (explicit paths).

---

### Task 3: The `callChain` model

**Files:**
- Create: `viewer/src/model/callchain.ts`, `viewer/test/callchain.test.ts`
- Delete: `viewer/src/model/sequence.ts`, after Task 4 moves its callers. In this task, leave it in place.

**Interfaces:**
- Consumes: `Index` from `viewer/src/model/graph.ts` (`byId`, `outEdges`), `ViewNode.lane` (blueprint lane index or null), `ViewNode.layer`, and `D.lanes` and `D.layers` names.
- Produces:

```ts
export interface Lifeline { key: string; label: string; system?: boolean }   // one per lane, or per layer
export interface Step {
  n: number;            // 1-based, in call order
  from: number;         // lifeline index
  to: number;
  label: string;        // the called function's name, or "GET /api/x" for http, "uses" for systems
  target: string;       // node id to open
  kind: "calls" | "dispatch" | "http" | "uses";
  file: string; line: number;   // where the call is made
}
export interface CallChain { lifelines: Lifeline[]; steps: Step[]; truncated: number }
export function callChain(ix: Index, start: string, opts?: { depth?: number; max?: number }): CallChain;   // depth default 6, max default 60
export function chainToMermaid(c: CallChain): string;
export function chainToPlantUml(c: CallChain): string;
```

- **The algorithm:**
  1. **Start:** if `start` is a module, its members and its own top-level calls are the start set (module-level script calls count); otherwise the start node itself.
  2. **Folding:** an edge whose target is in the **same file** as the current node is not a step. Instead, walk the target's outgoing edges as if they were the caller's: the local computed property's calls become the page's calls.
  3. **Kinds:** follow `calls`, `dispatch`, `http` and `uses`. Skip `guessed` confidence, test nodes, and `Error`/`Exception` classes, as today.
  4. **Http:** an `http` edge to a route is a step labelled with the route's name (`GET /api/x`); then walk the route's handler calls.
  5. **Uses:** a `uses` edge to a system is a step to the system's lifeline, and the walk stops there.
  6. **Visiting:** each node is walked once (a `seen` set), and walking stops at `depth` cross-file hops. After `max` steps, count the rest in `truncated`.
  7. **Lifelines:** by `lane` name when blueprint lanes exist (`D.lanes[lane]`), else by `layer`, else by module file basename. Systems get one lifeline each, named after the system. Lifelines are ordered by first appearance. Consecutive steps between the same pair of lifelines are kept, not merged.

- [ ] **Step 1: Write the failing tests**, in `viewer/test/callchain.test.ts`, with a hand-built `Index`. A small helper creates nodes and edges.

```ts
test("folds same-file calls and follows modules down to the database", () => {
  // page.vue: module calls local computed `title`, which calls composable useCatalog (other file, lane Front)
  // useCatalog -http-> api/routes.ts#GET /api/catalog (lane API) -calls-> api/use-cases/list.ts#list (lane Use cases)
  // list -calls-> api/repo.ts#find (lane Adapters) ; api/repo.ts -uses-> system:PostgreSQL
  const c = callChain(ix, "web/pages/page.vue");
  expect(c.lifelines.map((l) => l.label)).toEqual(["User", "Front", "API", "Use cases", "Adapters", "PostgreSQL"]);
  expect(c.steps.map((s) => s.label)).toEqual(["useCatalog", "GET /api/catalog", "list", "find", "uses"]);
  expect(c.steps[0].target).toBe("web/composables/catalog.ts#useCatalog");
  expect(c.steps.some((s) => s.label === "title")).toBe(false);
});
test("terminates on cycles", () => {                       // Review Focus 3
  // a -> b -> a, a -> a
  expect(callChain(cyc, "x/a.ts#a").steps.length).toBeLessThanOrEqual(2);
});
test("caps fan-out and reports the rest", () => {          // Review Focus 4
  const c = callChain(wide /* 80 cross-file calls */, "p.vue", { max: 60 });
  expect(c.steps.length).toBe(60);
  expect(c.truncated).toBe(20);
});
test("mermaid uses lifeline labels and step numbers", () => {
  const m = chainToMermaid(callChain(ix, "web/pages/page.vue"));
  expect(m.startsWith("sequenceDiagram")).toBe(true);
  expect(m).toContain("participant");
  expect(m).toContain("autonumber");
  expect(chainToPlantUml(callChain(ix, "web/pages/page.vue")).startsWith("@startuml")).toBe(true);
});
```

- [ ] **Step 2: Run** `cd viewer && bun run test callchain`. Expected: FAIL.

- [ ] **Step 3: Implement** `callchain.ts`. Reuse the label cleaning from `sequence.ts`: move the `clean` helper into `callchain.ts`.

- [ ] **Step 4: Run** `bun run test` and `bun run typecheck`. Expected: PASS.

- [ ] **Step 5: Commit:** `git commit -m "callChain model: across modules, through HTTP, down to systems, one lifeline per lane"` (explicit paths).

---

### Task 4: Call chain in the side panel and overlay

**Files:**
- Modify: `viewer/src/components/ig-side-panel.ts` (the "Sequence" section, about lines 115-140), `viewer/src/components/ig-overlay.ts`
- Delete: `viewer/src/model/sequence.ts` and its test, if any
- Test: `viewer/test/callchain.test.ts` (step-list rendering helper), plus a manual check

**Interfaces:**
- Consumes: Task 3's `callChain`, `chainToMermaid` and `chainToPlantUml`.
- Produces the UI:
  - **Side panel:** the "Sequence (N calls)" section becomes **"Calls down (N steps)"**. It holds a compact numbered step list, `n. Lane → Lane  label  file:line`, where each row opens the target node (focus it in the graph and show its code). Next to it: **Show diagram**, **Copy Mermaid**, **Copy PlantUML**, and a depth stepper (−/+, 1–10, default 6). With no steps it shows "No calls from here" (Review Focus 5). With `truncated > 0`, the last row reads "+N more (increase depth or max)".
  - **Overlay:** the Mermaid diagram on the left and the same clickable step list on the right. Mermaid's `autonumber` keeps the diagram's numbers matching the list. Clicking a step closes the overlay and opens that node.
  - A pure helper, `stepRows(c: CallChain): { n: number; text: string; target: string }[]`, exported from `callchain.ts` for both views and unit-tested: the row text uses lifeline labels and `file:line`.

- [ ] **Step 1: Write the failing test** `stepRows formats lane arrows and locations`: with the Task 3 fixture, `stepRows(c)[1].text` equals `"Front → API  GET /api/catalog  web/composables/catalog.ts:<line>"`, using the line set in the fixture.

- [ ] **Step 2: Run** `bun run test`. Expected: FAIL.

- [ ] **Step 3: Implement** `stepRows`, the side panel section, the overlay layout, and the depth stepper (re-computes on change). Remove `sequence.ts` and its imports.

- [ ] **Step 4: Verify.**
  1. Run `make test` and `bun run typecheck`.
  2. Run the e2e check by hand on acme, the user's real repo. It is read-only, so don't modify it; copy `ig.yaml` into a scratch copy, or point `--root` at it if `ig.yaml` already has `autoImports`.
     - With `autoImports` set to `front/acme-front/.nuxt/{imports,components}.d.ts` (and the backoffice equivalent), run `ig view --no-open` and open `dashboard/[reportName]/index.vue`.
     - The "Calls down" list must reach beyond `index.vue`, into composables and services and, where the HTTP links exist, the API routes.
     - Record the step list in the report.
  3. If acme isn't available, use the `tests/fixtures/monorepo` scratch copy with a `.nuxt/imports.d.ts` added.

- [ ] **Step 5: Commit:** `git commit -m "Viewer: Calls down — call chain list and diagram replace the sequence"` (explicit paths).

---

### Task 5: Graph gaps found on acme (added after Task 4's review)

The Task 4 review ran the chain for acme's `dashboard/[reportName]/index.vue` and found it stops early, for graph reasons:

- **A. Stale declarations.** acme's `.nuxt/imports.d.ts` predates `useDashboardCatalog` (`nuxt prepare` wasn't re-run), so the call stays a guess. Rule: a `Callee::Free(name)` that autoImports can't resolve, but whose name has **exactly one** exported definition inside a directory already covered by the declaration file's entries (the set of source directories its specs point into, such as `src/application/composables/`), resolves to that definition with confidence `resolved`. Each build then adds one notice: `autoImports: N names not in <file> resolved from its directories (run nuxt prepare): a, b, c` (at most 10 names listed).
- **B. Package-sourced names.** A name declared in a declaration file from a package (`export { useI18n } from 'vue-i18n'`) is recorded as external. A `Free` call to it resolves to **nothing**: no by-name guess, so it never lands on `test/nuxt-imports.stub.ts#useI18n`.
- **C. Bare-constant route paths.** Route ids like `GET BASE_PATH` (pattern `path: BASE_PATH`, with `const BASE_PATH = '/powerbi-integration'` in the same file) substitute same-file string constants for a **bare identifier** path, not only inside `${…}`. The result here is `GET /powerbi-integration`, so the existing HTTP matching links the template call `/api/powerbi-integration/${…}/embed-config` to `GET ${BASE_PATH}/{reportName}/embed-config`, whose id becomes `GET /powerbi-integration/{reportName}/embed-config`.

**Files:** `src/autoimports.rs`, `src/graph.rs`, `src/patterns.rs` (or wherever route ids substitute constants), `src/git.rs` (`GRAPH_REVISION` → 12). Tests go in the same modules.

- [ ] **Step 1: Write the failing tests.**
  - `stale_declarations_resolve_from_covered_dirs`: the declaration covers `src/composables/a.ts`; `b.ts` in the same directory exports `useB`, which isn't declared; a page calls `useB()`. Expect a resolved edge and one notice naming `useB`.
  - `package_names_never_guess`: the declaration has `export { useI18n } from 'vue-i18n'`; `test/stub.ts` exports `useI18n`; a page calls `useI18n()`. Expect no edge at all from the page to `test/stub.ts`, even with no `tests` globs set.
  - `bare_constant_route_path`: `const BASE_PATH = '/p'; app.get(BASE_PATH, h)` with the Express pattern. Expect route id `GET /p`, and a page calling `` fetch(`/p`) `` gets an `http` edge to it.
- [ ] **Step 2: Run them.** Expected: FAIL.
- [ ] **Step 3: Implement.** Bump `GRAPH_REVISION` to 12.
- [ ] **Step 4: Run** `cargo test` (golden files unchanged, or reviewed). Then repeat the e2e on the scratch acme copy with its **real**, stale `.nuxt`. `useDashboardCatalog` must now appear in the chain, along with the notice, and `dashboard.service.ts` must have `http` edges.
- [ ] **Step 5: Commit:** `git commit -m "autoImports: resolve stale names from covered dirs, never guess package names; bare-constant route paths"`

### Task 6: The chain follows components and inner members (added after Task 4's review)

- **D. Renders.** `callChain` follows `renders` edges into non-test components, as step kind `"renders"` labelled with the component name. It uses the same seen set and memo, so a shared child appears once per start. A page's data usually comes from its child components.
- **E. Inner members.** When a step's target is a function or class (not a module), the target's own members (its returned closures, or a class's methods) are walked as start points. This mirrors what a module start does. For example, `useDashboardCatalog`'s `loadDashboardCatalog`, which calls `DashboardService.list`, and `new DashboardService` reaching its methods.
- **Minors:**
  - A module start adds the file's `uses` steps once, not once per member.
  - The overlay step list shows "+N more" when truncated.
  - Rename `IgSidePanel.sequence()` to `callsDown()`.

**Files:** `viewer/src/model/callchain.ts`, `viewer/src/components/ig-side-panel.ts`, `viewer/src/components/ig-overlay.ts`, and `viewer/test/callchain.test.ts`.

- [ ] **Step 1: Write the failing tests.**
  - `follows renders into components`: a page renders `C.vue`, and `C.vue` calls `useX` (another file). Expect the steps `C` (renders), then `useX`.
  - `walks inner members of a called function`: page calls `useA`; `useA` has member `load`, which calls `svc.ts#Svc.list`. Expect a `list` step.
  - `module start adds uses once`: a module with 3 members and one file `uses` edge gives exactly 1 `uses` step.
- [ ] **Step 2: Run them.** Expected: FAIL.
- [ ] **Step 3: Implement** the changes above.
- [ ] **Step 4: Run** `bun run test` and `bun run typecheck`. Then run the e2e on the scratch acme copy (after Task 5). The dashboard page's chain should reach `DashboardReport`, `useDashboardCatalog` and `DashboardService`, through the HTTP routes to the API use cases and adapters, and to Power BI.
- [ ] **Step 5: Commit:** `git commit -m "Call chain follows rendered components and inner members"`
