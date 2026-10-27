# System Map (goal 1) Implementation Plan

**Goal:** Strip ig down to what the map needs, then make `ig view` robust. It builds the complete
map file (ig's facts plus an AI agent's annotations) and opens a walkable canvas: containers →
components → code, smooth zoom, and the rules, risks and file-based health layers.

**Architecture:**
- **ig-app's `map` module** computes the facts (boxes, arrows, calls in loops) and merges them
  with the annotations in `.ig/map.json` (refresh keeps annotations and pins).
- **An agent-agnostic runner** (claude, gemini or codex, headless) annotates the boxes that need
  reading; ig validates every answer.
- **At render time,** ig adds the code level, the layers, the layout and the routes, and embeds one
  `View` JSON in the single-file HTML.
- **The viewer is plain DOM/SVG** in one transformed "world". A box's inside is rendered, scaled,
  inside the box once it is big enough on screen. Zoom is a CSS-transform camera.

**Tech Stack:** Rust 2024 (ig-core, ig-lang-ts, ig-app, ig, ig-desktop/Tauri 2), Svelte 5 + TS +
Vite (bun), vitest + happy-dom, simple-icons (already a dependency).

**Spec:** `docs/specs/2026-10-27-ig-system-map-design.md`. Mockups:
the design canvas. Screen 4 and `Box.dc.html` are the visual
reference, including colors, sizes and fonts.

## Scope of this iteration: a robust `ig view`

The user re-scoped twice on 2026-10-27/28. **The canvas is the engine, and the first deliverable is
a robust, working `ig view`.** It is not the whole product: the app, the chat and the terminal
commands come later, on top of the same map file.

- **`.ig/map.json` is the complete map**, committed to the repo, with three writers:
  - **ig's analyser** writes the facts: which boxes exist, what each is made of, the arrows with
    their kinds and counts, outside services, calls in loops, and rule and risk marks;
  - **an AI agent** (Claude Code, Gemini CLI or Codex, run headless) writes the annotations:
    level and kind, pattern and slot, title, tech, description, logo, badges;
  - **a person** may edit any annotation; a field they pin is never overwritten.
  Each annotated field records its source (`ig`, `agent` or `person`). Positions are not stored;
  they are computed when drawing. Live health stays in `.ig/layers/*.json`.
- **`ig view`:**
  1. refreshes the facts and keeps the annotations;
  2. if boxes need reading, asks once in a terminal: *Let an AI agent read the architecture?
     [Y/n]*;
  3. writes the HTML;
  4. opens it.
  `--read` re-reads without asking, `--no-read` never asks, `--agent <name>` picks the agent.
- **Tasks:** 1 and 2 (strip), then 3–10 below. The old Tasks 8 and 11 and the app-only parts of
  the canvas are deferred. The desktop app keeps compiling but gets no new work.

## Global Constraints

- **Branch:** work on a new branch `feat/system-map` made from `feat/design-boards` (HEAD
  `1086f62`). Never push. No Claude attribution in commits.
- **No new runtime dependencies.** Keep simple-icons, mermaid, markdown-it, dompurify, svelte and
  @tauri-apps/api. Any dependency added must have an exact version.
- **The map never draws more than 12 children inside one box.** More than that is grouped (by
  folder, then by name prefix) into sub-boxes.
- **Box ids:**
  - container: `^[a-z0-9][a-z0-9-]{0,63}$`, equal to its production `service.name`;
  - component: `<container>/<slug>`;
  - code: the graph node id.
- **Healthy looks like nothing.** Only problems get color. Look precedence: health > Claude's
  highlight > branch status > kind.
- **Colors** (from the spec): entry `#3b6fd8`, use case `#2f8f5b`, port `#2f8f5b` dashed, domain
  `#b8860b`; slow and risk `#d08a00`; failing and rule `#c0392b`; down `#8a8378` dashed; network
  `#3b6fd8` dashed; event `#7b4fc9` dotted; data `#1f7a8c`; depends `#8a8378`; calls `#3a3631`.
- **Zoom glide:** 800 ms, `cubic-bezier(0.2, 0.7, 0.2, 1)`. Detail by on-screen box width: far
  < 140 px ≤ mid < 220 px ≤ near. A box's inside fades in once the box is ≥ 560 px wide on screen.
- **Fonts:** Kalam for box titles, Inter for UI text, `ui-monospace` for code-level titles and
  labels.
- **Read-only repos:** the user's repos under `~/code/shop` and `~/code/intake` are never
  written. Use scratch copies only.
