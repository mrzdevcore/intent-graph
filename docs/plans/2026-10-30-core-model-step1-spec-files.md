# Core model, step 1: the `ig/` spec files, plan and check Implementation Plan

**Goal:** The intended architecture lives in committed KDL files under `ig/`; ig writes them once (from today's `.ig/map.json` or from the code), then never rewrites them, and reports what is still to build and what drifted, on the map and through `ig plan` / `ig check`.

**Architecture:** Keep today's in-memory map (`map::file::MapFile`, its boxes and arrows) as the model at architecture depth; add `map::spec` to read and write it as `ig/*.kdl`, and `map::origin` to compare it with the code: each spec box claims the code files under its `at` globs (deepest box wins), arrows are lifted over those claims with today's `arrows()`, and every box and arrow gets an origin (built, to build, drift). `view::run_with` switches from "refresh `.ig/map.json` from the code" to "read the spec, compare with the code". The viewer draws the origins.

**Tech Stack:** Rust (ig-app, ig), `kdl` crate pinned `=6.5.0` (6.7.1 needs Rust 1.95; this toolchain is 1.93), globset (already used), Svelte viewer.

**Spec:** `docs/specs/2026-10-30-ig-core-model-design.md` (build order step 1). Its spike section explains the `kdl` whitespace model; this step only writes whole files, so the tidy step for edits belongs to step 4.

## Global Constraints

- The spec wins: after `ig/` exists, no ig code path writes to `ig/` (only step 4's edits will). A test pins this.
- Architecture depth only in `ig/`: levels `system`, `container`, `component`. Code-level boxes (`function`, `class`, `method`, `file`, component `parts`, Ansible `task`) are never written.
- "You check what you write": what the spec describes must exist; code inside a described box's `at` that the spec does not describe is fine; code under no box's `at` is drift; arrows between described boxes that the spec does not list are drift; things ig cannot detect (people, labels, descriptions, `tech`) are never checked.
- Deterministic output: files, nodes and properties in a stable order; byte-identical on every run of `ig view` against an unchanged tree.
- `ig/` file errors name the file and line (miette spans from `kdl`).
- Viewer styles use tokens (`viewer/src/styles/tokens.css`, `themes/*.css`), rem, no raw hex; text, never `{@html}`.
- Kind mapping from today's kinds: `api` → `service` is NOT applied in this step (the spec writes today's `BoxKind` names as they are, so nothing a user sees renames); new kinds come with step 4.

## Review Focus

- A repo with `.ig/map.json` written by an AI reading and a person's corrections: after migration the map looks the same (titles, descriptions, kinds, the agent's own grouping), and `ig check` passes on the unchanged code.
- A code file moved into a folder no box claims (a new top-level folder): `ig check` fails naming that folder, the map shows it as drift, `ig/` is untouched.
- A spec box whose `at` matches nothing (a planned service): shown dashed "to build", `ig check` fails naming it; its arrows show "to build" too.
- Two boxes whose `at` globs overlap (`api/**` and `api/src/checkout/**`): each file belongs to the deepest box only, so the api's arrows do not double-count checkout's.
- A hand-edited `ig/` file with a syntax error or an unknown parent: ig reports file and line, draws nothing from a half-read spec, and never overwrites the file.

---

### Task 1: Write the model as `ig/` files

**Files:**
- Create: `crates/ig-app/src/map/spec.rs`, `crates/ig-app/src/map/spec/tests.rs`
- Modify: `crates/ig-app/src/map/mod.rs` (add `pub mod spec;`), `crates/ig-app/Cargo.toml` (`kdl = "=6.5.0"`)

**Interfaces:**
- Consumes: `MapFile`, `MapBox`, `MapArrow` (`map::file`, `map::arrows`).
- Produces:
  - `pub const DIR: &str = "ig";`
  - `pub fn write(m: &MapFile) -> BTreeMap<String, String>` — relative path (`ig/system.kdl`, `ig/<id>.kdl`) → file text.

