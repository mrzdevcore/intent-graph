# Editing the map like Excalidraw: implementation plan

**Goal:** Draw boxes and arrows first, set kinds and details after, edit arrows, place boxes smartly,
arrange the map in one go, and move around it naturally.

**Architecture:** The ig/ format and edits change in Rust (`crates/ig-app/src/map`): a `box` kind,
`context` and arrow `note`/`context`, `ArrowSet` and `Batch` edits. Everything about places,
gestures, selection and panels is in the Svelte viewer (`viewer/src/map`), with pure TypeScript
modules (placement, arrange, navigation, kind guess, clipboard) tested on their own and thin
components on top.

**Tech Stack:** Rust (kdl crate, serde), Svelte 5 runes, TypeScript, vitest + happy-dom, Tauri.

**Spec:** `docs/specs/2026-11-10-excalidraw-editing-design.md`

## Global Constraints

- Colours only as `var(--color-*)`, `var(--map-*)`, `var(--kind-*)`; sizes from `tokens.css`; rem,
  not px, except hairlines and world-space sizes.
- As few code comments as possible: only a non-obvious why, one short line.
- Every `ig/` change stays a small in-place KDL edit; other lines keep their text.
- `note` at most 80 characters, one line. `context` any length.
- A new `box` kind: title "Not decided yet", a code kind (takes `at`), never misplaced.
- Batch: all or nothing, one journal entry, only the end state is validated.
- Tests: `cargo test --workspace`; `cd viewer && pnpm run test`; `pnpm run typecheck` with no
  warnings.

## Review Focus

- A map file in the wild with an unknown arrow property must still read (warning, not error).
- Setting a kind back to `box` and undoing must give the file back byte for byte.
- Esc on an empty draft box, or a draft from a dropped arrow, must leave `ig/` untouched.
- A pinch on a trackpad must zoom, never pan; a mouse wheel must zoom; a trackpad scroll must pan.
- Arrange run twice must not move anything the second time.

---

### Task 1: The `box` kind

**Files:** Modify `crates/ig-app/assets/kinds.kdl`, `crates/ig-app/src/map/facts.rs` (`BoxKind`,
`role`), `crates/ig-app/src/map/warn.rs` (misplaced checks), `crates/ig/assets/ig-map.5`,
`docs/reference.md`. Test: `crates/ig-app/src/map/edit/tests/kind.rs`,
`crates/ig-app/src/map/spec/tests`.

**Interfaces:** Produces `BoxKind::Box` (serde name `box`), `Role::Run` for placement purposes but
exempt from every misplaced warning, for itself and its children.

- [ ] Test `reads_a_box_with_no_kind_yet`: `box "billing" title="billing"` in `ig/system.kdl` reads;
  the map holds a box of kind `box`, no warning.
- [ ] Test `a_box_holds_run_and_design_blocks`: a `box` holding a `service` and a `module` reads with
  no misplaced warning.
- [ ] Test `set_kind_from_box_and_back_restores_bytes`: `set kind=store` then the inverse gives the
  file back byte for byte; `set kind=box` drops nothing.
- [ ] Test `plan_lists_a_box_as_to_build_kind_not_decided` (where `ig plan` builds its list).
- [ ] Implement; `cargo test -p ig-app` passes; commit "A box kind: a box whose kind is not decided
  yet".

### Task 2: `context` on boxes, `note` and `context` on arrows

**Files:** Modify `crates/ig-app/src/map/spec/read.rs` (`props`, `arrow`), `file.rs` (`MapBox`
gets `context: Option<Field<String>>`), `arrows.rs` (`MapArrow` gets `note: Option<String>`,
`context: Option<String>`, serde camelCase, skipped when none), `edit/set.rs` (key `context`),
`ig-map.5`, `docs/reference.md`, viewer `types.ts` (`MapBox.context?`, `MapArrow.note?`,
`MapArrow.context?`). Test: `crates/ig-app/src/map/spec/tests`, `edit/tests.rs`.

- [ ] Test `reads_box_context_multiline`: `context #"""…"""#` raw multi-line string reads as text.
- [ ] Test `reads_arrow_note_and_context`; `rejects_a_note_over_80_or_with_a_newline` (error).
- [ ] Test `unknown_arrow_property_is_a_warning`: `calls "x" colour="red"` reads, with one warning.
- [ ] Test `set_context_writes_a_raw_string_when_multiline` and its inverse restores bytes.
- [ ] Implement; tests pass; commit "Context for whoever builds it: on boxes, and a note and
  context on arrows".

