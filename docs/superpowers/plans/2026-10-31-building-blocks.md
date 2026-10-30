# Building Blocks Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** ig's map speaks the agreed vocabulary (run blocks, design blocks, `group`, seven arrows), each kind defined once in a definitions file that the reader, edits, Claude and the viewer read, with team kinds that extend a built-in one.

**Architecture:** The built-in kinds stay Rust enums (`BoxKind`, `ArrowKind` in `crates/ig-app/src/map/`), with old names as serde aliases. One embedded file, `crates/ig-app/assets/kinds.kdl`, defines every built-in kind (meaning, icon, role, properties) and the arrow rules; `map/kinds.rs` loads it plus a team's `ig/kinds/*.kdl` into a `Kinds` table. Placement follows the kind's **role** (run, design, group), replacing the fixed system/container/component levels in validation; the layout's `Level` is derived from the nearest run block.

**Tech Stack:** Rust (`kdl` =6.5.0, serde), Svelte 5 + Vitest (viewer), bun.

**Spec:** `docs/superpowers/specs/2026-10-30-building-blocks-design.md`

## Global Constraints

- Every `ig/` written today reads without edits; old names are aliases, rewritten only by `ig fmt` (comments and order kept).
- Old kind names → new: `api`, `worker` → `service`; `library` → `module`; `folder` → `group`; `component` (in `ig/`) → `module`; `app` stays `app`.
- Old arrow names → new: `depends` → `uses`; `network` → `calls`; `event` → `sends`; `data` → `reads`. `bundle` stays derived, never written.
- New built-in kinds: `service`, `job`, `model`, `agent`, `adapter`, `module`; `entry` takes `kind` = `route` | `command` | `schedule` | `message`; `model` takes `provider`, `model`, `hosted` (`local` | `hosted`); `agent` takes `prompt`; every kind may take `owner`.
- A team kind: `ig/kinds/<name>.kdl`, `kind "<name>" is="<built-in>"`; may add properties, title, meaning, icon; may not add arrow kinds, change placement, shadow a built-in name, or extend a code-level kind.
- `runs` is refused with exactly: ``"`runs` comes with live status; not yet"``.
- Arrow-rule mismatches and more than 12 children are **warnings** (printed by `ig check`/`ig plan`, shown as map notices), never read errors, and never change `ig check`'s exit code.
- Live-only kinds (`file`, `function`, `class`, `method`, `parts`, Ansible `task`, and now the code-derived `folder` and `component`) never enter `ig/`.
- No list of box kinds anywhere except `kinds.kdl` and the `BoxKind` enum; a test ties the two together. The MCP schema, the agent prompt's list and the viewer's lists come from the table.
- Viewer: colours in `viewer/src/styles/themes/*.css`, sizes from tokens, rem not px. **No viewer change that shows (Task 6) before its canvas sketch is approved by the user.**
- Never `git push`. No Claude attribution in commits.

## Review Focus

1. A real map from before this work (`api`, `worker`, `library`, `folder`, `component`, `depends`, `network`, `event`, `data`) reads to the same tree with new kinds, and `ig check`'s exit code is unchanged: fixture test in Task 3.
2. A team kind file with a KDL syntax error, an unknown `is=`, `is=` a code-level kind, or a name shadowing a built-in: one clear error naming the file and line, the rest of `ig/` still reported: Task 2 tests.
3. A group inside a service holding a `service` (a run block in an inner group) is refused with the placement message, while a group in a group in the system holding services reads: Task 3 tests.
4. `ig fmt` on a file using alias names keeps its comments and the order of children: Task 5 test.
5. `set kind` to a kind that may not sit where the box is (e.g. a `usecase` at the top) is refused and changes no file: Task 5 test.

---

### Task 1: The vocabulary in the enums, with the old names as aliases