- **No synthetic OS input of any kind.** Screenshots are of the app window only
  (`screencapture -l`).

## Review Focus

1. **A single-package repo (not a monorepo)** shows one container with its components, never an
   empty map. Tested in Task 3 (`single_package_repo`).
2. **The code changed since the last reading** (a folder renamed, a package added): annotations
   of boxes that still exist survive, new boxes are marked `needsReading`, gone ones are dropped,
   pinned ones are flagged instead of lost. Tested in Task 4 (`refresh_keeps_annotations`).
3. **A level with more than 12 children** is grouped; never more than 12 are drawn. Tested in
   Task 3 (`groups_past_twelve`).
4. **A malformed or hostile map file or agent output** (oversized, bad ids, unknown kinds, `..`
   globs, prose around the JSON) is rejected with a clear message, and the previous map stays.
   Tested in Task 4 (`rejects_hostile_annotations`) and Task 6 (`agent_garbage_keeps_map`).
5. **No agent installed, or the agent fails or times out:** `ig view` still writes and opens the
   map from ig's own reading and says why. Tested in Task 6 (`no_agent_still_views`).

---

### Task 1: Strip the Rust side to what the map needs

**Files:**
- Delete crates: `crates/ig-board`, `crates/ig-design`, `crates/ig-checkers`.
- Delete in ig-core: `design/`, `diff.rs`, `linediff.rs`, `intents.rs`, `notes.rs`, `plan/`,
  `review/`, `rules/{baseline,complexity,intents}.rs`. Also drop `CheckerRunner` and
  `CheckerOutput` from `ports.rs`.
- Delete in ig-app: `apply/`, `base.rs`, `baseline.rs`, `brief.rs`, `check/`, `design.rs`,
  `notes.rs`, `plan.rs`, `query.rs`, `review.rs`, `screens.rs`. Fold `lanes::system_category`
  into `build.rs` and delete `lanes.rs`. Keep `view_data.rs` until Task 6.
- Delete in ig: `report.rs`, `hook.rs`, `serve.rs`, `serve/`, `mcp/design.rs`, the tests for
  removed commands under `src/tests/`, and `assets/{agents,legacy}`. From `assets/skills`, delete
  `plan.md` and `apply.md`. Task 8 writes a new `setup.md`.
- Delete in ig-desktop: `design.rs`, `screens.rs`, `unsaved.rs`, `file_tree.rs`, `read_file.rs`,
  and their commands.
- Modify: `crates/ig/src/{main.rs,cli.rs,mcp.rs,init.rs,skills.rs}`, `crates/ig-core/src/config.rs`,
  `crates/ig-wire/src/lib.rs`, `Makefile`, all `Cargo.toml` files.
- **Keep:** ig-core (`model`, `workspace`, `config`, `cache`, `http`, `ports`, `rules/{arch,layers}`,
  `testing`), ig-lang-ts, ig-fs, ig-git, ig-wire, ig-app (`build`, `config`, `setup`, `show`,
  `view_data`, `error`), ig (`view`, `mcp`, `init`, `plugin`), and ig-desktop (agent, session,
  trust, permissions, plugin, events, exit, shell_path, bounded).

**Interfaces:**
- **Produces:**
  - CLI commands: `ig view`, `ig mcp`, `ig init`, `ig plugin export <dir>`. Task 11 adds `ig`, `ig <folder>`, `ig setup`, `ig map` and `ig check`.
  - MCP tools: `ig_state` and `ig_show` (unchanged).
  - `Config` keeps: `layers`, `rules`, `ignore`, `tests`, `embedded`, `auto_imports`, `patterns`,
    `blueprint.systems`, `blueprint.http_calls`, `blueprint.http_prefix`, `blueprint.route_kind`.
  - Config keys that were removed are ignored with a notice, never an error, so old `ig.yaml` files
    still load.

- [ ] **Step 1: Branch.** `git switch -c feat/system-map`.
- [ ] **Step 2: Write the failing test** `config::tests::old_keys_are_ignored_with_notice`. Parse a
  YAML with `complexityMax: 10`, `checkers: []` and `review: {}`. Assert `Config::parse` is `Ok`
  and its notices contain `"complexityMax"`.
- [ ] **Step 3: Delete, then fix the build until it compiles.**
  - Remove dead `match` arms, MCP tools and Tauri commands.
  - Makefile: drop `board-wasm` and `board-test`.
  - Remove `ig_wire` behavior for design, screens and plans. Keep graph rebuild on source change
    and the `.ig/show.json` passthrough.
