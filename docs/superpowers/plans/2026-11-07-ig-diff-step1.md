# `ig diff`, step 1 (the comparison and `--json`): implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `ig diff [<base>] [<head>]` compares the architecture of two sides of a change and prints it as text or `--json`; the map page comes in step 3.

**Architecture:** A pure `ig_app::map::diff::diff(base, head, changed)` over two architecture-level `Side`s; `view::architecture` builds a side from any `Env` (working tree or a `Vcs::checkout`); a thin `ig diff` command in the CLI crate prints it.

**Tech Stack:** Rust workspace (`ig-app`, `ig-core` ports, `ig-git`, CLI crate `intent-graph` with clap), `serde` for the JSON, real `git` in integration tests.

**Spec:** `docs/superpowers/specs/2026-11-06-ig-diff-design.md` (read "What the diff holds" and "The command" first).

## Global Constraints

- Branch `ig-diff`. Never stage the top-level `ig/` directory or `.claude/` (the user's uncommitted work); `git add` explicit paths only. Never push.
- Exit status: 0 whatever the diff holds; 2 on usage errors, a rev that is not a commit, outside a git repository, or a folder of projects.
- stdout carries results only (the diff text or JSON); notices and progress on stderr; `-q` drops notices, never errors.
- JSON keys camelCase, like the view. Field names in this plan are fixed: project 3 consumes them.
- The diff is of the architecture level only: the `MapFile` before `view()` adds the code level.
- Both sides use the head's `ig.yaml`.
- Writes nothing in the working tree but `.ig/` (step 1 writes nothing at all); never touches the user's branches, index or stash.
- No `.rs` file over 600 lines; code comments only a one-line non-obvious "why", none restating code, also in tests.
- Commits: no `Co-Authored-By` or "Generated with" lines.
- Ruling (plan): the base checkout uses the existing `Vcs::checkout` (a `git worktree` in the system temp dir, removed on drop), not a directory under `.ig/`, and no graph cache in step 1 — the spec allows "removed or reused"; a cache is a later optimisation.

## Review Focus

1. Same commit on both sides (`ig diff HEAD HEAD`) and a working tree with no changes: an empty diff, summary `no architecture change` (Task 1 unit test, Task 3 integration test).
2. A base where `ig/` does not exist and a head where it does: no box churn, intent `added` (Task 1).
3. A base whose `ig/` does not read: code-only on both sides, intent `baseBroken` with the first error (Task 1).
4. Drift whose id counter shifts between sides (`x-drift` vs `x-drift-2` for the same `madeOf`): not reported as new (Task 1).
5. A rev that is not a commit, outside git, and a folder of projects: exit 2 with one line on stderr, nothing written (Task 3).

---

### Task 1: the pure diff

**Files:**
- Create: `crates/ig-app/src/map/diff.rs`, `crates/ig-app/src/map/diff/tests.rs`
- Modify: `crates/ig-app/src/map/mod.rs` (`pub mod diff;`)

**Interfaces (Produces):**
```rust
pub enum SpecSide { Read(MapFile), Missing, Broken(String) }   // ig/ as spec::read gives it; Broken = first error
pub struct Side { pub arch: MapFile, pub spec: SpecSide }        // arch: architecture-level map of that side

#[derive(Serialize)] #[serde(rename_all = "camelCase")]
pub struct Diff {
    pub base: String, pub head: Option<String>,                 // head None = working tree
    pub source: Source,                                          // "spec" | "code": where box ids came from
    pub summary: String,
    pub boxes: BoxChanges, pub arrows: ArrowChanges,
    pub intent: Intent, pub problems: Problems,
}
pub enum Source { Spec, Code }                                   // serde lowercase
pub struct BoxRef { pub id: String, pub title: String, pub parent: Option<String> }
pub struct ChangedBox { pub id: String, pub title: String, pub parent: Option<String>, pub files: Vec<String>, pub files_changed: u32 }
pub struct BoxChanges { pub added: Vec<BoxRef>, pub removed: Vec<BoxRef>, pub changed: Vec<ChangedBox> }
pub struct ArrowRef { pub from: String, pub to: String, pub kind: String }
pub struct ArrowChanges { pub added: Vec<ArrowRef>, pub removed: Vec<ArrowRef> }
pub enum IntentStatus { Same, Changed, Added, Removed, BaseBroken }   // serde camelCase
pub struct Moved { pub id: String, pub from: Option<String>, pub to: Option<String> }
pub struct FieldsChanged { pub id: String, pub fields: Vec<String> }
pub struct Intent { pub status: IntentStatus, pub note: Option<String>, pub added: Vec<BoxRef>, pub removed: Vec<BoxRef>, pub moved: Vec<Moved>, pub changed: Vec<FieldsChanged>, pub arrows_added: Vec<ArrowRef>, pub arrows_removed: Vec<ArrowRef> }
pub enum ProblemKind { Rule, Drift }                             // serde lowercase
pub struct Problem { pub kind: ProblemKind, pub text: String, pub boxes: Vec<String> }
pub struct Problems { pub new: Vec<Problem>, pub fixed: Vec<Problem> }

pub fn diff(base: &Side, head: &Side, changed: &[String], base_rev: &str, head_rev: Option<&str>) -> Diff;
```
All structs `Serialize` with `rename_all = "camelCase"`; empty `Vec`s are serialized (project 3 reads fixed shapes); `note` skipped when `None`.

Rules the tests pin (from the spec):
- `source`: `Spec` only when both `spec` are `Read`; then `arch` of each side is used as built from the spec. Otherwise `Code`: the caller (Task 2) passes code-only `arch` for both sides; `diff` only reports it.
- Boxes matched by `id`. `changed`: a box (not stale) whose `made_of` globs match a changed path (use the glob matcher `ig-app` already uses for `made_of`); `files` sorted; then every ancestor gets an entry too, `files_changed` summed over its subtree without double counting. Added/removed boxes are not also listed as changed. Ordering: by `files_changed` desc, then id.
- Arrows matched by `(from, to, kind)`; `kind` serialized as the view serializes `ArrowKind`.
- Intent: `Same`/`Changed` when both `Read` (compare the two spec `MapFile`s by box id: added, removed, moved = same id with another `parent`, changed = names of differing fields among title, description, kind, tech, made_of → `at`, pattern, props keys (e.g. `runs`), badges, logo; arrows by `(from,to,kind)`); `Added` when base `Missing` and head `Read` (`note: "ig/ added (N boxes)"`); `Removed` when base `Read` and head `Missing`; `BaseBroken` when base `Broken(e)` (`note: "base ig/ does not read: <e>"`); both `Missing` → `Same`. A head `Broken` is the caller's error (Task 2), not a status.
- Problems, only when `source == Spec`: drift = boxes with `origin == Origin::Drift`, keyed by their sorted `made_of`, `text` = `code under <globs> is in no box`; rules = marks with `layer == "rules"` and `state == Bad`, keyed by `(badge or note, from box, to box)` of the arrow they target, `text` = the mark's badge/note. `new` = at head not base; `fixed` = at base not head.
- `summary`: parts joined by `, `, each only when non-zero, in this order: `N box(es) changed`, `N box(es) added`, `N box(es) removed`, `N new arrow(s)` (when 1: `1 new arrow <fromTitle> → <toTitle>`), `N arrow(s) removed`, intent (`ig/ unchanged` | `ig/ changed` | the note), `N new rule break(s)`, `drift +N`, `N problem(s) fixed`. Counts of boxes use top-level entries only (ancestors added by roll-up are not counted). Empty diff and intent `Same` → exactly `no architecture change`.

- [ ] **Step 1: Write the tests** in `diff/tests.rs`, building `MapFile`s by hand (look at `map/view/tests.rs` and `map/file.rs` for constructors/defaults): `same_sides_are_no_change`; `a_changed_file_marks_its_box_and_ancestors` (two children of one container each with one changed file: `files_changed` 1 on each child, 2 on the container; summary `2 boxes changed`, the container being a roll-up); `added_and_removed_boxes_and_arrows`; `one_new_arrow_names_it` (`1 new arrow Web → Db`); `ig_added_is_not_churn` (base `Missing`, head `Read`, same `arch` → boxes empty, intent `Added`, note `ig/ added (3 boxes)`); `base_ig_broken_is_noted` (`BaseBroken`, note starts `base ig/ does not read: `, `problems` empty); `intent_fields_and_moves` (a box with a new parent and a new `tech` → moved + `fields: ["tech"]`); `drift_matched_by_globs_not_id` (Review Focus 4); `new_and_fixed_rule_breaks`; `json_shape_is_camel_case` (serialize, assert keys `filesChanged`, `arrowsAdded`, `source: "spec"`).
- [ ] **Step 2: Run** `cargo test -p ig-app diff` — FAIL (module missing).
- [ ] **Step 3: Implement** `diff.rs` per Interfaces.
- [ ] **Step 4: Run** `cargo test -p ig-app` — PASS.
- [ ] **Step 5: Commit** `git add crates/ig-app/src/map/diff.rs crates/ig-app/src/map/diff crates/ig-app/src/map/mod.rs && git commit -m "The diff of two maps: boxes, arrows, ig/ and problems, matched so that the same code gives no change"`

### Task 2: building the two sides

**Files:**
- Modify: `crates/ig-app/src/map/view.rs` (split out the architecture build), `crates/ig-core/src/ports.rs` (`Vcs::changed_between`), `crates/ig-git/src/lib.rs`, `crates/ig-core/src/testing.rs` (`FakeVcs`), any other `impl Vcs` (`grep -rn "impl Vcs for"`)
- Create: `crates/ig-app/src/map/diff/build.rs` (or inside `diff.rs` if it stays under 600 lines)
- Test: `crates/ig-git/src/tests.rs`, `crates/ig-app/src/map/diff/tests.rs`

**Interfaces:**
- Consumes: Task 1's `Side`, `SpecSide`, `diff`.
- Produces:
  - `view::architecture(env: &Env, use_spec: bool) -> Result<Side>` — the `MapFile` `build` has just before `components`/`view()` (with `marks` applied), never writing; `use_spec == false` gives the code-only map (`today(None)`), with `spec` still reporting what `ig/` is (`Read`/`Missing`/`Broken`). `build` is refactored to call the shared part, behaviour unchanged (existing view tests stay green).
  - `Vcs::changed_between(&self, base: &str, head: Option<&str>) -> Result<Vec<String>, PortError>` — `git diff --name-only <base> [<head>]` relative to the ig root, plus untracked files when `head` is `None`; sorted.
  - `diff::run(env: &Env, base: &str, head: Option<&str>) -> Result<Diff>`: verifies both revs (`Vcs::verify`, error `"<rev>: not a commit"`), checks out `base` (and `head` when given) with `Vcs::checkout`, makes each checkout buildable with every frontend's `prepare_checkout(&work_info, root, &config, true)` (port of `git show 4bea9e2^:crates/ig-app/src/base.rs` `graph_at`/`work_info`, using the head's config), builds `architecture(env_at_checkout, true)` per side; if both specs are `Read` uses them, else rebuilds both with `use_spec = false`; a head `Broken` is an error (`"ig/ does not read: <e>"`); checkout root `None` (the ig root did not exist at that commit) = an empty side with `SpecSide::Missing`. Then `diff(...)` with `changed_between`.
