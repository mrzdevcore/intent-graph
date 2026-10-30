# Core model, step 4a: edits in ig's core Implementation Plan

**Goal:** People, Claude and the AI reading change the spec in `ig/` through one set of edit operations that keep the user's files intact, every edit is journaled (who, undoable, kept or not), and the CLI gains `ig fmt` and `ig accept`; plan names moves and code ig does not read.

**Architecture:** A new `map::edit` module edits the `ig/*.kdl` documents in place with the `kdl` crate (format-preserving), runs the tidy step on every block it touches, re-reads the whole edited spec in memory with `spec::read` before writing anything, and returns the inverse edit. A journal in `.ig/journal.json` records each edit with its author, inverse and kept flag. The MCP server, `ig accept`, the AI reading (`ig view --read`) and later the desktop app (4b) all go through it. `compare` gains the previous committed spec to name moves.

**Tech Stack:** Rust (ig-app, ig), `kdl =6.5.0`, serde_json, existing `Files`/`Vcs` ports.

**Spec:** `docs/specs/2026-10-30-ig-core-model-design.md` (Editing, The verbs, What a spike showed). Step 1 (merged) built `spec::{write, read, parse}`, `origin::{compare, plan, plan_of, Origin}`, `ig plan`, `ig check`.

## Global Constraints

- Every change to `ig/` goes through `map::edit::apply`; no other code path writes `ig/` except step 1's first-run `write_spec` and `ig fmt`.
- An edit never changes lines it does not touch: comments, blank lines, order and formatting elsewhere in the file stay byte-identical.
- Nothing is written unless the whole edited spec reads back without error (in memory, with `spec::read`); a rejected edit writes nothing and returns the reader's errors.
- `ig/system.kdl` is written last whenever several files change.
- The journal is not committed (`.ig/` is gitignored except as `init` sets it); losing it loses undo, never the spec.
- Authors: `person`, `claude`, `reading`. Edits by `claude` and `reading` start not kept; edits by `person` start kept.
- Tests run on temp copies or `MemFiles`; never write `ig/` into source fixtures.
- Files stay under 600 lines; do not grow `crates/ig-app/src/map/facts.rs`.

## Review Focus

- A file with comments above, between and inside nodes: add, remove, move and rename leave every untouched line byte-identical (a golden test per op).
- Moving a node with children from one file to another (component into another container): both files change, indentation follows the new depth, ids and arrows pointing at it still resolve.
- Dropping a not-kept Claude edit after the person edited the same node: the inverse either applies cleanly or fails with a clear error and changes nothing.
- `ig accept` on a drift box adds exactly that box (kind, title, `at`) under the system, and the next `ig check` no longer lists it.
- `ig fmt` on a file with comments keeps every comment and the node order; `ig fmt --check` exits 1 only when formatting differs.

---

### Task 1: The edit operations

**Files:**
- Create: `crates/ig-app/src/map/edit.rs`, `crates/ig-app/src/map/edit/tidy.rs`, `crates/ig-app/src/map/edit/tests.rs`
- Modify: `crates/ig-app/src/map/mod.rs`, `crates/ig-app/src/map/spec.rs` (expose `quote` and node building for reuse; no behaviour change)

**Interfaces:**
- Consumes: `spec::{read, DIR}`, the writer's node shape (spec.rs).
- Produces:
  - `#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)] #[serde(tag = "op", rename_all = "lowercase")] pub enum Edit {`
    `Add { parent: String, kind: BoxKind, id: String, title: String, at: Vec<String>, before: Option<String> },`
    `Remove { id: String },`
    `Move { id: String, parent: String, before: Option<String> },`
    `Rename { id: String, to: String },` (the id; arrows pointing at it follow)
    `Set { id: String, key: String, value: Option<String> },` (keys: `title`, `tech`, `description`, `logo`, `slot`, `at` (value = globs joined by `\n`); `None` removes)
    `Arrow { from: String, to: String, kind: ArrowKind, label: Option<String> },`
    `Unarrow { from: String, to: String, kind: ArrowKind },`
    `Restore { parent: String, before: Option<String>, kdl: String },` (re-inserts a removed subtree verbatim; only produced as an inverse)
    `}`
  - `pub fn apply(files: &dyn Files, e: &Edit) -> Result<Edit, Vec<String>>` — applies `e`, writes the changed files, returns the inverse edit.
  - `pub(crate) fn tidy(block: &mut KdlDocument, indent: &str)` — the spike's rule: the first child's `leading` starts with a line break, every node ends with `\n`, comment lines take `indent`; a moved subtree is re-indented to its new depth.

Rules: `Add` with a new top-level node (parent = the system root) and children later gets its own file only when a later `Add` gives it a child (a childless top-level node lives in `system.kdl`; moving a node into a childless top-level node moves that node out to `ig/<stem>.kdl` first). `Remove` of a node with children or with arrows pointing at it from other nodes is an error listing them (remove those first). `Move` into its own descendant is an error.