Layout: `ig/system.kdl` holds the `system` node and, inside it, every direct child that has no children of its own; each direct child of the system that has children gets its own file holding just that node (file stem = the box id with `/` replaced by `-`). Node shape: `<kind> "<id>" title="…" [tech="…"] [description="…"] [logo="…"] [slot="…"]`, then a block with, in order: one `at "<glob>"…` node listing the `made_of` globs (omitted when empty), `badges "<b>"…`, `pattern "<kind>" ["<band>"…]`, the box's outgoing arrows as `<arrow-kind> "<to-id>" [label="…"]`, then child boxes (sorted by id). Not written: field sources and confidence, `pinned`, `files`, `package`, `service_name`, `needs_reading`, `stale`, arrow counts, marks. Every file starts with the comment `// ig's intended architecture: edit freely; ig never rewrites this file.`

- [ ] **Step 1: Write the failing tests** (in `spec/tests.rs`, on `crate::map::shop::load()` run through `facts` + `refresh(None, …)` + relift, as `view.rs` tests do)

```rust
#[test]
fn the_shop_is_written_one_file_per_container() {
    let files = write(&shop_map());
    assert_eq!(files.keys().collect::<Vec<_>>(), ["ig/api-client.kdl", "ig/api.kdl", "ig/system.kdl", "ig/web.kdl"]);
    let sys = &files["ig/system.kdl"];
    assert!(sys.starts_with("// ig's intended architecture"));
    assert!(sys.contains("system \"system\""));
    assert!(sys.contains("store \"postgres\""));
    assert!(sys.contains("external \"stripe\""));
    let api = &files["ig/api.kdl"];
    assert!(api.contains("app \"api\" title=\"Api\""));
    assert!(api.contains("at \"apps/api/**\""));
    assert!(api.contains("data \"postgres\""));
    assert!(api.contains("network \"stripe\""));
    assert!(api.contains("usecase \"api/application-checkout\""));
    assert!(!api.contains("function"), "code level never written");
}

#[test]
fn writing_is_deterministic_and_every_file_parses() {
    let (a, b) = (write(&shop_map()), write(&shop_map()));
    assert_eq!(a, b);
    for t in a.values() { t.parse::<kdl::KdlDocument>().unwrap(); }
}
```

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p ig-app spec::`
Expected: FAIL (module `spec` missing).

- [ ] **Step 3: Implement `write`** building `KdlNode`s and calling `autoformat` on each document.

- [ ] **Step 4: Run to verify they pass**

