# System Map (goal 1) Implementation Plan

**Goal:** Strip ig down to what the map needs. Then build goal 1: open a repo and see its system as a
few boxes in the shape of its architecture (containers → components → code), zoom smoothly into any
box, and see the rules and risks layers. Health states are drawn from layer files, so production
data can plug in later.

**Architecture:**
- **Rust computes everything** (ig-app `map` module):
  - the reading (`.ig/map.json`, or a fallback from ig's facts);
  - the box tree;
  - the aggregated arrows;
  - the layout and orthogonal routes;
  - the layers.
  The result is shipped as one `MapView` JSON to the desktop app (Tauri command) and to `ig view`
  (embedded).
- **The viewer is plain DOM/SVG** in one transformed "world". A box's inside is rendered, scaled,
  inside the box once it is big enough on screen. Zoom is a CSS-transform camera.
- **Claude writes the reading** through two MCP tools and one skill.

**Tech Stack:** Rust 2024 (ig-core, ig-lang-ts, ig-app, ig, ig-desktop/Tauri 2), Svelte 5 + TS +
Vite (bun), vitest + happy-dom, simple-icons (already a dependency).

**Spec:** `docs/specs/2026-10-27-ig-system-map-design.md`. Mockups:
the design canvas. Screen 4 and `Box.dc.html` are the visual
reference, including colors, sizes and fonts.

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
   empty map. Tested in Task 3 (`fallback_single_package`).
2. **A `map.json` whose globs match nothing after a folder rename:** the box shows "needs refresh",
   and the rest of the map still renders. Tested in Task 4 (`stale_glob_flags_box`).
3. **A level with more than 12 children** (a big folder at the code level) is grouped. Never more
   than 12 are drawn. Tested in Task 4 (`groups_past_twelve`).
4. **A malformed or hostile `map.json`** (oversized, bad ids, unknown kinds, `..` or absolute
   globs) is rejected with a notice, and the fallback map is shown. Tested in Task 3
   (`rejects_hostile_reading`) and Task 8 (`map_write_rejects`).
5. **Clicking again or scrolling during a glide** ends at the last target, never stuck halfway.
   Tested in Task 10 (`camera_retarget_mid_glide`).

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
  `plan.md`, `apply.md` and `setup.md`.
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
  - CLI commands: `ig view`, `ig mcp`, `ig init`, `ig plugin export <dir>`.
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

### Task 3: The reading: `.ig/map.json` and the fallback

**Files:**
- Create: `crates/ig-app/src/map/mod.rs`, `crates/ig-app/src/map/reading.rs`.
- Create: `crates/ig/tests/fixtures/shop/`, a pnpm monorepo:
  - `apps/web` (Next-style pages importing `@shop/api-client`);
  - `apps/api/src/{domain,application,infra,routers}`, an onion where
    `domain/order.ts` imports `infra/prisma.ts` on purpose;
  - `packages/api-client`;
  - an `ig.yaml` with `blueprint.systems: {Postgres: [@prisma/client], Stripe: [stripe]}`;
  - `.ig/map.json` (a valid reading of it).

**Interfaces:**
- **Produces** (serde, camelCase JSON):
  - ```rust
    pub struct Reading {
        pub version: u32,
        pub system: SystemEntry,
        pub people: Vec<Person>,
        pub containers: Vec<ContainerEntry>,
        pub externals: Vec<ExternalEntry>,
    }
    ```
  - `pub struct SystemEntry { id, title, description }`.
  - `pub struct Person { id, title, description, uses: Vec<String> }`, where `uses` holds
    container ids.
  - `pub struct ContainerEntry { id, title, kind: ContainerKind, tech, description, logo: Option<String>, badges: Vec<String>, made_of: Vec<String>, pattern: Pattern, components: Vec<ComponentEntry>, pinned: bool }`.
  - `pub enum ContainerKind { App, Api, Worker, Store, Queue }`.
  - `pub enum Pattern { Onion, Layered { bands: Vec<String> }, Hexagonal, None }`, serialized as
    `{"kind":"onion"}` or `{"kind":"layered","bands":[…]}`.
  - `pub struct ComponentEntry { id, title, kind: ComponentKind, description, made_of: Vec<String>, slot: String, routes: Vec<String>, pinned: bool }`.
  - `pub enum ComponentKind { Entry, Usecase, Port, Domain }`.
  - `slot` is the ring (`domain`, `application` or `infrastructure`), the band name, or `core`,
    `port` or `adapter`.
  - `pub struct ExternalEntry { id, title, description, logo: Option<String>, packages: Vec<String> }`.
  - `pub fn load(root: &Path) -> (Option<Reading>, Vec<String> /*notices*/)` reads `.ig/map.json`.
  - `pub fn validate(r: &Reading) -> Result<(), Vec<String>>`.
  - `pub fn fallback(ws: &Workspace, graph: &Graph, cfg: &Config) -> Reading`.
- **Validation limits:**
  - file ≤ 256 KB;
  - ids as in Global Constraints, unique;
  - title ≤ 60, tech ≤ 60, description ≤ 200;
  - ≤ 12 containers + externals, ≤ 12 components per container, ≤ 8 people;
  - globs relative, with no `..` and no leading `/`;
  - `slot` valid for the pattern;
  - `uses` refers to known containers;
  - unknown fields are rejected (`deny_unknown_fields`).
- **Fallback:**
  - one container per workspace package that has source (root only when it is the sole
    package), id = the package name slugified (scope dropped), pattern `None`;
  - components = its top-level source folders (grouped at > 12);
  - externals = `blueprint.systems`;
  - no people.

- [ ] **Step 1: Write the failing tests** in `reading.rs`:
  - `loads_valid_shop_reading`: `load(fixture shop)` returns `Some`, with 3 containers, `api`
    pattern Onion, and component `api/checkout` slot `application`.
  - `rejects_hostile_reading`: for each of a `..` glob, an id `"Bad Id"`, a description of 201
    chars, 13 components, an unknown field, and a 300 KB file, `load` returns `None` plus a notice
    naming the problem.
  - `fallback_single_package`: a workspace with one root package and folders `src/a`, `src/b`
    gives 1 container with 2 components.
  - `fallback_monorepo`: on the `shop` fixture without `map.json`, the containers are `web`, `api`
    and `api-client`, and the externals are `postgres` and `stripe`.
- [ ] **Step 2: Run** `cargo test -p ig-app map::reading`. Expected: FAIL (module missing).
- [ ] **Step 3: Implement** with serde and `globset` (already a dependency).
- [ ] **Step 4: Run.** Expected: PASS.
- [ ] **Step 5: Commit** `Map: the reading and its fallback`.

### Task 4: The box tree and the arrows

**Files:**
- Create: `crates/ig-app/src/map/tree.rs`, `crates/ig-app/src/map/arrows.rs`.
- Modify: `crates/ig-lang-ts/src/extract/{visitor.rs,visit_impl.rs,mod.rs}`,
  `crates/ig-lang-ts/src/resolve.rs`, `crates/ig-core/src/model.rs`.

**Interfaces:**
- **Consumes:** `Reading` (Task 3) and `ig_core::model::Graph`.
- **Produces:**
  - `Edge.in_loop: bool` (JSON `inLoop`, skipped when false).
    - It is true when the call happens inside a `for`, `for…in`, `for…of`, `while` or `do`
      body, or inside a callback passed to `.map`, `.forEach`, `.filter`, `.reduce`, `.flatMap`,
      `.some` or `.every`.
    - It is set from a `loop_depth` counter in the visitor, stamped by `add_ref` and carried by
      `RawRef.in_loop`.
  - `pub struct MapBox { id, parent: Option<String>, level: Level, kind: BoxKind, title, meta, description, logo: Option<String>, badges: Vec<String>, made_of: Vec<String>, service_name: Option<String>, slot: Option<String>, stale: bool, nodes: Vec<String> /*graph node ids inside*/ }`.
  - `pub enum Level { System, Container, Component, Code }`.
  - `pub enum BoxKind { Person, System, External, App, Api, Worker, Store, Queue, Entry, Usecase, Port, Domain, Function, Class, Group }`.
  - `pub fn tree(r: &Reading, g: &Graph) -> Vec<MapBox>`. Code-level children are the graph's
    function and class nodes in the component's files. A level with more than 12 children gets
    `Group` boxes, by folder, then by name prefix. `stale` is set when a `made_of` matches no file.
  - `pub struct MapArrow { id, parent: String, from: String, to: String, kind: ArrowKind, label: Option<String>, count: u32, kinds: Vec<ArrowKind>, in_loop: bool, edges: Vec<(String,String)> }`.
  - `pub enum ArrowKind { Uses, Depends, Calls, Network, Event, Implements, Data, Bundle }`.
  - `pub fn arrows(boxes: &[MapBox], g: &Graph) -> Vec<MapArrow>`. Each graph edge is lifted to
    the children of the lowest common parent of its two ends.
    - Kind mapping:
      - imports → Depends;
      - calls, renders, dispatch → Calls;
      - implements and extends (to a Port) → Implements;
      - http → Network;
      - uses to an external whose category is database → Data, otherwise Network;
      - person `uses` → Uses.
    - More than one kind between the same pair → Bundle. Its label is `"{n} calls · {m} requests"`,
      listing only the non-zero kinds in the order calls, requests, imports, singular for 1 ("1 import").
    - Label of a Network arrow: the route (`POST /checkout`) when there is one route, otherwise the
      tech (`tRPC`). Label of a Calls arrow: the callee name when there is one, otherwise none.

- [ ] **Step 1: Write the failing tests:**
  - `extract::tests::marks_calls_in_loops`: `for (const x of xs) save(x); xs.map(y => load(y)); once();`
    → `save` and `load` have `in_loop` true, `once` false.
  - `tree::tests::shop_tree`: there are boxes `api`, `api/checkout` and
    `api/checkout#placeOrder`-style code ids, every code box's parent is a component, and no
    parent has more than 12 children.
  - `tree::tests::groups_past_twelve`: 30 functions in one component make ≤ 12 children, of
    kind `Group` where needed.
  - `tree::tests::stale_glob_flags_box`: a component with `made_of: ["gone/**"]` has
    `stale == true`, and the other boxes are intact.
  - `arrows::tests::lifts_to_common_parent`: in shop, `web` → `api` is Network with label `tRPC`,
    and `api/domain-order` → `api/infra-prisma` is Depends at component level.
  - `arrows::tests::bundles_mixed_kinds`: 3 calls and 1 import between two components give one
    Bundle with label `"3 calls · 1 import"`.
- [ ] **Step 2: Run** `cargo test -p ig-lang-ts marks_calls_in_loops && cargo test -p ig-app map::`.
  Expected: FAIL.
- [ ] **Step 3: Implement.** Update the goldens for the new `inLoop` field where it is true.
- [ ] **Step 4: Run** `cargo test --workspace`. Expected: PASS.
- [ ] **Step 5: Commit** `Map: box tree, arrows, calls in loops`.

### Task 5: Layout and routing

**Files:**
- Create: `crates/ig-app/src/map/layout.rs`, `crates/ig-app/src/map/route.rs`.

**Interfaces:**
- **Consumes:** `MapBox`, `MapArrow`, `Pattern`.
- **Produces:**
  - `pub struct Frame { x: f32, y: f32, w: f32, h: f32 }`, in the parent's inner coordinates. Each
    parent's inner space is laid out in its own units, and the viewer scales it into the parent.
  - `pub struct Region { parent: String, kind: RegionKind /*Ring|Band|Group*/, label: String, frame: Frame }`.
  - `pub struct Placed { frames: HashMap<String, Frame>, inner: HashMap<String, (f32,f32)> /*inner w,h per parent*/, regions: Vec<Region>, routes: HashMap<String, Vec<(f32,f32)>> /*arrow id → orthogonal points*/ }`.
  - `pub fn layout(boxes: &[MapBox], arrows: &[MapArrow], patterns: &HashMap<String, Pattern>) -> Placed`.
- **Rules:**
  - Box sizes come from the mockup at "near" detail: container 264×152, component 248×152, code
    220×112, person 180×136, system 280×152. Gutters are ≥ 48.
  - **System level:** people on top, containers in the middle rows, externals at the bottom.
    Inside each row, order by arrows so that most arrows are vertical.
  - **Onion:** three nested rings, `domain` innermost. Components sit in their ring's band. Arrows
    should point inward.
  - **Layered:** bands top to bottom in the order of `bands`.
  - **Hexagonal:** core in the center, ports on its edge, adapters around it.
  - **None:** a grid of ≤ 4 columns.
  - **Routing:** Manhattan paths through gutter channels, on a coarse grid where box cells are
    blocked. Each path leaves the middle of a box edge and ends 1 unit short of the target edge.

- [ ] **Step 1: Write the failing tests:**
  - `no_overlap`: for shop and for 12 random trees (fixed seed), no two sibling frames intersect,
    and every frame is inside its parent's inner box.
  - `routes_avoid_boxes`: no route segment crosses a sibling frame, other than at its own two
    ends.
  - `onion_rings_nest`: `domain` is inside `application`, which is inside `infrastructure`.
  - `deterministic`: two runs give byte-identical `serde_json::to_string(&placed)`.
- [ ] **Step 2: Run** `cargo test -p ig-app map::layout map::route`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run.** Expected: PASS.
- [ ] **Step 5: Commit** `Map: layout per pattern, orthogonal routes`.

### Task 6: Layers, the MapView, and delivery

**Files:**
- Create: `crates/ig-app/src/map/layers.rs`, `crates/ig-app/src/map/view.rs`.
- Delete: `crates/ig-app/src/view_data.rs`.
- Modify:
  - `crates/ig-desktop/src/commands.rs`: `map_view` replaces `view_data`;
  - `crates/ig-desktop/src/main.rs`: the watcher emits `map-changed` on a graph rebuild, a
    `.ig/map.json` change or a `.ig/layers/*.json` change;
  - `crates/ig/src/{main.rs,render.rs}`: `ig view` embeds a `MapView`.

**Interfaces:**
- **Produces:**
  - `pub struct Mark { target: String, state: MarkState /*Ok|Warn|Bad|Down*/, badge: Option<String>, value: Option<String>, note: Option<String> }`.
  - `pub struct Layer { id: String, title: String, marks: Vec<Mark> }`.
  - `pub fn rules(boxes, arrows, patterns, findings: &[Finding]) -> Layer`, with id `rules`:
    - Onion: an arrow from an inner ring to an outer ring is `Bad`, badge
      `"breaks: domain → infra"` (that is, `"breaks: {from slot} → {to slot}"`).
    - Layered: an arrow going up the bands is `Bad`.
    - The `ig.yaml` `rules` findings are mapped onto the arrow carrying their edge.
  - `pub fn risks(boxes, arrows) -> Layer`, with id `risks`:
    - an arrow with `in_loop` whose target is a Store, Port, External or Network is `Warn`, badge
      `"risk: query in a loop"`;
    - an entry container whose longest chain of sync Network arrows is ≥ 3 is `Warn`, badge
      `"{n} sync hops"`.
  - `pub fn file_layers(root: &Path) -> (Vec<Layer>, Vec<String>)` reads `.ig/layers/*.json`. Each
    file is a `Layer` (≤ 512 KB, ≤ 5,000 marks). Unknown targets are dropped with a notice.
  - `pub struct MapView { version: u32, boxes: Vec<MapBox>, arrows: Vec<MapArrow>, placed: Placed, layers: Vec<Layer>, notices: Vec<String>, reading: ReadingSource /*File|Fallback*/ }`.
  - `pub fn map_view(env: &Env) -> Result<MapView>`.
  - Tauri: `map_view() -> MapView`; event `map-changed` (no payload).

- [ ] **Step 1: Write the failing tests:**
  - `layers::tests::onion_outward_is_bad`: shop gives one `Bad` mark on the `domain-order` →
    `infra-prisma` arrow, badge `"breaks: domain → infrastructure"`.
  - `layers::tests::loop_to_store_is_risk`: shop's `checkout` loops over `orderRepo.save`, giving
    a `Warn` mark with `"risk: query in a loop"`.
  - `layers::tests::file_layer_marks`: `.ig/layers/health.json` with marks on `api` (Warn,
    value `"p95 1.4 s"`) and on `nope` ends with 1 mark kept and 1 notice.
  - `view::tests::shop_view_snapshot`: `map_view` on shop serializes. Assert the top-level keys,
    20 or fewer system-level frames, and `reading == "file"`.
  - `crates/ig/src/tests/view.rs::view_embeds_map`: the `ig view` HTML contains `"placed"` and no
    `"nodes":[` from the old format.
- [ ] **Step 2: Run** `cargo test --workspace`. Expected: FAIL.
- [ ] **Step 3: Implement.** `env` builds the graph as today (cache included).
- [ ] **Step 4: Run** `cargo test --workspace && cargo clippy --workspace -- -D warnings`.
  Expected: PASS.
- [ ] **Step 5: Commit** `Map: layers, MapView, delivery to the app and ig view`.

### Task 7: The viewer model: types, camera, detail and layers

**Files:**
- Create: `viewer/src/map/types.ts` (a mirror of `MapView`), `viewer/src/map/camera.ts`,
  `viewer/src/map/looks.ts`.
- Test: `viewer/test/map-camera.test.ts`, `viewer/test/map-looks.test.ts`.

**Interfaces:**
- **Produces:**
  - `type Cam = { x: number; y: number; k: number }`.
  - `fitTo(frame: Frame, view: {w:number;h:number}, margin = 0.1): Cam`.
  - `worldFrame(id: string, view: MapView): Frame`: the absolute frame, composing the parent
    scales.
  - `detailFor(screenW: number): "far" | "mid" | "near"`: < 140 far, < 220 mid, otherwise near.
  - `showsInside(screenW: number): boolean`: ≥ 560.
  - `look(id: string, layers: Layer[], on: Set<string>, box: MapBox): Look`, where
    `Look = { edge: string; width: number; dashed: boolean; glow: string|null; bg: string; fade: number; chips: {label:string; tone:"neutral"|"warn"|"bad"|"down"}[]; dot: boolean }`.
    It applies the precedence health > highlight > status > kind. The `health` layer is the file
    layer with id `health`.
  - `insideCounts(view: MapView, layers, on): Map<string, number>`: the count of `Bad` and `Down`
    marks under each box, for the red "N inside" badge at every level up.
  - `arrowLook(arrow: MapArrow, layers, on): { color: string; width: number; dash: string|null; head: "filled"|"open"|"hollow"; label: string|null; tone }`.

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

### Task 8: Claude writes the reading: MCP tools and the skill

**Files:**
- Modify: `crates/ig/src/mcp.rs` (two tools), `crates/ig/src/skills.rs`, `crates/ig/src/init.rs`.
- Create: `crates/ig/src/mcp/map.rs`, `crates/ig/assets/skills/map.md`.
- Test: `crates/ig/src/mcp/tests.rs`.

**Interfaces:**
- **Consumes:** `reading::{validate, fallback}`, `tree`, `arrows`, `ig_fs::write_atomic`.
- **Produces:**
  - MCP `ig_map_facts {}` returns JSON:
    - `packages`: name, dir, dependencies, entry;
    - `folders`: the tree to depth 4, with file counts;
    - `imports`: aggregated folder → folder edges with counts and kinds;
    - `externals`: blueprint systems, plus npm dependencies matching a known list (`pg`,
      `@prisma/client`, `mongodb`, `redis`, `ioredis`, `bullmq`, `stripe`, `@aws-sdk/*`, `kafkajs`,
      `amqplib`, `@sentry/*`);
    - `current`: the current `map.json`, or null.
    - The output is capped at 200 KB, trimmed deepest-first.
  - MCP `ig_map_write { reading: Reading }`:
    - validates;
    - merges, keeping every container or component that is `pinned: true` in the current file as
      it is;
    - writes `.ig/map.json` atomically;
    - returns `"Map written: N containers, M components"` or the validation errors.
  - Skill `map.md` (the `/ig:map` command): call `ig_map_facts`, decide the pattern per container
    using the spec's rules, write with `ig_map_write`, then explain the reading in ≤ 6 sentences.
    A correction from the user means: change that box, set `pinned: true`, write again.

- [ ] **Step 1: Write the failing tests:**
  - `map_facts_shop`: has `packages.len() == 3` and `externals` containing `stripe`.
  - `map_write_rejects`: a reading with a `..` glob is not written; the reply names the glob.
  - `map_write_keeps_pinned`: the current file pins `api/checkout` slot `application`; writing a
    reading that moves it to `domain` keeps `application`.
  - `skill_listed`: `ig plugin export` lists `skills/map/SKILL.md`.
- [ ] **Step 2: Run** `cargo test -p intent-graph mcp::`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `cargo test --workspace`. Expected: PASS.
- [ ] **Step 5: Commit** `Map: Claude reads the architecture (ig_map_facts, ig_map_write, /ig:map)`.

### Task 9: The box, the arrow and the region components

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

- [ ] **Step 1: Write the failing tests** (happy-dom render):
  - a container box with logo `stripe` renders an `svg path` from simple-icons;
  - at `detail="far"` there are no description or chips;
  - a person renders a figure and no box border;
  - `logoPath("not-a-logo") === null`.
- [ ] **Step 2: Run** `bun run test map-box`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run.** Expected: PASS.
- [ ] **Step 5: Commit** `Map viewer: box, arrow, region`.

### Task 10: The map canvas, the controls and the app

**Files:**
- Create: `viewer/src/map/Map.svelte`, `viewer/src/map/DetailsCard.svelte`,
  `viewer/src/map/Controls.svelte` (breadcrumb, layers panel, zoom), `viewer/src/map/glide.ts`.
- Modify: `viewer/src/components/App.svelte`, `viewer/src/api/{index,tauri,embedded}.ts`
  (`mapView()`, `on("map-changed")`).
- Test: `viewer/test/map-glide.test.ts`.

**Interfaces:**
- **Consumes:** everything from Tasks 7 and 9. `api.mapView()`. The chat session's `send(text)`.
- **Produces:**
  - `glide(from: Cam, to: Cam, t: number): Cam`: eases `cubic-bezier(0.2,0.7,0.2,1)` over 800
    ms and interpolates scale in log space.
  - `class Camera { to(target: Cam): void; tick(now: number): Cam }`. A new `to()` during a glide
    restarts from the current position.
- **Map behavior:**
  - The world `<div>` uses `transform: translate(x,y) scale(k)`.
  - Clicking a box glides to `fitTo(its frame)`. Wheel zooms around the pointer. Dragging pans.
  - A box's inside is mounted only while `showsInside(screenW)` holds, and fades in over 400 ms.
  - The breadcrumb follows the deepest box that fills ≥ 60% of the view.
  - Selecting a box opens the details card: production values from the health layer, "needs
    attention" from the rules and risks marks, connections, "made of", `service.name`, and the
    buttons Zoom in, Open in editor (existing editor link) and links.
  - The layers panel holds toggles for rules, risks and each file layer.
  - The top of the map shows "Read the architecture" when `reading == "fallback"`. In the app it
    sends `/ig:map` to the chat session. In `ig view` it shows the command to run.
  - On `map-changed` the map re-fetches through the existing `coalesce`. The camera keeps its
    place when the box it shows still exists, and otherwise fits the system.

- [ ] **Step 1: Write the failing tests:**
  - `glide(a, b, 0)` deep-equals `a` and `glide(a, b, 800)` deep-equals `b`. The halfway scale
    between k 1 and k 4 is about 2.6 (log interpolation plus easing; assert between 2.2 and 3.2).
  - `camera_retarget_mid_glide`: `to(A)`, tick at 300 ms, `to(B)`, tick at 1100 ms deep-equals B.
- [ ] **Step 2: Run** `bun run test map-glide`. Expected: FAIL.
- [ ] **Step 3: Implement**, then build: `bun run typecheck && bun run test && bun run build && bun run build:app`.
- [ ] **Step 4: Verify in the app on the shop fixture copy** (`make app-dev`, Project › Open…):
  - the containers are drawn;
  - clicking `api` glides into the onion;
  - the red domain → infrastructure arrow and the amber loop risk show;
  - dropping a `.ig/layers/health.json` turns `api` amber live.
  Take an app-window screenshot (`screencapture -l <id>`).
- [ ] **Step 5: Commit** `Map viewer: the canvas, zoom, details, layers, Read the architecture`.

### Task 11: Real repos, timings, docs

**Files:**
- Modify: `README.md`, `docs/specs/design.md`: replace the removed sections with a short "System
  map" section pointing at the spec.
- Create: `docs/plans/2026-10-27-system-map-walkthrough.md`, a 10-step manual
  walkthrough.

- [ ] **Step 1: Run `/ig:map` on scratch copies** of intake and shop-front, at most 3 short
  prompts each, and record whether the pattern and containers it found are right.
- [ ] **Step 2: Measure.** `map_view` time on shop-front (3,760 nodes) must be < 1 s warm.
  Measure first paint and the glide fps in the app (Performance panel); target ≥ 50 fps.
- [ ] **Step 3: Write the docs and the walkthrough.**
- [ ] **Step 4: Run** `make test` (all green).
- [ ] **Step 5: Commit** `System map: docs, walkthrough, timings`.
