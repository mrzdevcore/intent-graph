# Hexagonal Workspace Refactor Implementation Plan

**Goal:** Restructure ig from one flat crate (29 modules, no traits, I/O spread through the logic) into a Cargo workspace with a pure domain core, ports, adapters and thin entry points, with **no behaviour change**.

**Architecture:** Ports and adapters. `ig-core` holds the domain and the port traits. `ig-app` holds the use cases. `ig-lang-ts`, `ig-git`, `ig-fs` and `ig-checkers` are the adapters, and the `ig` binary is the driving adapters (CLI, MCP, serve, hook). Dependencies point inward only. The move happens in small green steps; golden outputs prove nothing changed.

**Tech Stack:** Rust 2024, a Cargo workspace. The existing crates are distributed to the crates that need them. `thiserror` is new, for typed errors in `ig-core` and `ig-app`.

**Spec:** `docs/specs/2026-10-25-ig-architecture-refactor-design.md`

## Global Constraints

- **No behaviour change.** These must be identical before and after every task:
  - CLI flags and output;
  - the `.ig/` files;
  - the golden files (`tests/golden/*`), which are never regenerated in this plan;
  - the viewer data JSON;
  - the skill texts.
  
  A task that needs to change a golden file is wrong; stop and report.
- **Every task ends green:** `make test` (viewer and Rust), `cargo clippy --workspace --all-targets -- -D warnings` with no new errors (7 pre-existing errors today, wherever they move), and `cargo fmt --check` on the moved code.
- **The dependency rule:**
  - `ig-core` depends on no ig crate;
  - adapters depend only on `ig-core`;
  - `ig-app` depends only on `ig-core`;
  - only the `ig` binary depends on everything.
  
  Task 8 enforces it with a test.
- **Traits only at real boundaries:** `Files`, `Vcs`, `CheckerRunner`, `LanguageFrontend`. No trait with a single implementation inside the core, no dependency-injection framework, no factories.
- **`ig-core`'s allowed external dependencies:** `serde`, `serde_json`, `serde_yaml_ng` (for config parsing), `regex`, `globset`, `sha1`, `thiserror`. No `std::fs`, `std::process` or `std::net` in `ig-core`. Task 8 greps for them.
- **No file over about 600 lines** at the end. Split by responsibility when moving one.
- `make build`, `make test` and `make install` keep working. `build.rs`, which embeds `viewer/dist/index.html`, moves with the binary crate.
- Commits: plain messages, no AI attribution, explicit paths (`git mv` for moves so history follows), never push. Never stage the screenshot files at the repo root.

## Review Focus

1. **Golden tests are the contract.** Any golden diff means behaviour changed. The task must stop and report it, not regenerate. *(every task)*
2. **The base-graph cache key** (`git.rs` `cache_key`, `GRAPH_REVISION` 14) must produce the same key after the move. Otherwise users' `.ig/cache` is invalidated silently, which is harmless but a sign of drift. A test pins the key for a fixed input. *(Task 5)*
3. **`build.rs` and the embedded viewer** must still embed `viewer/dist/index.html` from the binary crate's new location, and the placeholder must still be used when `dist` is missing. *(Task 1)*
4. **The Stop hook's stdout stays pure JSON.** Moving `hook.rs` and `check` must not add prints. *(Task 7)*
5. **The `ig mcp` stdio protocol is unchanged.** Its existing tests move with it and must pass unmodified. *(Task 7)*

---

## Target layout