**Files:**
- Modify: `crates/ig-app/src/map/facts.rs` (`BoxKind`, `code_level`, new `role`), `crates/ig-app/src/map/arrows.rs` (`ArrowKind`, `TELLING`, `NOUNS`, `edge_kind`)
- Modify every Rust use of removed variants: `map/ansible.rs`, `map/facts.rs`, `map/code.rs`, `map/layers.rs`, `map/layout.rs`, `map/origin.rs` and their tests (find with `grep -rn "BoxKind::\(Api\|Worker\|Library\)\|ArrowKind::\(Depends\|Network\|Event\|Data\)" crates`)
- Modify: `viewer/src/map/types.ts`, `viewer/src/map/edit/add.ts`, `viewer/src/map/edit/arrows.ts`, viewer tests that name old kinds (names only: nothing that shows changes)
- Test: `crates/ig-app/src/map/facts/tests.rs`, `crates/ig-app/src/map/arrows.rs` tests

**Interfaces:**
- Produces: `BoxKind` variants `Person, System, External, App, Service, Job, Store, Queue, Model, Module, Entry, Usecase, Port, Adapter, Agent, Domain, Group` plus live-only `Folder, File, Component, Parts, Task, Function, Class, Method`; serde aliases `api`/`worker` → `Service`, `library` → `Module`.
- Produces: `pub enum Role { Root, Run, Design, Group, Code }` and `BoxKind::role(self) -> Role` (`System` → Root; `Group` → Group; Service/Job/App/Store/Queue/Model/External/Person → Run; Entry/Usecase/Domain/Port/Adapter/Agent/Module → Design; live-only → Code). `code_level()` now also returns true for `Folder` and `Component`.
- Produces: `ArrowKind` variants `Calls, Sends, Reads, Writes, Uses, Implements, Triggers, Bundle`; serde aliases `depends` → `Uses`, `network` → `Calls`, `event` → `Sends`, `data` → `Reads`.

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn old_kind_and_arrow_names_read_as_the_new_ones() {
    let k = |s: &str| serde_json::from_value::<BoxKind>(serde_json::json!(s)).unwrap();
    assert_eq!((k("api"), k("worker"), k("library")), (BoxKind::Service, BoxKind::Service, BoxKind::Module));
    assert_eq!(serde_json::to_value(BoxKind::Service).unwrap(), "service");
    let a = |s: &str| serde_json::from_value::<ArrowKind>(serde_json::json!(s)).unwrap();
    assert_eq!([a("depends"), a("network"), a("event"), a("data")],
               [ArrowKind::Uses, ArrowKind::Calls, ArrowKind::Sends, ArrowKind::Reads]);
}

#[test]
fn every_kind_has_a_role() {
    assert_eq!(BoxKind::Model.role(), Role::Run);
    assert_eq!(BoxKind::Agent.role(), Role::Design);
    assert!(BoxKind::Folder.code_level() && BoxKind::Component.code_level());
}
```

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test -p ig-app old_kind_and_arrow_names every_kind_has_a_role`
Expected: FAIL to compile (`Service`, `Role`, `Sends` not found).

- [ ] **Step 3: Change the enums and every use**

Code-derived maps produce `Service` where they produced `Api`/`Worker`, `Module` where they produced `Library`; `edge_kind` maps `imports` → `Uses`, `http` → `Calls`, `uses` into a store/queue → `Reads`, into an external → `Calls`. `layout.rs`'s system rank puts `Model` in the row with `External`, `Store`, `Queue`. `NOUNS` gains `Sends` "message", `Reads` "read", `Writes` "write", `Triggers` "trigger"; `TELLING` order: Calls, Sends, Reads, Writes, Triggers, Implements, Uses. Viewer `types.ts` unions and `add.ts`/`arrows.ts` lists use the new names (the lists are replaced from the table in Task 6).

- [ ] **Step 4: Run everything**

Run: `cargo test --workspace && (cd viewer && bun run test && bunx svelte-check)`
Expected: PASS (update expected strings in existing tests where they name `depends`, `api`, … : the meaning, not the assertion, stays).

- [ ] **Step 5: Commit**

```bash
git commit -am "Vocabulary: service, job, model, agent, adapter, module; sends, reads, writes, triggers; old names read as aliases"
```

---

### Task 2: Definitions in a file, and team kinds

**Files:**
- Create: `crates/ig-app/assets/kinds.kdl`, `crates/ig-app/src/map/kinds.rs`, `crates/ig-app/src/map/kinds/tests.rs`
- Modify: `crates/ig-app/src/map/mod.rs` (`pub mod kinds;`)