Run: `cargo test -p ig-app spec::`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/ig-app/Cargo.toml Cargo.lock crates/ig-app/src/map/mod.rs crates/ig-app/src/map/spec.rs crates/ig-app/src/map/spec/tests.rs
git commit -m "Spec: write the map as ig/*.kdl, one file per container"
```

### Task 2: Read `ig/` back into the model, strictly

**Files:**
- Modify: `crates/ig-app/src/map/spec.rs`, `crates/ig-app/src/map/spec/tests.rs`

**Interfaces:**
- Consumes: `write` (Task 1); `file::validate` (structure rules).
- Produces: `pub fn read(files: &dyn Files) -> Result<Option<MapFile>, Vec<String>>` — `Ok(None)` when `ig/` holds no `.kdl` file; every field read is `Field::person(…)` with all `PINNABLE` names in `pinned` and `made_of_source: Source::Person`; arrows get `count: 0` and `kinds: vec![kind]`. Errors are `"<path>:<line>: <what>"`.

Checks: KDL syntax; exactly one `system` node across all files; a non-system file holds exactly one top-level node; known `BoxKind` names; unique ids; arrow targets exist; globs compile; then `file::validate`. Any error: `Err`, nothing partial.

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn what_is_written_reads_back_the_same() {
    let m = shop_map();
    let w = write(&m);
    let files = MemFiles::with(&w.iter().map(|(p, t)| (p.as_str(), t.as_str())).collect::<Vec<_>>());
    let r = read(&files).unwrap().unwrap();
    let key = |b: &MapBox| (b.id.clone(), b.parent.clone(), b.kind.value, b.title.value.clone(), b.made_of.clone());
    assert_eq!(r.boxes.iter().map(key).collect::<Vec<_>>(), m.boxes.iter().filter(|b| !b.stale).map(key).collect::<Vec<_>>());
    assert_eq!(r.arrows.iter().map(|a| (&a.from, &a.to, a.kind)).collect::<Vec<_>>(),
               m.arrows.iter().map(|a| (&a.from, &a.to, a.kind)).collect::<Vec<_>>());
}

#[test]
fn a_broken_spec_names_file_and_line_and_reads_nothing() {
    let files = MemFiles::with(&[("ig/system.kdl", "system \"system\" {\n  app \"web\" {\n    calls \"nowhere\"\n  }\n}\n")]);
    let e = read(&files).unwrap_err();
    assert!(e.iter().any(|x| x.starts_with("ig/system.kdl:3:") && x.contains("nowhere")), "{e:?}");
    let files = MemFiles::with(&[("ig/system.kdl", "system \"system\" {\n  app \"web\" title=\n}\n")]);
    assert!(read(&files).unwrap_err()[0].starts_with("ig/system.kdl:2:"));
    assert_eq!(read(&MemFiles::with(&[])).unwrap(), None);
}
```

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p ig-app spec::`
Expected: FAIL (`read` missing).

- [ ] **Step 3: Implement `read`** (line numbers from the node spans).

- [ ] **Step 4: Run to verify they pass**

Run: `cargo test -p ig-app spec::`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/ig-app/src/map/spec.rs crates/ig-app/src/map/spec/tests.rs
git commit -m "Spec: read ig/*.kdl strictly, errors with file and line"
```

### Task 3: Compare the spec with the code (origins)

**Files:**
- Create: `crates/ig-app/src/map/origin.rs`, `crates/ig-app/src/map/origin/tests.rs`
- Modify: `crates/ig-app/src/map/mod.rs`, `crates/ig-app/src/map/file.rs` (field on `MapBox`), `crates/ig-app/src/map/arrows.rs` (field on `MapArrow`)

**Interfaces:**
- Consumes: `read` (Task 2); `facts`, `arrows` (`map`); the graph `Graph`.
- Produces:
  - `#[serde(rename_all = "lowercase")] pub enum Origin { Built, Build, Drift }` (default `Built`), as `#[serde(skip_serializing_if = "is_built", default)] pub origin: Origin` on `MapBox` and `MapArrow`.
  - `pub fn compare(spec: &MapFile, facts: &[FactBox], g: &Graph) -> MapFile` — the spec's boxes (origin `Built` when their `at` claims at least one module file, else `Build`; a box with no `at` and no children is `Built`: it is a declaration), plus drift boxes: each fact box whose module files are all claimed by no spec box, added under the system with `origin: Drift`, `Field::ig` values from the fact; its `nodes` set. Arrows: today's `arrows()` over the spec boxes (with `nodes` = the files each claims, the deepest `at` match wins) and the drift boxes; an arrow the spec lists and the code has → `Built`; listed, not in the code → `Build`; in the code between two spec boxes, not listed → `Drift`; touching a drift box → `Drift`.
  - `pub struct Plan { pub build: Vec<String>, pub drift: Vec<String> }` and `pub fn plan(m: &MapFile) -> Plan` — one line per item, e.g. `"to build: service billing (billing/**)"`, `"drift: code under apps/admin/** is in no box"`, `"drift: web calls postgres, not in ig/"`, sorted.

- [ ] **Step 1: Write the failing tests** (shop: spec = `write` → `read` of the shop map; facts and graph from `shop::load()`)