### Task 3: `ArrowSet`

**Files:** Modify `crates/ig-app/src/map/edit.rs` (variant), new `crates/ig-app/src/map/edit/arrow.rs`,
viewer `types.ts` (`{ op: "arrowset"; from; to; key; value: string | null }`). Test: new
`crates/ig-app/src/map/edit/tests/arrow.rs`.

**Interfaces:** `Edit::ArrowSet { from: String, to: String, key: String, value: Option<String> }`,
serde op `arrowset`. Keys: `kind`, `label`, `note`, `context`, `from`, `to`, `flip`.

- [ ] Tests, one per key, each checking the file and that the inverse restores bytes:
  `arrowset_kind`, `_label`, `_note`, `_context`, `_to_reconnects_keeping_props`,
  `_from_moves_the_node_to_the_new_source_box`, `_flip_swaps_ends_keeping_props`.
- [ ] Tests refusals: `arrowset_refuses_a_duplicate_pair`, `_refuses_a_self_arrow`,
  `_refuses_an_unknown_arrow`, `_refuses_a_note_over_80`.
- [ ] Implement in `edit/arrow.rs`; commit "Edit an arrow after it is drawn".

### Task 4: `Batch`

**Files:** Modify `edit.rs` (`Edit::Batch { edits: Vec<Edit> }`, op `batch`; `Spec::edit` applies
each in memory, collects inverses in reverse into a `Batch`; `save` once), `journal.rs` (nothing
special: one `record`), `crates/ig/src/mcp.rs` docs for `ig_edit`, viewer `types.ts`. Test:
`edit/tests/batch.rs`, `edit/tests/journal.rs`.

- [ ] Test `batch_applies_in_order_and_saves_once` (add box + arrow to it in one batch).
- [ ] Test `batch_is_all_or_nothing`: second edit refused → files unchanged, error names it.
- [ ] Test `batch_only_checks_the_end_state`: add `x`, arrow to `x`, remove the arrow, remove `x`.
- [ ] Test `batch_is_one_journal_entry_and_undoes_in_one_step`.
- [ ] Implement; commit "Batch edits: all or nothing, one undo".

### Task 5: Arrow kinds and rules in the view

**Files:** Modify `crates/ig-app/src/map/view.rs` (`arrowKinds: Vec<ArrowKindView>` with
`name`, `to: Vec<String>`, `from: Vec<String>` from `kinds.kdl` rules, without `bundle`),
viewer `types.ts`, `edit/arrows.ts` (read from the view). New `viewer/src/map/edit/guess.ts`.
Test: `crates/ig-app/src/map/view/tests.rs`, `viewer/test/map-guess.test.ts`.

**Interfaces:** `guessArrow(fromKind: string, toKind: string, kinds: ArrowKindView[]): ArrowKind`.

- [ ] Rust test `view_sends_arrow_kinds_and_rules` (7 kinds, `writes` to `store`).
- [ ] TS tests: store→`writes`, queue→`sends`, job→`triggers`, entry→`triggers`,
  adapter→port→`implements`, box→`uses`, service→`calls`.
- [ ] Implement; commit "The view sends arrow kinds; the viewer guesses a new arrow's kind".

### Task 6: Navigation and the map's dot grid

**Files:** New `viewer/src/map/nav.ts`; modify `glide.ts`, `Map.svelte` (wheel, pointer, keys,
background). Test: `viewer/test/map-nav.test.ts`.

**Interfaces:**
- `wheelIntent(e: {deltaX, deltaY, deltaMode, ctrlKey, metaKey, shiftKey}, pref: "auto"|"zoom"|"pan"): "zoom"|"pan"`
  (ctrl/meta → zoom; fractional or two-axis small deltas → pan; whole-line or ≥ 50px single-axis →
  zoom; pref overrides auto).
- `gridLevel(k: number, unit: number): { step: number; fade: number }`: screen spacing
  `unit*k*step` kept between 8 and 64 px, `fade` 0..1 for the halfway dots.
- `momentum(v: {x, y}, ms: number): {x, y}` (decay 0.92 per 16 ms, stops under 0.05 px/ms).

- [ ] Tests for `wheelIntent` (pinch, mouse wheel, trackpad scroll, prefs), `gridLevel` (levels at
  k = 0.1, 0.5, 1, 4), `momentum` (decays, stops).