- [ ] **Step 4: Verify.**
  - Run: `cargo build --workspace && cargo test --workspace && cargo clippy --workspace -- -D warnings`.
  - Expected: all green.
  - Golden outputs in `crates/ig/tests/golden` change only where removed fields disappear; update
    them with the existing golden-update mechanism and note it in the commit.
- [ ] **Step 5: Commit** `Strip to the map: remove plans, apply, review, design boards, checkers`.

### Task 2: Strip the viewer to the app shell and chat

**Files:**
- **Delete:**
  - `viewer/src/board/`;
  - the components Graph, Tree, Files, CodeView, Overlay, PlanPanel, SidePanel, ShowBanner, shared,
    CopyButton (keep it if chat uses it);
  - the models architecture, blueprint, boardselect, callchain, changed, code, design, focus,
    graph, notes, tests and tree;
  - their tests in `viewer/test/`.
- **Modify:**
  - `viewer/src/components/App.svelte`: header plus an empty `#map` placeholder plus the chat
    aside (app only);
  - `viewer/package.json`: remove cytoscape, cytoscape-fcose, @types/cytoscape and all
    `@codemirror/*` (unless chat imports them);
  - `viewer/vite.config.ts`: drop `__BOARDS__` and the `noBoards()` guard;
  - `viewer/src/vendor.d.ts`;
  - `viewer/src/api/*`: drop the removed host methods.
- **Keep:** `chat/*`, `Trust.svelte`, `api/*`, `model/{chat,coalesce,markdown,show}.ts`,
  `icons.ts`, `styles/*`, `data.ts` (reduced to what is still used).

**Interfaces:**
- **Produces:** `App.svelte` renders `<header>`, `<div id="map">`, and `<aside id="chat">` when
  `chatApi`. Task 10 mounts the map into `#map`.

- [ ] **Step 1: Delete and fix imports.**
- [ ] **Step 2: Verify.**
  - Run: `cd viewer && bun install && bun run typecheck && bun run test && bun run build && bun run build:app`.
  - Expected: all green. `dist/index.html` has no cytoscape (`grep -c cytoscape dist/index.html` →
    0).
- [ ] **Step 3: Verify the app builds.** `make app-dev` starts and the chat still opens a session
  on a scratch project.
- [ ] **Step 4: Commit** `Strip the viewer to the shell and chat`.

### Task 3: The facts: boxes, arrows, calls in loops

**Files:**
- Create: `crates/ig-app/src/map/{mod.rs,facts.rs,arrows.rs}`.
- Modify: `crates/ig-lang-ts/src/extract/{visitor.rs,visit_impl.rs,mod.rs}`,
  `crates/ig-lang-ts/src/resolve.rs`, `crates/ig-core/src/model.rs`.
- Create: `crates/ig/tests/fixtures/shop/`. This is the shop fixture of ruling R1, with no
  `.ig/map.json`; Task 4 adds one in its tests.

**Interfaces:**
- **Consumes:** `Workspace`, `Graph`, `Config` (`blueprint.systems`).
- **Produces:**
  - `Edge.in_loop: bool` (JSON `inLoop`, skipped when false). It is true when the call is inside a
    `for`, `for…in`, `for…of`, `while` or `do` body, or inside a callback passed to `.map`,
    `.forEach`, `.filter`, `.reduce`, `.flatMap`, `.some` or `.every`. A `loop_depth` counter in
    the visitor sets it, `add_ref` stamps it, and `RawRef.in_loop` carries it.
  - `pub enum Level { System, Container, Component, Code }`.
  - `pub enum BoxKind { Person, System, External, App, Api, Worker, Store, Queue, Library, Entry, Usecase, Port, Domain, Function, Class, Group }`.
  - `pub struct FactBox { id: String, parent: Option<String>, level: Level, made_of: Vec<String> /*globs*/, package: Option<String>, files: u32, nodes: Vec<String> /*graph node ids*/, guess: Guess }`.
  - `pub struct Guess { kind: BoxKind, title: String, pattern: Option<Pattern>, slot: Option<String> }`
    is ig's own reading. The `Pattern` enum is the same as before: Onion | Layered{bands} |
    Hexagonal | None.
  - `pub fn facts(ws: &Workspace, g: &Graph, cfg: &Config) -> Vec<FactBox>`:
    - **system:** the repo;
    - **containers:** one per workspace package that has source (root only when it is the sole
      package), id = the package name slugified with its scope dropped, guessed kind `App` if it
      has pages or a framework entry, otherwise `Library`; plus one per `blueprint.systems`
      entry (`External`, or `Store` when its category is database);
    - **components:** the container's source folders at the depth where the pattern shows, with
      the folder-name detection of ruling R7 giving `pattern` and `slot`; ≤ 12 per container,
      grouped past that;
    - **code:** level 4 is not in `facts`; it is computed at render time (Task 5).
  - `pub struct MapArrow { id: String /*"{from}->{to}"*/, parent: String, from: String, to: String, kind: ArrowKind, label: Option<String>, count: u32, kinds: Vec<ArrowKind>, in_loop: bool }`.
  - `pub enum ArrowKind { Uses, Depends, Calls, Network, Event, Implements, Data, Bundle }`.
  - `pub fn arrows(boxes: &[FactBox], g: &Graph) -> Vec<MapArrow>`: each graph edge is lifted to the
    children of its ends' lowest common parent.
    - Kind mapping:
      - imports → Depends;
      - calls, renders, dispatch → Calls;
      - implements and extends → Implements;
      - http → Network;
      - uses to a Store → Data, to an External → Network.
    - More than one kind between the same pair → Bundle, with a label like `"3 calls · 1 import"`
      (plural only when n > 1; order: calls, requests, imports).
    - Label of a Network arrow: the route when there is one route, otherwise none. Label of a
      Calls arrow: the callee when there is one.