- An `Env` for a checkout: see how `Wired::new(root).env()` builds one in `ig-wire`; `diff::run` takes a closure or the frontends list to build it — keep `ig-app` free of `ig-wire` (dependency rule in `crates/ig/tests/architecture.rs`): signature becomes `diff::run(env: &Env, base: &str, head: Option<&str>, at: &dyn Fn(&Path) -> Result<Side>) -> Result<Diff>` where `at` builds a side for a checkout root; the CLI passes `|root| view::architecture(&Wired::new(root).env(), true)` (and the code-only variant via a `use_spec` arg: `at: &dyn Fn(&Path, bool) -> Result<Side>`).

- [ ] **Step 1: Tests.** `ig-git/src/tests.rs`: `changed_between_lists_paths_between_commits_and_the_working_tree` (temp repo, two commits, an untracked file). `diff/tests.rs`: `run_uses_code_ids_when_one_side_lacks_ig` and `run_rejects_a_rev_that_is_not_a_commit` with `FakeVcs` (`commits`, `checkout_root`, a new `changed_between` field) and an `at` closure returning prepared `Side`s; `view` tests: `architecture_never_writes_and_reports_the_spec` (MemFiles without `ig/` → `Missing`, nothing written; with a broken `ig/system.kdl` → `Broken`).
- [ ] **Step 2: Run** `cargo test -p ig-git -p ig-app` — FAIL.
- [ ] **Step 3: Implement** per Interfaces.
- [ ] **Step 4: Run** `cargo test --workspace` — PASS.
- [ ] **Step 5: Commit** `git add crates/ig-core/src crates/ig-git/src crates/ig-app/src && git commit -m "Both sides of a diff: the base built in a checkout, the head from the tree, changed files from git"`