- [ ] Wire: Space or middle-button or hand tool drag pans; select drag on ground is the marquee
  (Task 11); ⌘± ⌘0 Shift+1 Shift+2, eased 150 ms; soft zoom limits; edge auto-pan while dragging a
  box; grid drawn in world space from `cam` with two dot layers (step, halfway with `fade`); while a
  gesture runs only the transform changes, detail and wires update on settle; more-menu setting
  "Scroll wheel: zoom | pan" kept in localStorage.
- [ ] Commit "Natural zoom and pan; the dot grid belongs to the map".

### Task 7: Placement: places, hold-still, free spots, nudges, shared undo

**Files:** New `viewer/src/map/place/places.ts` (store, replaces `ig-moves:` with
`ig-places:<system>` and reads the old key once), `place/rules.ts`, `place/history.ts`;
modify `Map.svelte` (`shown`, `within`). Test: `viewer/test/map-place.test.ts`.

**Interfaces:**
- `type Place = { x: number; y: number; w: number; h: number }` (parent inner units).
- `class Places { get(id): Place | undefined; set(id, p); forget(ids: string[]); snapshot(): Record<string, Place> }`
- `settle(prev: Record<string, Place>, next: View, frames, held: string | null): Record<string, Place>`
  keeps every previously seen box at its place, places new boxes by `freeSpot`, resolves overlaps
  by `nudge`, grows parents.
- `freeSpot(parent: string, box: string, placed: Record<string, Place>, arrows): Place`
- `nudge(placed: Record<string, Place>, held: string, gap: number): Record<string, Place>`
- `class PlaceHistory { push(before, after); undo(); redo() }` and `undoNext(edits, places)` that
  picks the latest of a place change or an `ig/` edit (each push stamped with a counter shared with
  edit runs).

- [ ] Tests: new box lands next to its arrow partner; free spot never overlaps; overlap nudge moves
  the fewest pixels and repeats outward; the held box never moves; a child leaving its parent grows
  the parent; delete leaves the gap; old `ig-moves:` offsets are read once; shared undo order.
- [ ] Wire into `Map.svelte`: snapshot before each `run`, `settle` on the new view, glide what moved.
- [ ] Commit "Smart placement: places stay, new boxes find room, overlaps nudge".

### Task 8: Arrange

**Files:** New `viewer/src/map/place/arrange.ts`; menu items and the island tool. Test:
`viewer/test/map-arrange.test.ts` on `fixtures/shop-view.json`.

**Interfaces:** `arrange(scope: string[], view: View, places: Record<string, Place>, grid: number): Record<string, Place>`

Algorithm (layered, deterministic): per group from the innermost out: break cycles by reversing
arrows against id order; layer by longest path (people and apps forced to layer 0; stores,
externals to the last); order each layer by barycentre over 4 down/up sweeps, ties by id; place
layers left to right with equal gaps on `grid`; align each box to the median y of its strongest
neighbour when that keeps the order; group = one block sized to its children plus the usual margin.

- [ ] Tests: arrows point left to right after arrange; crossings ≤ before on the fixture;
  `arrange(arrange(x)) == arrange(x)`; same input → same output; one undo restores.
- [ ] Wire: island wand, ⌥⇧F, more menu, group menu "Arrange inside", multi-select "Arrange these";
  toast "Arranged N boxes" with Undo.
- [ ] Commit "Arrange: a formatter for the map".

### Task 9: Tool island and drawing boxes

**Files:** Replace `edit/Toolbar.svelte` with `edit/Island.svelte`; new `edit/Draft.svelte`;
modify `edit/tool.svelte.ts` (`"hand"|"select"|"box"|"arrow"|"frame"`, `locked: boolean`),
`edit/session.svelte.ts`, `Map.svelte`; bottom-left `edit/Corner.svelte` (zoom, undo, redo).
Test: `viewer/test/map-edit.test.ts`.

- [ ] Tests: drag with box draws a draft at that size; click places the default; double-click
  ground in select makes a draft; Enter sends `add` kind `box` with the title inside the box under
  it; Tab confirms and opens the kind list; Esc on empty sends nothing; the tool returns to select
  unless locked; corner handles resize (a place change, nudges others).
- [ ] Commit "Draw a box and name it where it sits".

### Task 10: Connect

**Files:** Modify `edit/session.svelte.ts` (`startLink`, `moveLink`, `endLink`), `edit/ArrowDraw.svelte`
(no popover: the arrow is sent at once with the guessed kind and its chip opens the kind list),
`Map.svelte` (side dots on hover). Test: `viewer/test/session.test.ts`.

- [ ] Tests: dots on hover under any tool; release on a box sends `arrow` with `guessArrow`; release
  on ground makes a draft and, on confirm, sends `batch [add, arrow]`; Esc leaves nothing; two boxes
  selected + A connects first to second.