**Interfaces:**
- Consumes: `BoxKind`, `Role`, `ArrowKind` (Task 1).
- Produces:

```rust
pub struct PropDef { pub name: String, pub means: String, pub required: bool, pub one_of: Vec<String> }
pub struct KindDef {
    pub name: String, pub base: BoxKind, pub custom: bool,
    pub title: String, pub means: String, pub icon: String,
    pub props: Vec<PropDef>,                 // its own, plus its base's for a team kind
}
pub struct ArrowRule { pub kind: ArrowKind, pub from: Vec<String>, pub to: Vec<String> } // kind names; empty = any
pub struct Kinds { /* name -> KindDef, arrow rules */ }
impl Kinds {
    pub fn builtin() -> &'static Kinds;                                   // parsed once from kinds.kdl
    pub fn load(files: &dyn ig_core::ports::Files) -> Result<Kinds, Vec<String>>; // builtin + ig/kinds/*.kdl
    pub fn get(&self, name: &str) -> Option<&KindDef>;
    pub fn names(&self) -> Vec<&str>;                                     // spec kinds, built-in first
    pub fn common_props() -> &'static [&'static str];                     // title description tech at owner logo slot badges pattern
    pub fn arrow_ok(&self, kind: ArrowKind, from: &KindDef, to: &KindDef) -> bool;
}
```

`kinds.kdl` shape (one node per built-in kind, then the arrow rules); placement is **not** in the file, it follows `base.role()` (Task 3):

```kdl
kind "model" title="AI model" icon="sparkles" means="An AI model the system calls, hosted or local" {
    property "provider" means="Who serves it: anthropic, openai, ollama…"
    property "model" means="Its name, like claude-opus-5-5 or llama3.2"
    property "hosted" means="Where it runs" one-of="local" "hosted"
}
arrow "reads" to="store"
arrow "writes" to="store"
arrow "sends" to="queue"
arrow "implements" from="adapter" to="port"
arrow "triggers" to="job" "entry"
```

- [ ] **Step 1: Write the failing tests** (`map/kinds/tests.rs`)

```rust
#[test]
fn every_spec_kind_is_defined_exactly_once() {
    let k = Kinds::builtin();
    for b in ALL_BOX_KINDS.iter().filter(|b| !b.code_level()) {   // a const array in kinds.rs's test module
        let name = serde_json::to_value(b).unwrap();
        assert_eq!(k.names().iter().filter(|n| **n == name).count(), 1, "{name}");
    }
    assert!(k.names().iter().all(|n| k.get(n).unwrap().base.role() != Role::Code));
}

#[test]
fn a_team_kind_extends_a_built_in_one() {
    let files = mem(&[("ig/kinds/iot-device.kdl",
        "kind \"iot-device\" is=\"service\" title=\"IoT device\" { property \"firmware\" required=#true }")]);
    let k = Kinds::load(&files).unwrap();
    let d = k.get("iot-device").unwrap();
    assert_eq!((d.base, d.custom), (BoxKind::Service, true));
    assert!(d.props.iter().any(|p| p.name == "firmware" && p.required));
}

#[test]
fn bad_team_kinds_name_their_file() {
    for (text, says) in [
        ("kind \"x\" is=\"nope\"", "`nope` is not a built-in kind"),
        ("kind \"x\" is=\"function\"", "`function` is a code-level kind"),
        ("kind \"service\" is=\"service\"", "`service` is a built-in kind"),
        ("kind \"x\" is=\"service\" {", "ig/kinds/x.kdl:1"),
        ("kind \"x\" is=\"service\" { arrow \"pings\" }", "a kind cannot add arrows"),
    ] {
        let e = Kinds::load(&mem(&[("ig/kinds/x.kdl", text)])).unwrap_err().join("\n");
        assert!(e.contains("ig/kinds/x.kdl") && e.contains(says), "{text}: {e}");
    }
}
```