- [ ] **Step 1: Write the failing tests:**
  - `extract::tests::marks_calls_in_loops`: `for (const x of xs) save(x); xs.map(y => load(y)); once();`
    → `save` and `load` are in a loop, `once` is not.
  - `facts::tests::shop_containers`: the containers are `web`, `api` and `api-client`; the
    externals are `postgres` (Store) and `stripe` (External).
  - `facts::tests::shop_onion_guess`: `api` guesses Onion, with `api/checkout` in `application`
    and `api/domain-order` in `domain`.
  - `facts::tests::single_package_repo`: one root package with `src/a` and `src/b` gives 1
    container with 2 components.
  - `facts::tests::groups_past_twelve`: 30 folders give ≤ 12 components, of kind `Group` where
    needed.
  - `arrows::tests::lifts_to_common_parent`: in shop, `web` → `api` is Network, and
    `api/domain-order` → `api/infra-prisma` is Depends.
  - `arrows::tests::bundles_mixed_kinds`: 3 calls and 1 import give one Bundle labeled
    `"3 calls · 1 import"`.
- [ ] **Step 2: Run** `cargo test -p ig-lang-ts marks_calls_in_loops && cargo test -p ig-app map::`.
  Expected: FAIL.
- [ ] **Step 3: Implement.** Update the goldens for `inLoop` where it is true.
- [ ] **Step 4: Run** `cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings`.
  Expected: PASS.
- [ ] **Step 5: Commit** `Map: facts, arrows, calls in loops`.

### Task 4: The map file: merge, annotations, validation

**Files:**
- Create: `crates/ig-app/src/map/file.rs`.

**Interfaces:**
- **Consumes:** `facts`, `arrows` (Task 3).
- **Produces:**
  - `pub struct Field<T> { value: T, source: Source /*Ig|Agent|Person*/ }`.
  - ```rust
    pub struct MapBox {
        id, parent, level, made_of, package, files,
        kind: Field<BoxKind>, title: Field<String>, tech: Option<Field<String>>,
        description: Option<Field<String>>, logo: Option<Field<String>> /*simple-icons slug*/,
        badges: Option<Field<Vec<String>>>, pattern: Option<Field<Pattern>>, slot: Option<Field<String>>,
        service_name: Option<String>, pinned: Vec<String> /*field names*/,
        needs_reading: bool, stale: bool,
    }
    ```
  - `pub struct MapFile { version: u32 /*1*/, boxes: Vec<MapBox>, arrows: Vec<MapArrow>, marks: Vec<Mark> }`.
    It is written with keys sorted and boxes and arrows sorted by id, so diffs are stable.
  - `pub fn load(root: &Path) -> (Option<MapFile>, Vec<String>)`. Over 2 MB, unparsable, or
    failing `validate` → `None` plus a notice.
  - `pub fn refresh(old: Option<&MapFile>, facts: &[FactBox], arrows: Vec<MapArrow>) -> MapFile`:
    - a box that is in the facts and in the old file keeps the old annotations;
    - a field that the old file lists as pinned keeps its value even if ig guesses otherwise;
    - a new box gets ig's guess (`source: Ig`) and `needs_reading: true`;
    - an old box that is gone is dropped, unless it has pins, in which case it is kept with
      `stale: true`;
    - arrows and marks always come from the new facts.
  - `pub struct Annotation { id, kind?, title?, tech?, description?, logo?, badges?, pattern?, slot?, service_name?, made_of? }`.
  - `pub fn annotate(map: &mut MapFile, json: &str) -> Result<usize, Vec<String>>` applies a JSON
    array of `Annotation` as `source: Agent`:
    - it skips pinned fields;
    - it clears `needs_reading`;
    - it returns the count applied, or every validation error;
    - `made_of` may regroup the components of one container (an agent merging folders): the old
      components of that container are replaced, and every file must still belong to exactly one
      component.
  - `pub fn validate(m: &MapFile) -> Result<(), Vec<String>>`:
    - ids: containers `^[a-z0-9][a-z0-9-]{0,63}$`, components `<container>/<slug>`, unique;
    - title ≤ 60, tech ≤ 60, description ≤ 200, ≤ 8 badges of ≤ 40;
    - globs relative, with no `..` and no leading `/`;
    - `slot` valid for the parent's pattern;
    - ≤ 12 children per box;
    - unknown fields rejected.
  - `pub fn save(root: &Path, m: &MapFile) -> Result<()>` writes `.ig/map.json` with
    `ig_fs::write_atomic`.