- [ ] **Step 1: Write the failing tests** — for each op on a fixture spec with comments everywhere (`// above`, between nodes, inside blocks, trailing): the edited output equals a golden string; untouched lines byte-identical; `apply` then the returned inverse gives back the original text exactly; error cases (unknown id, duplicate id on Add/Rename, move into descendant, remove with children) return `Err` and leave files unchanged.
- [ ] **Step 2: Run to verify they fail** — `cargo test -p ig-app edit::` → FAIL (module missing).
- [ ] **Step 3: Implement** `Edit`, `apply`, `tidy`. Locate a node by its id across `ig/*.kdl`; edit the `KdlDocument`; tidy the touched blocks; serialize; `spec::read` over an in-memory overlay of the changed texts; write changed files, `system.kdl` last.
- [ ] **Step 4: Run** — `cargo test -p ig-app edit::`, then `cargo test -p ig-app`, `cargo clippy -p ig-app --all-targets` → PASS.
- [ ] **Step 5: Commit** — `git commit -m "Spec edits: add, remove, move, rename, set, arrows, in place, undoable"`

### Task 2: The journal

**Files:**
- Create: `crates/ig-app/src/map/edit/journal.rs`
- Modify: `crates/ig-app/src/map/edit.rs`, `crates/ig-app/src/map/edit/tests.rs`

**Interfaces:**
- Consumes: `apply`, `Edit` (Task 1).
- Produces:
  - `pub const JOURNAL: &str = ".ig/journal.json";`
  - `#[derive(Serialize, Deserialize)] pub enum By { Person, Claude, Reading }` (lowercase)
  - `pub struct Entry { pub n: u64, pub by: By, pub kept: bool, pub undone: bool, pub edit: Edit, pub inverse: Edit }`
  - `pub fn record(files: &dyn Files, by: By, e: &Edit) -> Result<Entry, Vec<String>>` — `apply` + append (kept = `by == Person`).
  - `pub fn entries(files: &dyn Files) -> Vec<Entry>`; `pub fn keep(files, n: Option<u64>)` (None = all not kept); `pub fn drop(files, n: u64) -> Result<(), Vec<String>>` (applies the inverse; on error nothing changes and the entry stays); `pub fn undo(files) -> Result<Option<Entry>, Vec<String>>` / `redo` (last not-undone entry / last undone).
- A journal that cannot be read is treated as empty with a notice; the spec is never touched because of it.

- [ ] **Step 1: Failing tests** — record by Claude → not kept; keep → kept; drop → file back to before, entry gone; undo/redo round trip; drop after a person edit on another node works; drop after a person removed that node fails with an error and changes nothing.
- [ ] **Step 2: Verify RED** — `cargo test -p ig-app edit::` → FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** crate tests and clippy → PASS.
- [ ] **Step 5: Commit** — `git commit -m "Spec edits: a journal (who, kept, undo, drop)"`

### Task 3: `ig fmt` and `ig accept`

**Files:**
- Modify: `crates/ig-app/src/map/edit.rs` (add `fmt`), `crates/ig/src/main.rs`, `crates/ig/src/cli.rs`, `crates/ig/tests/plan_check.rs` (or a new `crates/ig/tests/edit_cli.rs`)
- Modify: `crates/ig-app/src/map/origin.rs` (drift lines carry the drift box id: `drift: code under lib/** is in no box (lib)`)