(`mem` is the in-memory `Files` the spec tests already use; reuse it.)

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test -p ig-app kinds::`
Expected: FAIL to compile (`Kinds` not found).

- [ ] **Step 3: Write `kinds.kdl` (all spec kinds of Task 1 with title, means, icon, the properties from Global Constraints) and `kinds.rs`**

Parse with the `kdl` crate (as `spec/read.rs` does); collect every error before returning. `builtin()` uses `std::sync::OnceLock`; a parse failure of the embedded file is a panic caught by the test above.

- [ ] **Step 4: Run them**

Run: `cargo test -p ig-app kinds::`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/ig-app/assets/kinds.kdl crates/ig-app/src/map/kinds.rs crates/ig-app/src/map/kinds crates/ig-app/src/map/mod.rs
git commit -m "Kinds: every built-in kind defined in kinds.kdl; team kinds in ig/kinds extend one"
```

---

### Task 3: Reading and validating `ig/` with the table

**Files:**
- Modify: `crates/ig-app/src/map/spec/read.rs` (kind lookup, levels, properties, team kinds), `crates/ig-app/src/map/spec.rs` (write custom name and `props`), `crates/ig-app/src/map/file.rs` (`MapBox` fields; `validate_spec` placement; drop the 12-children error)
- Test: `crates/ig-app/src/map/spec/tests.rs`

**Interfaces:**
- Consumes: `Kinds::load`, `KindDef`, `Role` (Task 2).
- Produces: `MapBox.custom: Option<String>` (serde skip if none), `MapBox.props: BTreeMap<String, String>` (kind properties and `owner`; serde skip if empty); `pub fn may_sit(child: Role, parent: Role, parent_outer: bool) -> bool` in `kinds.rs`; `spec::read` takes the team kinds from `ig/kinds/` itself.

Placement (`may_sit`), from the spec's table: Run → parent Root, or Group that is outer. Design → parent Run, Design, or Group that is inner. Group → anywhere; it is outer when no Run ancestor, inner when one. `Level` per box: the system → `System`; a box with a Run block among its ancestors → `Component`; any other box (a Run block or an outer Group) → `Container`.

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn a_map_from_before_reads_with_the_new_kinds() {
    // fixture: system with `api "api"` (calls → `worker`), `library "lib"`, `folder "f"` holding
    // `component "c"`, and arrows depends/network/event/data
    let m = parse(&OLD_MAP).unwrap();
    let kind = |id: &str| m.boxes.iter().find(|b| b.id == id).unwrap().kind.value;
    assert_eq!([kind("api"), kind("worker"), kind("lib"), kind("f"), kind("f/c")],
               [BoxKind::Service, BoxKind::Service, BoxKind::Module, BoxKind::Group, BoxKind::Module]);
    assert!(m.arrows.iter().all(|a| matches!(a.kind, ArrowKind::Uses | ArrowKind::Calls | ArrowKind::Sends | ArrowKind::Reads)));
}

#[test]
fn groups_hold_what_may_sit_where_they_sit() {
    assert!(parse(&sys("group \"g\" title=\"G\" { group \"h\" title=\"H\" { service \"s\" title=\"S\" } }")).is_ok());
    let e = parse(&sys("service \"s\" title=\"S\" { group \"g\" title=\"G\" { service \"t\" title=\"T\" } }")).unwrap_err();
    assert!(e.join("\n").contains("`t`: a service sits in the system or a group outside any running block"), "{e:?}");
    let m = parse(&sys("group \"g\" title=\"G\" { service \"s\" title=\"S\" { usecase \"s/u\" title=\"U\" } }")).unwrap();
    let lvl = |id: &str| m.boxes.iter().find(|b| b.id == id).unwrap().level;
    assert_eq!([lvl("g"), lvl("s"), lvl("s/u")], [Level::Container, Level::Container, Level::Component]);
}

#[test]
fn properties_follow_the_kind() {
    let m = parse(&sys("model \"llama\" title=\"Llama\" provider=\"ollama\" hosted=\"local\" owner=\"me\"")).unwrap();
    let b = m.boxes.iter().find(|b| b.id == "llama").unwrap();
    assert_eq!(b.props.get("provider").map(String::as_str), Some("ollama"));
    for (text, says) in [
        ("model \"m\" title=\"M\" hosted=\"cloud\"", "`hosted` is one of local, hosted"),
        ("service \"s\" title=\"S\" provider=\"x\"", "unknown property `provider`"),
        ("service \"s\" title=\"S\" runs=\"compose:s\"", "`runs` comes with live status; not yet"),
    ] {
        assert!(parse(&sys(text)).unwrap_err().join("\n").contains(says), "{text}");
    }
}