```rust
#[test] fn the_unchanged_code_matches_its_spec() {
    let m = compare(&spec(), &facts(), &graph());
    assert!(m.boxes.iter().all(|b| b.origin == Origin::Built));
    assert!(m.arrows.iter().all(|a| a.origin == Origin::Built));
    assert_eq!(plan(&m), Plan { build: vec![], drift: vec![] });
}
#[test] fn a_planned_box_and_its_arrow_are_to_build() {
    // add `service "billing" at "billing/**"` with `network "stripe"` to the spec
    let m = compare(&with_billing(spec()), &facts(), &graph());
    assert_eq!(origin_of(&m, "billing"), Origin::Build);
    assert_eq!(arrow_origin(&m, "billing", "stripe"), Origin::Build);
    assert!(plan(&m).build.iter().any(|l| l.contains("billing")));
}
#[test] fn code_in_no_box_and_unlisted_arrows_are_drift() {
    // remove `web` from the spec, and its arrow `network "api"`
    let m = compare(&without(spec(), "web"), &facts(), &graph());
    assert_eq!(origin_of(&m, "web"), Origin::Drift);
    assert!(plan(&m).drift.iter().any(|l| l.contains("apps/web")));
}
#[test] fn the_deepest_box_claims_a_file() {
    // api (apps/api/**) and checkout (apps/api/src/application/checkout/**) overlap
    let m = compare(&spec(), &facts(), &graph());
    let api = m.boxes.iter().find(|b| b.id == "api").unwrap();
    assert!(api.nodes.iter().all(|n| !n.contains("application/checkout")));
}
#[test] fn code_inside_a_described_box_needs_no_description() {
    // drop every component of api from the spec: api still claims all its files, no drift
    let m = compare(&without_children(spec(), "api"), &facts(), &graph());
    assert!(plan(&m).drift.is_empty(), "{:?}", plan(&m));
}
```

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p ig-app origin::`
Expected: FAIL (module missing).

- [ ] **Step 3: Implement `Origin`, `compare` and `plan`.** Claiming: compile each box's `made_of` globs; for each module file, the matching box with the longest box path (most ancestors) claims it.

- [ ] **Step 4: Run to verify they pass, and the crate**

Run: `cargo test -p ig-app`
Expected: PASS (existing tests unchanged: `origin` is skipped when `Built`).

- [ ] **Step 5: Commit**

```bash
git add crates/ig-app/src/map
git commit -m "Spec: compare ig/ with the code: built, to build, drift"
```

### Task 4: `ig view` reads the spec; the first run writes it

**Files:**
- Modify: `crates/ig-app/src/map/view.rs` (`run_with`), `crates/ig-app/src/map/view.rs` tests, `crates/ig/src/render.rs` (`.ig/.gitignore` content), `crates/ig/assets/skills/explore.md`, `README.md`, `docs/system-map-v1-summary.md` (mentions of `.ig/map.json`)

**Interfaces:**
- Consumes: `spec::{read, write, DIR}`, `origin::compare` (Tasks 1–3); today's `refresh`, `relift`, `offer`.
- Produces: `run_with(env, ask) -> View` with this flow:
  1. `spec::read`. `Err(e)`: notices `e`, plus "ig/ left as is; fix it", and the view of today's code alone (facts + `refresh(None…)`), never writing.
  2. `Ok(None)` (first run): build today's map as now (`file::load` of `.ig/map.json` as the old map when present, facts, `refresh`, `relift`, the AI reading when `ask` consents), then write every file of `spec::write` and push the notice `"wrote ig/: the intended architecture, from <.ig/map.json | the code>; edit it, commit it"`. Continue with step 3 on what was written.
  3. `Ok(Some(spec))`: `compare(spec, facts, g)`, then marks (`rules`, `risks`) as `relift` does today, then `view`. `ask` with an existing spec: no reading; notice `"the AI reading runs when ig/ is written; it will propose edits in a later version"`.
  - `.ig/map.json` is never written again; it is left where it is (the user deletes it).

- [ ] **Step 1: Write the failing tests** (in `view.rs` tests, `MemFiles`)

```rust
#[test] fn the_first_run_writes_ig_and_later_runs_never_rewrite_it() {
    let files = MemFiles::with(&[("src/a.ts", "export function f() {}\n")]);
    let v = run(&env(&files, None, &Fake)).unwrap();
    assert!(v.notices.iter().any(|n| n.starts_with("wrote ig/")));
    let before = files.read("ig/system.kdl").unwrap();
    files.write_atomic("lib/c.ts", b"export const y = 2;\n").unwrap();  // outside every `at`: drift
    let v = run(&env(&files, None, &Fake)).unwrap();
    assert_eq!(files.read("ig/system.kdl").unwrap(), before);
    assert!(v.boxes.iter().any(|b| b.origin == Origin::Drift));
}
#[test] fn a_map_json_is_migrated_with_its_readings() {
    // MemFiles with src files and a .ig/map.json whose box `root` has title "Core" by Source::Person
    let v = run(&env(&files, None, &Fake)).unwrap();
    assert!(files.read("ig/system.kdl").unwrap().contains("title=\"Core\""));
    assert!(v.notices.iter().any(|n| n.contains("from .ig/map.json")));
}
#[test] fn a_broken_spec_is_reported_and_kept() {
    // ig/system.kdl = "system {" (unclosed)
    let v = run(&env(&files, None, &Fake)).unwrap();
    assert!(v.notices.iter().any(|n| n.starts_with("ig/system.kdl:")));
    assert_eq!(files.read("ig/system.kdl").as_deref(), Some("system {"));
}
```

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p ig-app view::`
Expected: FAIL.