**Interfaces:**
- `pub fn fmt(files: &dyn Files) -> Result<Vec<String>, Vec<String>>` — formats every `ig/*.kdl` canonically (autoformat, the writer's `quote` pass, comments kept, order kept); returns the paths it would change; `ig fmt` writes them, `ig fmt --check` writes nothing and exits 1 when the list is not empty (2 on a broken spec).
- `ig accept [--all | <id>…]`: for each drift box id from `ig plan`, `record(By::Person, Add { parent: <system root>, kind, id, title, at })` from the drift box; for each drift arrow (`from->to~drift` or `from->to`), `record(By::Person, Arrow { … })`. Prints what it added; exit 0; unknown id → exit 2 listing it. `USAGE` gains `| fmt [--check] | accept [--all | <id>...]`.

- [ ] **Step 1: Failing tests** — temp repo: `ig plan` (writes ig/), add stray code under a package, `ig check` exit 1, `ig accept lib` (or the printed id) exit 0, `ig check` exit 0; `ig fmt --check` on a hand-mangled indentation exits 1, `ig fmt` fixes it and keeps a comment, `--check` then exits 0.
- [ ] **Step 2: Verify RED.**
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `cargo test -p intent-graph`, clippy → PASS.
- [ ] **Step 5: Commit** — `git commit -m "ig fmt and ig accept"`

### Task 4: Claude edits through MCP

**Files:**
- Modify: `crates/ig/src/mcp.rs`, `crates/ig/src/mcp/tests.rs`

**Interfaces:**
- Tool `ig_plan` (no input): the plan as text (same lines as `ig plan`) and `structuredContent: { build: [..], drift: [..] }`.
- Tool `ig_edit` (input: one `Edit` as JSON, schema derived from the enum's fields; or `{ "edits": [Edit, …] }` applied in order, stopping at the first error): records each with `By::Claude`; returns the plan after the edits, and the journal numbers. Errors return `isError: true` with the reader's messages.
- Tool descriptions say: edits are proposals the user keeps or drops; always pass `at` for new boxes; ids are slugs.

- [ ] **Step 1: Failing tests** (`mcp/tests.rs`, temp repo) — `tools/list` contains `ig_edit` and `ig_plan`; `ig_edit` Add then `ig_plan` shows it "to build"; the journal entry is `claude`, not kept; a bad edit returns `isError` and changes nothing.
- [ ] **Step 2: Verify RED.**
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `cargo test -p intent-graph mcp`, clippy → PASS.
- [ ] **Step 5: Commit** — `git commit -m "MCP: ig_plan and ig_edit (Claude's edits as proposals)"`

### Task 5: The AI reading as proposals

**Files:**
- Modify: `crates/ig-app/src/map/view.rs` (the `ask` branch with an existing spec), `crates/ig-app/src/map/agent.rs` only if needed to read a map built from the spec, `crates/ig-app/src/map/view.rs` tests

**Interfaces:**
- With an existing spec and `--read` (`ask.all`): build the compared map, run today's reading (`offer`) on the boxes that are drift or have no description; turn its result into edits recorded `By::Reading`: `Set` for title/description/tech/logo changes on spec boxes, `Add` (parent = system root, the reading's kind/title, the drift box's `at`) for drift boxes. Notice: `the AI reading proposed <n> edits; keep or drop them in the app, or review with git diff ig/`. Without `--read`: no reading, no notice (today's behaviour).

- [ ] **Step 1: Failing tests** (fake agent as in `agent/tests.rs`) — a spec box without description gets a `Set` proposal; a drift box gets an `Add` proposal; journal entries are `reading`, not kept; files changed accordingly; a reading that fails changes nothing.
- [ ] **Step 2: Verify RED.**
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `cargo test -p ig-app`, clippy → PASS.
- [ ] **Step 5: Commit** — `git commit -m "The AI reading proposes edits to ig/"`

### Task 6: Moves, and code ig does not read

**Files:**
- Modify: `crates/ig-app/src/map/origin.rs` (+ tests), `crates/ig-app/src/map/view.rs`, `viewer/src/map/types.ts`, `viewer/src/map/looks.ts`, `viewer/test/map-looks.test.ts`

**Interfaces:**
- `compare(spec, facts, g, prior: Option<&MapFile>)` — `prior` = the spec at the last commit (view.rs reads `ig/*.kdl` at HEAD through `Vcs::show`, parses with `spec::read` over an in-memory `Files`; none without git or a commit). A spec box whose `at` differs from its `at` in `prior`, whose new place has no code and whose old place still does, is `Origin::Moved` (serialized `moved`), and the code at its old place is not drift. Plan line: `move: <old glob> → <new glob> (<id>)`.
- Code ig does not read: files tracked by git (not gitignored) with an extension ig's frontends handle, under no package ig reads → plan line `not read: <folder>/** (outside every package ig reads)`; counted with drift for `ig check`.
- Viewer: `origin: "moved"` → edge `var(--map-blue)`, dashed, chip `{ label: "to move", tone: "neutral" }`.

- [ ] **Step 1: Failing tests** — prior spec with `at "apps/api/src/refund/**"`, current with the box moved under billing at `billing/src/refund/**`, code still at the old place → origin moved, plan has the `move:` line, no drift line; a stray `scripts/seed.ts` outside every package → `not read: scripts/**`, `ig check` exit 1; viewer looks test for `moved`.
- [ ] **Step 2: Verify RED.**
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `cargo test`, clippy, `cd viewer && bun run test && npx svelte-check --threshold warning` → PASS.
- [ ] **Step 5: Commit** — `git commit -m "Plan names moves, and code ig does not read"`

### Task 7: The deferred fixes from step 1

**Files:**
- Modify: `crates/ig-app/src/map/spec.rs`, `crates/ig-app/src/map/origin.rs`, their tests

**Interfaces / behaviour:**
- A bundle arrow is written with its most informative kind: the first of its `kinds` in the order `calls, network, event, data, implements, uses, depends` (instead of enum order).
- Cross-level arrow matching skips pairs where one end is an ancestor of the other (including the system root): an arrow from a component to its own container, or to/from the system, is never satisfied by and never covers code inside the same subtree.
- `owner` in `arrows_against` is filled deepest-first, as `arrows()` does.

- [ ] **Step 1: Failing tests** — a calls+imports bundle writes `calls`; a hand-written `C -> its container` arrow stays to build and covers no sibling drift.
- [ ] **Step 2: Verify RED.** **Step 3: Implement.** **Step 4: Run** crate tests, clippy → PASS.
- [ ] **Step 5: Commit** — `git commit -m "Spec: the most telling arrow kind; no ancestor pairs in arrow matching"`

## Not in this step

- Editing on the map in the desktop app, and the keep/drop bar (4b; designed on the canvas first).
- Kind renames (`api` → `service`): not needed for editing; left until a use for them appears.
- Selectors and `ig_query`/`ig_get` (step 2), `ig apply` (step 5).