#[test]
fn thirteen_children_read() {
    let kids: String = (0..13).map(|i| format!("service \"s{i}\" title=\"S\"\n")).collect();
    assert!(parse(&sys(&kids)).is_ok());
}

#[test]
fn a_team_kind_reads_and_writes_by_its_own_name() {
    // files: ig/kinds/iot-device.kdl (is="service", property firmware required) and a system with
    // `iot-device "sensor" title="Sensor" firmware="1.2"`: base Service, custom Some("iot-device"),
    // and `spec::write` gives back `iot-device "sensor" … firmware="1.2"`. Without firmware: error
    // "`sensor`: `firmware` is required". Without the kind file: "`iot-device` is not a kind (no ig/kinds/iot-device.kdl)".
}
```

(`sys(s)` wraps `s` in `system "system" title="T" { … }`; `OLD_MAP` is a const in the test file.)

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test -p ig-app spec::tests`
Expected: FAIL (aliases unknown to the KDL reader, placement still by level, no `props`).

- [ ] **Step 3: Implement**

The reader resolves a node name through the team `Kinds` (aliases `folder` → `group`, `component` → `module` in the reader; the rest via serde); unknown properties are checked against `Kinds::common_props()` and the kind's `props`. `validate_spec` checks placement with `may_sit` and no longer errors on more than 12 children. `spec::write` writes `custom` as the node name when set, then `props` in name order after the common ones.

- [ ] **Step 4: Run everything**

Run: `cargo test --workspace`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git commit -am "ig/ reads by the kinds table: placement by role, groups at any level, kind properties, team kinds"
```

---

### Task 4: Warnings: arrow rules and crowded boxes

**Files:**
- Create: `crates/ig-app/src/map/warn.rs` (+ tests module)
- Modify: `crates/ig/src/main.rs` (`plan`/`check` print warnings), `crates/ig-app/src/map/view.rs` (warnings join `notices`)

**Interfaces:**
- Consumes: `Kinds`, `KindDef`, `MapFile` (Tasks 2–3).
- Produces: `pub fn warnings(m: &MapFile, kinds: &Kinds) -> Vec<String>`.

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn mismatched_arrows_and_crowded_boxes_warn() {
    // a service `reads` an external; a group with 13 children
    let w = warnings(&m, Kinds::builtin());
    assert!(w.contains(&"`api` reads `gmail`: reads goes to a store".to_string()), "{w:?}");
    assert!(w.contains(&"`g` holds 13 boxes; more than 12 is hard to read".to_string()), "{w:?}");
}
```

Plus in `crates/ig/tests/` (the CLI tests): `ig check` on a map whose only finding is a warning prints `warning: …` on stderr and exits 0.

- [ ] **Step 2: Run, see it fail** — `cargo test -p ig-app warn::` → FAIL (no `warnings`).
- [ ] **Step 3: Implement `warnings`** with `Kinds::arrow_ok`; `main.rs` prints each as `warning: {w}` to stderr for `plan` and `check` without touching the exit code; `view` appends them to `notices`.
- [ ] **Step 4: Run** — `cargo test --workspace` → PASS.
- [ ] **Step 5: Commit** — `git commit -am "Warnings for arrows the kinds do not expect and boxes with more than 12 children"`

---

### Task 5: Changing a kind, `ig fmt` to the new names, generated lists for Claude

**Files:**
- Modify: `crates/ig-app/src/map/edit.rs` (`set` with `key == "kind"`), `crates/ig-app/src/map/edit/fmt.rs` (canonical names), `crates/ig/src/mcp/schema.rs` (`KINDS`, `ARROWS` from the table), `crates/ig/src/agent.rs` (the annotation `kind` enum from the table), `crates/ig/assets/prompts/read.md` (the kind list as a `{kinds}` placeholder filled from the table; an instruction to propose `set kind service` for an `app` no person uses that serves routes)
- Test: `crates/ig-app/src/map/edit/tests.rs`, `crates/ig-app/src/map/edit/fmt.rs` tests, `crates/ig/src/mcp/tests.rs`