- [ ] **Step 1: Write the failing tests:**
  - `refresh_keeps_annotations`:
    - an old file with an agent description on `api/checkout`, a person-pinned slot on
      `api/refund`, and a box `api/legacy` (pinned title) whose folder is gone;
    - new facts with folder `api/src/application/billing` added.
    - Expect: the checkout description is kept; the refund slot is kept although ig guesses
      otherwise; `api/legacy` is `stale`; `api/billing` has `needs_reading`.
  - `annotate_respects_pins`: annotating the pinned slot leaves it, and the count excludes it.
  - `rejects_hostile_annotations`: each of a `..` glob, an id `"Bad Id"`, a 201-character
    description, an unknown kind, an unknown field, and 13 components returns `Err` naming the
    problem, and the map is unchanged.
  - `save_is_stable`: `save` twice yields byte-identical files, and boxes are sorted by id.
- [ ] **Step 2: Run** `cargo test -p ig-app map::file`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run.** Expected: PASS.
- [ ] **Step 5: Commit** `Map: the map file, refresh, annotations`.

### Task 5: Layers, the code level, layout and routing

**Files:**
- Create: `crates/ig-app/src/map/{layers.rs,code.rs,layout.rs,route.rs,view.rs}`.
- Delete: `crates/ig-app/src/view_data.rs`. Adapt `crates/ig/src/{main.rs,render.rs}` and the
  explore and view skills' mention of `ig view --json`.

**Interfaces:**
- **Consumes:** `MapFile`, `Graph`.
- **Produces:**
  - `pub struct Mark { layer: String, target: String, state: MarkState /*Ok|Warn|Bad|Down*/, badge: Option<String>, value: Option<String>, note: Option<String> }`.
  - `pub fn rules(m: &MapFile, findings: &[Finding]) -> Vec<Mark>` (layer `rules`):
    - Onion: an arrow from an inner ring to an outer ring is `Bad`, badge
      `"breaks: {from slot} → {to slot}"` (ruling R4);
    - Layered: an arrow going up the bands is `Bad`;
    - each `ig.yaml` rules finding is mapped onto the arrow carrying its edge.
  - `pub fn risks(m: &MapFile) -> Vec<Mark>` (layer `risks`):
    - an `in_loop` arrow into a Store, Port, External or Network target is `Warn`, badge
      `"risk: query in a loop"`;
    - an App container whose longest chain of Network arrows is ≥ 3 is `Warn`, badge
      `"{n} network hops"`.
  - Both are stored in `MapFile.marks` by `refresh`.
  - `pub fn file_layers(root) -> (Vec<Mark>, Vec<String>)` reads `.ig/layers/*.json`, each
    `{ "layer": "...", "marks": [...] }` (≤ 512 KB, ≤ 5,000 marks). Marks on unknown targets are
    dropped with a notice. These are not stored in map.json.
  - `pub fn code_boxes(m: &MapFile, g: &Graph) -> (Vec<MapBox>, Vec<MapArrow>)`: the function and
    class nodes inside each component, grouped to ≤ 12 by file, then by name prefix. Each id is
    the graph node id. Arrows are lifted the same way as in Task 3.
  - `pub struct Frame { x, y, w, h: f32 }`, in the parent's own inner units.
  - `pub struct Region { parent, kind /*Ring|Band|Group*/, label, frame }`.
  - `pub struct Placed { frames: BTreeMap<String,Frame>, inner: BTreeMap<String,(f32,f32)>, regions: Vec<Region>, routes: BTreeMap<String, Vec<(f32,f32)>> }`.
  - `pub fn layout(boxes, arrows) -> Placed`, with these rules:
    - mockup sizes; gutters ≥ 48;
    - system level: people, then containers, then externals;
    - onion rings nest, domain innermost; layered bands in order; hexagonal core, ports and
      adapters; None as a grid of ≤ 4 columns;
    - Manhattan routes through gutter channels that never cross a sibling box.
  - `pub struct View { version: u32, boxes, arrows, marks, placed: Placed, notices: Vec<String> }`
    and `pub fn view(root, m: &MapFile, g: &Graph) -> View`.
  - `ig view --json` prints `View`.