- [ ] **Step 3: Implement the flow in `run_with`;** update the old tests that assert `.ig/map.json` is written (`notices_reach_the_view_and_a_rejected_map_is_kept`, `an_invalid_map_is_never_written`) to the new behaviour; replace `.ig/map.json` in the listed docs and skill with `ig/`.

- [ ] **Step 4: Run the whole workspace**

Run: `cargo test` and `cargo clippy --all-targets`
Expected: all pass except the known `files_stay_small` (`facts.rs` 604 lines, unrelated); clippy clean. Golden tests: `origin` is skipped when `Built`, so goldens only change where a test fixture now gets an `ig/` written; regenerate per the golden test's own instructions and review the diff.

- [ ] **Step 5: Commit**

```bash
git add -A crates docs README.md
git commit -m "ig view: the spec in ig/ is read, written once, never rewritten"
```

### Task 5: `ig plan` and `ig check`

**Files:**
- Modify: `crates/ig/src/main.rs` (remove `check` from `removed()`; add `plan`, `check`), `crates/ig/src/cli.rs` (`USAGE`), `crates/ig/src/tests/` (CLI test module that runs commands on a temp repo; use the existing `crate::tests::tmp_with`)

**Interfaces:**
- Consumes: `view::run(&env)` (Task 4), `origin::plan`.
- Produces:
  - `ig plan`: prints `ig plan: <n> to build, <m> drift` then the `plan()` lines; exit 0.
  - `ig check`: prints the same; exit 0 when both lists are empty, else 1. With a broken spec: the errors, exit 2.
  - Both write `ig/` on a first run (Task 4's flow) and say so.
  - `USAGE` gains `| plan | check`.

- [ ] **Step 1: Write the failing tests**

```rust
#[test] fn check_passes_on_a_fresh_repo_then_fails_on_drift() {
    let dir = tmp_with(&[("src/a.ts", "export function f() {}\n")]);
    assert_eq!(run_ig(&dir, &["check"]).code, 0);                 // writes ig/, matches
    std::fs::create_dir_all(dir.join("lib")).unwrap();
    std::fs::write(dir.join("lib/c.ts"), "export const y = 2;\n").unwrap();
    let out = run_ig(&dir, &["check"]);
    assert_eq!(out.code, 1);
    assert!(out.stdout.contains("drift: code under lib/"), "{}", out.stdout);
    assert_eq!(run_ig(&dir, &["plan"]).code, 0);
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p intent-graph check_passes`
Expected: FAIL (`ig check` prints "removed", exit 2).

- [ ] **Step 3: Implement both commands.**

- [ ] **Step 4: Run to verify it passes**

Run: `cargo test -p intent-graph` and `cargo clippy --all-targets`
Expected: PASS, clippy clean.

- [ ] **Step 5: Commit**

```bash
git add crates/ig/src
git commit -m "ig plan and ig check: what is to build and what drifted"
```

### Task 6: The map draws the origins

**Files:**
- Modify: `viewer/src/map/types.ts` (`origin?: "build" | "drift"` on `MapBox` and `MapArrow`), `viewer/src/map/looks.ts`, `viewer/src/map/MapBox.svelte`, `viewer/src/map/MapArrow.svelte`, `viewer/src/map/DetailsCard.svelte`, `viewer/src/styles/themes/light.css` and `dark.css` (no new tokens expected: `--map-trace`, `--map-warn`), `viewer/test/map-looks.test.ts`, `viewer/test/map-canvas.test.ts`

**Interfaces:**
- Consumes: the View JSON's `origin` (Task 3; absent = built).
- Produces: `look()` returns, for `origin: "build"`, `edge: "var(--map-trace)"`, `dashed: true`, a chip `{ label: "to build", tone: "neutral" }`, `fade: 0.85`; for `"drift"`, `edge: "var(--map-warn)"`, `dashed: true`, chip `{ label: "not in ig/", tone: "warn" }`. Health and rule marks still take precedence over origin. `arrowLook()`: `"build"` → `color: "var(--map-trace)"`, `dash: "6 4"`; `"drift"` → `color: "var(--map-warn)"`, `dash: "2 4"`, `label` "not in ig/" when it has none. DetailsCard shows one line under the meta: "To build: no code under `<at>` yet" or "Drift: this code is in no box of ig/".

Design: follows the "Design mode" row of the canvas (planned dashed gold, unplanned amber); no new artboard needed.

- [ ] **Step 1: Write the failing tests**

```ts
test("a box to build is dashed gold with a chip; drift is dashed amber", () => {
  const b = (origin?: "build" | "drift") => ({ ...box, origin });
  expect(look(b().id, [], new Set(), b("build"))).toMatchObject({ edge: "var(--map-trace)", dashed: true, chips: [{ label: "to build" }] });
  expect(look(b().id, [], new Set(), b("drift"))).toMatchObject({ edge: "var(--map-warn)", dashed: true, chips: [{ label: "not in ig/" }] });
  expect(look(b().id, [], new Set(), b())).toMatchObject({ dashed: false });
});
test("an arrow to build or in drift", () => {
  expect(arrowLook({ ...arrow("calls"), origin: "build" }, [], new Set())).toMatchObject({ color: "var(--map-trace)", dash: "6 4" });
  expect(arrowLook({ ...arrow("calls"), origin: "drift" }, [], new Set())).toMatchObject({ color: "var(--map-warn)", label: "not in ig/" });
});
```

- [ ] **Step 2: Run to verify they fail**

Run: `cd viewer && bun run test test/map-looks.test.ts`
Expected: FAIL.

- [ ] **Step 3: Implement the looks and the DetailsCard line.**

- [ ] **Step 4: Run all viewer checks**

Run: `cd viewer && bun run test && npx svelte-check --threshold warning && bun run build`
Expected: pass, 0 errors, 0 warnings.

- [ ] **Step 5: See it** — `cargo build --bin ig`, then in a scratch copy of a repo: `target/debug/ig view`, add a planned service to `ig/system.kdl`, add a stray folder of code, `ig view` again: dashed gold box and arrow, amber drift box; `ig check` exits 1 listing both.

- [ ] **Step 6: Commit**

```bash
git add viewer
git commit -m "Map: boxes and arrows to build, and drift"
```

## Not in this step (later steps of the design)

- Naming moves ("moved", not "drift + to build"): moves come from edits, step 4.
- Kind renames (`api` → `service`), the tidy step, `ig_edit`, the journal, Claude's gold edits, `ig accept`: step 4.
- Selectors and `ig_query` / `ig_get`: step 2.
- The AI reading after `ig/` exists, as proposals: step 4.