```
Cargo.toml                 # [workspace] members = ["crates/*"]
crates/
  ig-core/      src/{lib.rs, model/, diff.rs, linediff.rs, config/, rules/ (layers, arch, complexity, intents, baseline), review/ (metrics by family), plan/ (parse, merge, volume), notes.rs, http.rs (url segments, route matching), ports.rs}
  ig-app/       src/{lib.rs, build.rs (graph build orchestration), check.rs, apply.rs, plan.rs, brief.rs, view_data.rs, review.rs, setup.rs, serve_state.rs}
  ig-lang-ts/   src/{lib.rs, extract/ (visitor, callee, http), embedded.rs, patterns.rs, tsconfig.rs, autoimports.rs, resolve.rs}
  ig-git/       src/lib.rs           # Vcs adapter: git CLI, worktrees, base-graph cache
  ig-fs/        src/lib.rs           # Files adapter: workspace discovery, globs, atomic writes
  ig-checkers/  src/{lib.rs, formats.rs}   # CheckerRunner adapter: spawn, timeouts, sarif/checkstyle/eslint/stylelint/lines
  ig/           build.rs, src/{main.rs, cli.rs, mcp.rs, serve.rs, hook.rs, init.rs, skills.rs, report.rs, render.rs (HTML)}, tests/golden.rs
```

## Ports (in `ig-core::ports`)

```rust
pub trait Files {                         // repo-relative paths; root fixed at construction
    fn read(&self, path: &str) -> Option<String>;
    fn write_atomic(&self, path: &str, bytes: &[u8]) -> Result<(), PortError>;
    fn remove(&self, path: &str) -> Result<(), PortError>;
    fn exists(&self, path: &str) -> bool;
    fn glob(&self, patterns: &[String]) -> Vec<String>;   // sorted, deterministic
}
pub trait Vcs {
    fn head(&self) -> Option<String>;
    fn resolve_base(&self, explicit: Option<&str>, notices: &mut Vec<String>) -> Result<Option<String>, PortError>;
    fn show(&self, rev: &str, path: &str) -> Option<String>;
    fn changed_files(&self, base: &str) -> Vec<String>;      // diff --name-only + untracked
    fn commit_exists(&self, rev: &str) -> bool;
    fn base_graph(&self, rev: &str, build: &dyn Fn(&std::path::Path) -> Result<Graph, PortError>) -> Result<Graph, PortError>; // materialise worktree, cache by key
}
pub trait CheckerRunner { fn run(&self, checker: &Checker, files: Option<&[String]>) -> CheckerOutput; }
pub trait LanguageFrontend: Sync {
    fn handles(&self, path: &str) -> bool;
    fn extract(&self, file: &SourceFile) -> FileFacts;                       // per file, parallel
    fn resolve(&self, facts: &[FileFacts], ws: &WorkspaceInfo) -> Resolved;  // edges, notices, parse errors
}
```

These shapes are the target. Exact field names follow the existing code (`FileFacts`, `Built`, `Extras` move into core as language-neutral types). The builder may adjust signatures where the existing code needs it, recording each deviation in the task report. `PortError` is a `thiserror` enum.

---

### Task 1: Workspace skeleton (pure move)

**Files:**
- Create: root `Cargo.toml` as `[workspace]`.
- Move: everything in `src/`, `build.rs`, `tests/` and the `[package]` section into `crates/ig/`.
- Modify: `Makefile`, `.gitignore`, and the README build section if paths are mentioned. The `viewer/` path in `build.rs` becomes `../../viewer/dist/index.html`.

- [ ] **Step 1: Move the code with `git mv`.** Set up `[workspace] members = ["crates/*"]`, `resolver = "3"`, and shared `[workspace.package]` edition and version. `crates/ig/Cargo.toml` keeps every current dependency.
- [ ] **Step 2: Fix paths.** The golden test uses `CARGO_MANIFEST_DIR` for fixtures, so move `tests/fixtures` and `tests/golden` with it. `build.rs` reads the viewer through `CARGO_MANIFEST_DIR/../../viewer/dist/index.html` (Review Focus 3).
- [ ] **Step 3: Verify.** Run `make test` and `make build`. Then remove `viewer/dist`, run `cargo build`, and confirm the placeholder warning appears with no build failure. Run `cargo install --path crates/ig --locked --root <scratch>` and confirm `ig --version` works. Update the Makefile `install` target to `--path crates/ig`.
- [ ] **Step 4: Commit:** `Workspace skeleton: move the ig crate to crates/ig`.