- [ ] **Step 1: Write the failing tests:**
  - `rules::onion_outward_is_bad`: shop gives one `Bad` mark on `api/domain-order->api/infra-prisma`
    with `"breaks: domain → infrastructure"`.
  - `risks::loop_to_port_is_risk`: shop's checkout loop over `orderRepo.save` gives a `Warn` mark
    with `"risk: query in a loop"`.
  - `file_layer_marks`: a health file with marks on `api` and `nope` gives 1 mark and 1 notice.
  - `code::groups_past_twelve_code`: 30 functions in one component give ≤ 12 children.
  - `layout::no_overlap`, `layout::routes_avoid_boxes`, `layout::onion_rings_nest`,
    `layout::deterministic`: no two sibling frames intersect and each is inside its parent; no route segment crosses a sibling frame other than its own ends; domain ⊂ application ⊂ infrastructure; two runs are byte-identical. Run for shop and for 12 seeded random trees.
  - `tests/view.rs::view_json_shape`: `ig view --json` on shop has `boxes`, `arrows`, `marks`
    and `placed`, and no `nodes` key.
- [ ] **Step 2: Run** `cargo test --workspace`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings`.
  Expected: PASS.
- [ ] **Step 5: Commit** `Map: layers, code level, layout, routing, view`.

### Task 6: The AI agent reads the architecture

**Files:**
- Create: `crates/ig-app/src/map/agent.rs`, `crates/ig/assets/prompts/read.md`.
- Modify: `crates/ig/src/{main.rs,cli.rs}` (`ig view` flags), `crates/ig-core/src/config.rs`
  (`agent:`).

**Interfaces:**
- **Consumes:** `refresh`, `annotate`, `save` (Task 4).
- **Produces:**
  - `ig.yaml` `agent: claude | gemini | codex` (optional).
  - `pub enum Agent { Claude, Gemini, Codex }`, with:
    - `pick(cfg, flag) -> Option<Agent>`: the flag, then the config, then the first found on PATH
      in the order claude, gemini, codex;
    - `command(&self, prompt_file) -> Command`:
      - Claude: `claude -p <prompt> --output-format text`;
      - Gemini: `gemini -p <prompt>`;
      - Codex: `codex exec <prompt>`.
  - **The prompt** (`read.md`, embedded with `include_str!`) gives:
    - the spec's level and kind definitions and the pattern rules;
    - the boxes that need reading, with their facts (made_of, files, sample file names, imports
      in and out, packages);
    - the instruction to answer with ONE JSON array of `Annotation` between `<ig-annotations>`
      and `</ig-annotations>`, and nothing else.
  - `pub fn read(root, agent, map: &mut MapFile, progress: &mut dyn FnMut(&str)) -> Result<usize, String>`:
    - runs the agent with a 10-minute timeout;
    - extracts the tagged block;
    - calls `annotate`;
    - on validation errors, re-asks once with the errors appended;
    - if it still fails, returns `Err` and leaves the map untouched.
    - Progress lines: `Asking {agent} to read {n} boxes…`, `{agent} answered, checking…`,
      `Read {k} boxes.`
  - **`ig view`:**
    1. refresh;
    2. if any box `needs_reading` and stdin and stdout are a TTY and neither `--no-read` nor
       `--read` is given, ask `Let an AI agent ({agent}) read the architecture? [Y/n]`;
    3. read (`--read` reads all boxes, even already-annotated ones);
    4. save;
    5. render;
    6. open.
    - With no agent installed, print `No AI agent found (claude, gemini or codex): showing ig's own reading.`
      and continue.
    - `--agent <name>` picks the agent.