**Interfaces:**
- Consumes: `Kinds`, `may_sit` (Tasks 2–3).
- Produces: `Edit::Set { id, key: "kind", value: Some(name) }` accepted by `ig_edit` and the desktop's `edit_apply`.

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn set_kind_renames_the_node_in_place() {
    // ig/system.kdl: `app "api" title="API" { // serves the shop\n usecase "api/x" title="X" }`
    // apply Set { id: "api", key: "kind", value: Some("service") }
    // → the file reads `service "api" title="API" {`, the comment and the child are unchanged
}

#[test]
fn set_kind_refuses_a_kind_that_may_not_sit_there() {
    // Set { id: "api", key: "kind", value: Some("usecase") } on a box in the system
    // → Err containing "a usecase sits inside a running block"; no file changes
}

#[test]
fn fmt_writes_the_new_names_and_keeps_comments() {
    // `api "a" title="A" { // why\n depends "b" }` → fmt → `service "a" title="A" { // why\n uses "b" }`
}

#[test]
fn the_tool_schema_lists_the_kinds_of_the_table() {
    let s = schema();
    let kinds = /* the enum of ig_edit's `kind` in s */;
    assert!(kinds.contains(&json!("model")) && kinds.contains(&json!("agent")) && !kinds.contains(&json!("api")));
}
```

- [ ] **Step 2: Run, see them fail** — `cargo test --workspace set_kind fmt_writes the_tool_schema` → FAIL.
- [ ] **Step 3: Implement.** `set kind` validates against `Kinds` and `may_sit` before touching the document, then renames the node (`KdlNode::set_name`) keeping entries and children. `fmt` renames alias node names (kinds and arrows) to the canonical ones. Schema and prompt lists come from `Kinds::builtin().names()` and the `ArrowKind` variants minus `Bundle`.
- [ ] **Step 4: Run** — `cargo test --workspace && cargo clippy --workspace --all-targets` → PASS, no warnings.
- [ ] **Step 5: Commit** — `git commit -am "set kind; ig fmt writes the new names; Claude's kind lists come from the table"`

---

### Task 6: The viewer speaks the table (after the canvas)

**Gate:** sketch on the canvas "ig map: details panel", in a new row "Building blocks": the shape of each run block (`store` cylinder, `queue` pipe, `service` box, `app` window / terminal for a CLI, `job`, `model`, `agent`, `external` dashed, `person`, `group` dotted frame), the add-box form grouped "Runs" / "Inside" with each kind's meaning and icon, and the crowded-box warning. **Wait for the user's approval of the sketch before Step 1.**

**Files:**
- Modify: `crates/ig-app/src/map/view.rs` (`View.kinds: Vec<KindView>` with `name, base, title, means, icon, role, props`), `viewer/src/map/types.ts`, `viewer/src/map/edit/add.ts` and `AddBox.svelte` (from `view.kinds`), `viewer/src/map/edit/arrows.ts` (seven arrows), `viewer/src/map/MapBox.svelte` (shape per base kind, icons from `icon`), `viewer/src/map/looks.ts`, theme files for any new colour
- Test: `viewer/test/map-edit.test.ts`, `viewer/test/map-canvas.test.ts`

- [ ] **Step 1: Write the failing tests**

```ts
test("the add form offers the kinds of the view, grouped, with their meaning", async () => {
  // view.kinds includes a team kind "iot-device" (base service): the form's "Runs" group lists it
  // with its means text; "Inside" lists usecase, port, adapter, agent…; no `api`, no `folder`
});
test("a store is drawn as a cylinder and a group as a frame", () => {
  // box(target, "db").dataset.shape === "store"; box(target, "g").dataset.shape === "group"
});
```

- [ ] **Step 2: Run, see them fail** — `cd viewer && bun run test` → FAIL.
- [ ] **Step 3: Implement as sketched.** Shape by `base`, colour by status as today; a team kind uses its base's shape and its own icon.
- [ ] **Step 4: Run** — `cd viewer && bun run test && bunx svelte-check && cd .. && cargo test --workspace` → PASS.
- [ ] **Step 5: Commit** — `git commit -am "Map: each kind drawn as what it is; the add form from the kinds table"`