### Task 2: `ig-core` part 1: model, diff, linediff, config parsing, notes text

**Interfaces produced:**
- `ig_core::model::{Node, Edge, Graph, Finding, Range, Metrics, ParseError, sha1_hex}`
- `ig_core::diff::{GraphDiff, Status}`
- `ig_core::linediff`
- `ig_core::config::{Config, Severity, Checker, Format, Blueprint, …}`, including `Config::parse(&str)`. `Config::load(root)` stays in the binary or app, because it reads files.
- `ig_core::notes::{Note, NoteKind, parse, format}`, the pure part only. Append, set-done and wait stay where the I/O is.
- `ig_core::intents::{parse, changes}`

**Steps:**
- [ ] Create `crates/ig-core`.
- [ ] Move these modules, splitting any I/O helpers out into the binary crate.
- [ ] Re-export through `crates/ig/src/…` shims (`pub use ig_core::model::*;`) so the rest of the binary compiles unchanged.
- [ ] Unit tests move with their modules.
- [ ] Verify green.
- [ ] Commit.

### Task 3: `ig-core` part 2: rules, review metrics, plan logic, HTTP matching

**Interfaces produced:**
- `ig_core::rules::{Layers, arch, complexity, intents, baseline}`, where `baseline` holds fingerprint set operations and read/parse from a string. Reading the file moves to app or fs.
- `ig_core::review`: the metrics, split by family (cycles, hubs, cohesion, coupling, unused exports, oversized, layer cycles, regressions). Git history (`history_commits`) becomes an input passed in, not a call to git.
- `ig_core::plan::{Plan, PlanNode, PlanEdge, PlanStatus, parse, merge, volume}`. `check` and `overlay` I/O stay in app.
- `ig_core::http::{url_segments, seg_match, …}`: the route matching and constant substitution currently in `graph.rs` `blueprint_links`. The language-neutral matching lives here; extracting the HTTP calls stays in the TS frontend.

**Steps:**
- [ ] Move and split. `review.rs` (1,208 lines) is split into modules of at most about 600 lines.
- [ ] Shims in the binary crate.
- [ ] Tests move with the code.
- [ ] Verify green.
- [ ] Commit.

### Task 4: Ports and fakes

**Files:** `crates/ig-core/src/ports.rs` and `crates/ig-core/src/testing.rs` (behind `feature = "testing"`): `MemFiles` (an in-memory map), `FakeVcs` (scripted head, base, show and changed files), and `FakeRunner` (canned outputs).

**Steps:**
- [ ] Define the four traits and `PortError`, as above.
- [ ] Write the fakes.
- [ ] Add a **contract test module per port**, a function taking `&dyn Files` (and so on) and asserting the behaviour every implementation must have. Run it against the fake here; Tasks 5–6 run it against the real adapters too:
  - `write_atomic` followed by `read` round-trips;
  - `glob` is sorted;
  - `show` of a missing path is None.
- [ ] Verify green.
- [ ] Commit.

### Task 5: Adapters: `ig-fs`, `ig-git`, `ig-checkers`

- **`ig-fs`:** workspace discovery (packages, files, jsonc, globs, `normalize`, `rel`, `is_source_ext`) and `FsFiles: Files`.
- **`ig-git`:** `GitVcs: Vcs`, from today's `git.rs`. Worktrees, `link_missing_extends`, auto-import linking, `cache_key` and `GRAPH_REVISION` all move. Add a **pinned-key test**: a fixed config and declaration file contents give a known key string, so the move can't change it silently (Review Focus 2).
- **`ig-checkers`:** `ProcessRunner: CheckerRunner`, from today's `checks.rs` runner (spawn, 120 s timeout, 64 MB cap, `{files}` expansion and quoting) and the output-format parsers (`formats.rs`).