- [ ] Commit "Connect by dragging from a box's side".

### Task 11: Selection, box bar, kind dropdown, menus

**Files:** New `edit/BoxBar.svelte`, `edit/KindPicker.svelte`, `edit/kinds.ts` (filter and rank);
replace `edit/BoxMenu.svelte` items; new `edit/GroundMenu.svelte`; marquee in `Map.svelte`.
Test: `viewer/test/map-kinds.test.ts`, `map-edit.test.ts`.

**Interfaces:** `rankKinds(q: string, kinds: KindView[], context: { parentKinds: string[]; inRun: boolean }): { best: KindView | null; groups: { title: string; kinds: KindView[] }[] }`

- [ ] Tests: filter and best match; best before typing = kind most used among siblings; Enter
  sends `set kind`; "No kind" sends `set kind=box`; right-click menu items and keys; marquee and
  Shift-click selection; multi bar: kind (batch), group (disabled across parents, else batch add
  group + moves, title in edit mode), delete (batch with arrows and children, inline confirm when
  children), arrange these.
- [ ] Frame tool F: drag draws a group around what it encloses (same batch as group).
- [ ] Commit "Set a box's kind afterwards; select several boxes".

### Task 12: Arrows: select, bar, reconnect, inline label, note line

**Files:** New `edit/ArrowBar.svelte`; modify `MapArrow.svelte` (wide hit path, selected look,
end handles, label and note line in `--font-hand`, note cut with an ellipsis), `session.svelte.ts`.
Test: `viewer/test/map-edit.test.ts`.

- [ ] Tests: click selects; bar items; kind change sends `arrowset kind`; note field sends
  `arrowset note`; flip; delete sends `unarrow`; end-handle drop sends `arrowset to|from`;
  double-click edits label; a bundle's bar lists its parts read-only; a drift arrow shows "Keep it
  in ig/" which sends `batch [arrow, …]`; context dot when `context` set.
- [ ] Commit "Edit an arrow on the map".

### Task 13: Details panel editing

**Files:** New `edit/BoxPanel.svelte`, `edit/ArrowPanel.svelte`; modify `DetailsCard.svelte` (opens
them for editing on E / pencil). Test: `viewer/test/map-edit.test.ts`.

- [ ] Tests: box fields (name, kind, description, tech, kind props with chips for one-of, context,
  at) each send one `set` on blur/Enter; Shift+Enter is a newline in context; arrow fields (kind
  chips, label, note, context) send `arrowset`; the KDL preview shows the arrow as written; Esc
  closes and refocuses the map.
- [ ] Commit "Edit everything about a box or an arrow in the panel".

### Task 14: Copy and paste

**Files:** New `viewer/src/map/edit/clipboard.ts`; keys in `Map.svelte`. Test:
`viewer/test/map-clipboard.test.ts`.

**Interfaces:**
- `copy(ids: string[], view: View, inside: boolean, places): Copy` and
  `toKdl(c: Copy): string`, `fromKdl(text: string, kinds): Copy | null`.
- `paste(c: Copy, parent: string, at: {x, y}, view: View): { batch: Edit; places: Record<string, Place> }`
  (fresh ids `id-2…`, no `at`, arrows only between copied boxes).

- [ ] Tests: boxes alone; with inside (children and inner arrows with note and context); outside
  arrows dropped; `at` never copied; fresh ids; KDL round trip; paste of KDL from an `ig/` file;
  Duplicate ⌘D / ⌘⇧D; one undo.
- [ ] Commit "Copy and paste boxes, alone or with what's inside".

### Task 15: Look: Excalifont, light dots, design system

**Files:** Add `viewer/src/assets/fonts/Excalifont-*.woff2` and `OFL.txt`; `viewer/src/styles/tokens.css`
(`--font-hand: Excalifont, Kalam, "Comic Sans MS", cursive` and `@font-face` with the two unicode
ranges), `themes/light.css` (`--map-dots: #e2dccd`), box kind `box` look (dashed `map-edge`).
Update the ig design system artifact tokens (hand family, light dots).

- [ ] Typecheck and tests pass; the build includes the fonts; commit "Excalidraw's hand font and
  the dot grid in light".

### Task 16: Whole-branch check

- [ ] `make test` and `pnpm run typecheck` pass.
- [ ] Try it in the desktop app on the shop fixture and on shop-front: the "Done when" list
  of the spec, step by step.
- [ ] Review of the whole branch; fix findings; commit.