- [ ] **Step 1: Write the failing tests** (a fake agent: a script on a temp PATH named `claude`
  that prints a canned reply):
  - `agent_annotates`: the fake reply sets the checkout description; after `ig view --read --no-open`
    `.ig/map.json` has it with `source: "agent"`.
  - `agent_garbage_keeps_map`: a reply of prose without the tag, twice, leaves the map
    byte-identical, exits 0 and prints the reason.
  - `agent_retry_on_errors`: the first reply is invalid (a `..` glob) and the second valid; the
    second prompt contains the error text.
  - `no_agent_still_views`: with an empty PATH, `ig view --no-open` writes the HTML and prints
    the no-agent line.
  - `pick_order`: with `gemini` and `codex` on PATH and no config, Gemini is picked; with
    `agent: codex` in config, Codex is picked.
- [ ] **Step 2: Run** `cargo test -p intent-graph agent_`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `cargo test --workspace`. Expected: PASS.
- [ ] **Step 5: Commit** `ig view: an AI agent reads the architecture into the map file`.

### Task 7: The viewer model: types, camera, detail and looks

**Files:**
- Create: `viewer/src/map/types.ts` (a mirror of `View` from Task 5: `MapBox` with `Field<T> = {value, source}`, `MapArrow`, `Mark` with `layer`, `Placed`), `viewer/src/map/camera.ts`,
  `viewer/src/map/looks.ts`.
- Test: `viewer/test/map-camera.test.ts`, `viewer/test/map-looks.test.ts`.

**Interfaces:**
- **Produces:**
  - `type Cam = { x: number; y: number; k: number }`.
  - `fitTo(frame: Frame, view: {w:number;h:number}, margin = 0.1): Cam`.
  - `worldFrame(id: string, view: View): Frame`: the absolute frame, composing the parent
    scales.
  - `detailFor(screenW: number): "far" | "mid" | "near"`: < 140 far, < 220 mid, otherwise near.
  - `showsInside(screenW: number): boolean`: ≥ 560.
  - `look(id: string, marks: Mark[], on: Set<string> /*layer ids shown*/, box: MapBox): Look`, where
    `Look = { edge: string; width: number; dashed: boolean; glow: string|null; bg: string; fade: number; chips: {label:string; tone:"neutral"|"warn"|"bad"|"down"}[]; dot: boolean }`.
    It applies the precedence health > highlight > status > kind. The `health` layer is the file
    layer with id `health`.
  - `insideCounts(view: View, marks, on): Map<string, number>`: the count of `Bad` and `Down`
    marks under each box, for the red "N inside" badge at every level up.
  - `arrowLook(arrow: MapArrow, marks, on): { color: string; width: number; dash: string|null; head: "filled"|"open"|"hollow"; label: string|null; tone }`.

- [ ] **Step 1: Write the failing tests:**
  - `fitTo` centers a 100×50 frame in a 1000×800 view at k = 8. The margin is 10% per side, so
    the usable area is 800×640 and the width limits.
  - `worldFrame` of a code box composes 3 scales.
  - `detailFor(139) === "far"`, `detailFor(140) === "mid"`, `detailFor(220) === "near"`.
  - `look` with health Bad and kind usecase gives edge `#c0392b`, width 2.5 and a glow, and no
    chips for an Ok mark other than `dot: true`.
  - With the health layer off, the same box gives edge `#2f8f5b`.
  - `insideCounts`: one Bad mark on a code box gives a count of 1 on its component, its container
    and the system.
  - `arrowLook` for Event gives `#7b4fc9`, dash `"2 5"` and head `"open"`. A Warn risk mark on a
    Network arrow gives color `#d08a00` and keeps its dash.
- [ ] **Step 2: Run** `cd viewer && bun run test map-`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run.** Expected: PASS.
- [ ] **Step 5: Commit** `Map viewer: types, camera, looks`.

### Task 8: The box, the arrow and the region components

**Files:**
- Create: `viewer/src/map/MapBox.svelte`, `viewer/src/map/MapArrow.svelte`,
  `viewer/src/map/Region.svelte`, `viewer/src/map/logo.ts`.
- Test: `viewer/test/map-box.test.ts`.

**Interfaces:**
- **Consumes:** `look`, `arrowLook` and `detailFor` (Task 7).
- **Produces:**
  - `<MapBox box look detail onopen />`. It is a port of `Box.dc.html` in the mockup canvas: the
    same anatomy, sizes, fonts and per-level looks. It is a `<button>` with
    `aria-label="Open {title}"`.
  - `<MapArrow points arrowLook />`: an SVG polyline with the head marker, and the label in a pill
    that cuts the line at the longest segment's middle, hidden at far detail.
  - `<Region region />`: a ring, band or group with its label.
  - `logo.ts`: `logoPath(slug: string): string | null`, from simple-icons by slug; unknown slugs
    return null and the kind icon is used.