**Steps:**
- [ ] Each adapter's crate runs the Task 4 contract tests against the real implementation, in a temporary directory or git repo.
- [ ] Shims in the binary.
- [ ] Verify green.
- [ ] Commit, one commit per adapter.

### Task 6: `ig-lang-ts`: the TypeScript frontend behind `LanguageFrontend`

- **Moves into the crate:**
  - `extract.rs` (1,579 lines), split into `extract/visitor.rs`, `extract/callee.rs`, `extract/http.rs` and `extract/mod.rs`;
  - `embedded.rs`, `patterns.rs`, `tsconfig.rs` and `autoimports.rs`;
  - the TS-specific resolution half of `graph.rs`: oxc_resolver per package, exports and imports, Free/Auto/stale/external, typed members, implementers.
- **What becomes language-neutral:** graph assembly (merging nodes and edges, the test-target filter, blueprint systems and http links through `ig_core::http`) becomes `ig_app::build::build_graph(frontends: &[&dyn LanguageFrontend], files: &dyn Files, config)`.
- `TsFrontend::handles` covers today's source extensions plus `embedded`.

**Steps:**
- [ ] Move and split.
- [ ] `ig_app::build` calls the frontend.
- [ ] The golden tests are the proof (Review Focus 1).
- [ ] Unit tests move with their code.
- [ ] Verify green.
- [ ] Commit.

### Task 7: `ig-app` use cases; thin binary

- **Use cases** in `ig-app`, taking ports as parameters: `check` (today's `check::run`, minus printing), `apply` (start, accept, finish, abort, except, review_ingest, findings), `plan` (check, overlay data), `brief`, `view_data` (today's `view::data` JSON, not the HTML), `review` and `setup` (facts).
- **No I/O in `ig-app` except through ports.** No `std::fs` or `Command`: grep-checked in Task 8.
- **The binary crate keeps:**
  - `main.rs` (args, then wiring of `FsFiles`, `GitVcs`, `ProcessRunner` and `TsFrontend`, then a use case, then rendering);
  - `mcp.rs`, `serve.rs`, `hook.rs`, `init.rs`, `skills.rs` and `report.rs`;
  - `render.rs` (the HTML page from `view_data`).
  
  `view.rs` (774 lines) splits into `ig_app::view_data` and `render.rs`.
- **App tests with fakes, at least one per use case.** For example: `check` with `MemFiles` + `FakeVcs` + `FakeRunner` + the TS frontend over an in-memory file reports an arch violation, and `apply::finish` on stale state returns the "run ig apply abort" error. These run without temporary repos.
- **Verify:** golden, the MCP tests unmodified (Review Focus 5), the hook test unmodified (Review Focus 4), and green overall.
- **Commit.**

### Task 8: Enforce the architecture

- A test, `crates/ig/tests/architecture.rs`, that runs `cargo metadata --format-version 1 --no-deps` and asserts the dependency rule:
  - `ig-core` has no `ig-*` dependencies;
  - each adapter depends only on `ig-core` among `ig-*`;
  - `ig-app` depends only on `ig-core`.
- The same test reads `crates/ig-core/src/**/*.rs` and `crates/ig-app/src/**/*.rs` and asserts no occurrence of `std::fs`, `std::process`, `std::net`, `Command::new` or `File::open`.
- A line-count assertion: no `.rs` file over 650 lines in `crates/`, with a short allow-list only if a file can't sensibly be split; justify each in the report.
- `docs/specs/design.md` gets an "Architecture" section: the crate table and the dependency rule. The README contributor section gets the same in 5 lines.

**Steps:**
- [ ] Write the test, which should pass if Tasks 1–7 were right.
- [ ] Fix any violations.
- [ ] Update the docs.
- [ ] Verify green.
- [ ] Commit.