### Task 3: `ig diff` on the command line

**Files:**
- Modify: `crates/ig/src/cli.rs` (`Cmd::Diff`), `crates/ig/src/main.rs`, `README.md` (an `ig diff` section and the Commands table)
- Create: `crates/ig/src/diff.rs`, `crates/ig/tests/diff.rs`

**Interfaces:**
- `Cmd::Diff(DiffArgs)` placed after `explore` in help, one-liner `Show what a change does to the architecture`; `pub struct DiffArgs { pub base: Option<String>, #[arg(requires = "base")] pub head: Option<String>, #[arg(long)] pub json: bool }`.
- `diff::run(root: &Path, args: &DiffArgs, quiet: bool) -> Result<u8>`: folder of projects → `ig: ig diff does not read a folder of projects yet`, 2; no git → `ig: ig diff needs a git repository`, 2; base `None` → `Vcs::resolve_base(None, &mut notices)` (notices via `say`); `--json` → `serde_json::to_string_pretty(&diff)` on stdout; else text on stdout:
  ```
  <base7>..<head7 | working tree>: <summary>
    changed  <title> (<n> files)          one line per top-level changed box
    added    <title>
    removed  <title>
    arrow +  <fromTitle> → <toTitle> (<kind>)
    arrow −  <fromTitle> → <toTitle> (<kind>)
    ig/      <id>: <fields joined by ", "> | moved <from> → <to> | added | removed
    problem  <text>                        new problems
    fixed    <text>
  ```
  (titles from the side that has the box; exact spacing is the builder's.) Exit 0.

- [ ] **Step 1: Tests** in `tests/diff.rs` with real git in temp dirs (helper: `git init`, `git -c user.name=t -c user.email=t@t commit`, a tiny TS repo like `tests/plan_check.rs` uses, `ig analyse --no-agent` to make `ig/` where needed): `no_change_on_the_same_commit` (`ig diff HEAD HEAD --json` → summary `no architecture change`; Review Focus 1); `a_new_file_in_a_box_is_a_changed_box` (commit, then edit a file in the working tree → `ig diff HEAD` text starts `<sha7>..working tree: 1 box changed`); `a_new_import_is_a_new_arrow` (two packages, add an import, commit, `ig diff HEAD~1 HEAD --json` → `arrows.added` has it); `ig_added_by_the_change` (`ig/` committed only in HEAD → `intent.status == "added"`, no box churn); `head_without_base_is_a_usage_error` (exit 2); `not_a_commit_and_no_git` (exit 2, one stderr line each; Review Focus 5); `nothing_is_written` (`git status --porcelain` identical before and after, no `ig/` created, `git worktree list` has one entry after).
- [ ] **Step 2: Run** `cargo test -p intent-graph --test diff` — FAIL.
- [ ] **Step 3: Implement** per Interfaces; README: a short `ig diff` section (what it compares, defaults, `--json`, "the map page comes later" is not written — describe only what exists) and the Commands table row.
- [ ] **Step 4: Run** `cargo test --workspace` and `cargo clippy --workspace --all-targets` — PASS, no new warnings. By hand: `cargo run -q -p intent-graph -- diff 0c3dc39 1c0959a` on this repo prints a plausible summary (the CLI rebuild).
- [ ] **Step 5: Commit** `git add crates/ig/src crates/ig/tests/diff.rs README.md && git commit -m "ig diff prints what a change does to the architecture, as text or JSON"`