- `MapBox` also shows a dotted outline when `needsReading` and a "stale" chip when `stale`.

- [ ] **Step 1: Write the failing tests** (happy-dom render):
  - a box with `needsReading` has the dotted outline;
  - a container box with logo `stripe` renders an `svg path` from simple-icons;
  - at `detail="far"` there are no description or chips;
  - a person renders a figure and no box border;
  - `logoPath("not-a-logo") === null`.
- [ ] **Step 2: Run** `bun run test map-box`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run.** Expected: PASS.
- [ ] **Step 5: Commit** `Map viewer: box, arrow, region`.

### Task 9: The canvas in `ig view`

**Files:**
- Create: `viewer/src/map/Map.svelte`, `viewer/src/map/DetailsCard.svelte`,
  `viewer/src/map/Controls.svelte` (breadcrumb, layers panel, zoom), `viewer/src/map/glide.ts`.
- Modify:
  - `viewer/src/components/App.svelte`: in `ig view` the map fills the page; the app shell keeps
    its placeholder;
  - `viewer/src/api/{index,embedded}.ts`: `view(): Promise<View>`, read from the embedded data.
- Test: `viewer/test/map-glide.test.ts`.

**Interfaces:**
- **Consumes:** everything from Tasks 7 and 8.
- **Produces:**
  - `glide(from: Cam, to: Cam, t: number): Cam`: eases `cubic-bezier(0.2,0.7,0.2,1)` over 800 ms
    and interpolates scale in log space.
  - `class Camera { to(target: Cam): void; tick(now: number): Cam }`. A new `to()` during a glide
    restarts from the current position.
- **Map behavior:**
  - The world `<div>` uses `transform: translate(x,y) scale(k)`.
  - Clicking a box glides to `fitTo(its frame)`. Wheel zooms around the pointer. Dragging pans.
  - A box's inside is mounted only while `showsInside(screenW)` holds, and fades in over 400 ms.
  - The breadcrumb follows the deepest box that fills ≥ 60% of the view.
  - Selecting a box opens the details card:
    - health values from the `health` layer;
    - "needs attention" from the rules and risks marks;
    - connections;
    - "made of", `service.name`, and each annotated field's source;
    - Open in editor (the existing editor link).
  - The layers panel holds toggles for rules, risks and each file layer. Zoom −, Fit and +.
  - When any box `needsReading`, one line on the map reads "Some boxes are ig's guess. Run
    `ig view --read` to let an AI agent read them."
  - No modes, tabs or side panels.
  - Not in this iteration: chat, progress bar and live updates.

- [ ] **Step 1: Write the failing tests:**
  - `glide(a, b, 0)` deep-equals `a` and `glide(a, b, 800)` deep-equals `b`. The halfway scale
    between k 1 and k 4 is about 2.6 (log interpolation plus easing; assert between 2.2 and 3.2).
  - `camera_retarget_mid_glide`: `to(A)`, tick at 300 ms, `to(B)`, tick at 1100 ms deep-equals B.
- [ ] **Step 2: Run** `bun run test map-glide`. Expected: FAIL.
- [ ] **Step 3: Implement**, then `bun run typecheck && bun run test && bun run build && bun run build:app`.
- [ ] **Step 4: Verify** `ig view` on a scratch copy of the shop fixture:
  - the containers are drawn;
  - clicking `api` glides into the onion;
  - the red domain → infrastructure arrow and the amber loop risk show;
  - dropping `.ig/layers/health.json` and re-running `ig view` turns `api` amber.
  Screenshot the browser window only (`screencapture -l`).
- [ ] **Step 5: Commit** `ig view: the walkable canvas`.

### Task 10: Real repos, timings, docs

- [ ] **Step 1:** Run `ig view --read` on scratch copies of intake and shop-front, with one
  agent run each, and record whether the containers, patterns and descriptions are right.
- [ ] **Step 2: Measure.**
  - `ig view --no-read --no-open` on shop-front (3,760 nodes) must take < 2 s warm;
  - the HTML's first paint must be < 1 s;
  - the glide must run at ≥ 50 fps.
- [ ] **Step 3: Docs.** README "ig view" section and a short "System map" section in
  `docs/specs/design.md`; a 10-step walkthrough at
  `docs/plans/2026-10-27-system-map-walkthrough.md`.
- [ ] **Step 4:** `make test`, all green.
- [ ] **Step 5: Commit** `ig view: docs, walkthrough, timings`.
