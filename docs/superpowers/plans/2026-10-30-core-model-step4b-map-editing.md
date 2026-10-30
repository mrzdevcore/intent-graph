# Core model, step 4b: editing on the map Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** In the desktop app the map is shown, stays live, and can be edited: add a box, draw an arrow, rename, move, delete, undo/redo, and keep or drop Claude's and the AI reading's proposals — all through step 4a's edit operations and journal.

**Architecture:** The desktop app (Tauri, crates/ig-desktop) gains commands that call `ig_app::map::edit::journal` (`record` with `By::Person`, `keep`, `drop`, `undo`, `redo`, `entries`) and return the fresh View; the View carries the not-kept proposals. The viewer's `IgApi` gets `view()` and `onRebuild()` for the desktop (today only `ig view`'s page has `view`), and an optional `edit` API; when `edit` is present the map shows the editing toolbar, popovers and the proposals bar. `ig view`'s page stays read-only (no `edit`).

**Tech Stack:** Rust (ig-desktop, ig-app), Svelte 5 viewer, Vitest + happy-dom, Tauri 2.

**Spec:** `docs/superpowers/specs/2026-10-30-ig-core-model-design.md` (Editing; Claude's edits). Design: the "4b: editing on the map" row of https://claude.ai/artifact/Wny722GMWPs6bRBu7KZy9k (EditAdd, EditArrow, EditProposals). Step 4a (merged) built `map::edit` (`Edit`, `apply`), `edit::journal`, `ig accept`, `ig fmt`, MCP `ig_edit`.

## Global Constraints

- Every change to `ig/` goes through `edit::journal::record` (person edits: `By::Person`, kept) or `keep`/`drop`/`undo`/`redo`; the viewer never writes files.
- Editing exists only where the host provides `edit` (the desktop app); `ig view`'s page stays read-only and unchanged.
- A refused edit (reader errors, journal postcondition) changes nothing and is shown to the user as a short message near the action, with the reader's text.
- Viewer styles use tokens only (`var(--map-*)`, `--space-*`, `--text-*`, `--radius-*`), rem not px (world-space box geometry excepted), no raw hex; text only, never `{@html}`; real `<button>`/`<input>`/`<label>` elements, `aria-label` on icon buttons, focus visible.
- Keyboard: V select, B box, A arrow, F2 rename, M move to…, ⌫/Delete delete, ⌘Z / ⌘⇧Z undo/redo, Esc cancels the current tool or popover; shortcuts never fire while typing in an input or the chat.
- Desktop tests: Rust unit tests for commands' logic (call the plain functions, not Tauri); viewer tests with a fake `edit` API.
- Files under 600 lines.

## Review Focus

- An edit made while the file watcher also fires a rebuild: the map shows the new state once, no flicker back to the old one, the camera and selection stay.
- Delete on a box that holds others or has arrows pointing at it: refused with the reader's message naming them; nothing changes.
- Undo after the person edited the same field by hand in their editor: refused with the journal's message; the map unchanged.
- The add-box form's "code lives at" prefill for a box in a parent with several `at` globs, and for a parent with none.
- Proposals bar with 0 entries is not shown; with entries from two authors, counts per author; Keep all / Drop all act on not-kept entries only.

---

### Task 1: Leftovers from 4a

**Files:** `crates/ig-app/src/map/spec.rs` (HEADER), every golden/test expecting the header, `crates/ig/assets/skills/explore.md`, `crates/ig-app/src/map/edit/journal.rs` (+ tests)

- HEADER becomes exactly `// ig's intended architecture: edit freely; ig changes it only when you or an agent ask (ig accept, ig fmt, edits in the app, ig_edit).` (update goldens and fixtures that contain the old line; `ig fmt` keeps whichever header the file has).
- explore.md: drop "Read-only: never edit files… only .ig/show.json" and the "Read-only" in its description; say instead: it reads code freely, writes only `.ig/show.json` itself, and changes the architecture only through `ig_edit` (proposals).
- `redo` checks, like `drop`/`undo`, that the spec is still as the undo left it (the entry's inverse postcondition), else refuses with "entry <n> changed since; nothing done".
- `keep(Some(n))` with an unknown n returns `Err(["no journal entry <n>"])` and writes nothing.
- [ ] Failing tests (redo after a hand edit refuses; keep unknown n errs; header text in a fresh `write`) → implement → `cargo test` (workspace), clippy → commit `"4a leftovers: header, explore skill, redo and keep checks"`.

### Task 2: The map in the desktop app, live

**Files:** `viewer/src/api/index.ts`, `viewer/src/api/tauri.ts`, `viewer/src/components/App.svelte`, `viewer/test/api.test.ts` (or a new app test)

**Interfaces:**
- `IgApi.view` documented as "the map (both hosts)"; `tauri()` gains `view: () => invoke<View>("view_data")` and `onRebuild(cb: () => void): () => void` (subscribes to the existing `rebuild` event).
- App keeps one `view` in state (load once, then replace it on every rebuild and after every edit), passes it to `<Map {view}>` without remounting Map (camera, selection and hand moves stay); a failed reload keeps the last view and shows the error in the header hint.
- [ ] Failing test (a fake api whose `view` changes on a rebuild callback → the map's box count updates, `Map` not remounted: the camera transform string is unchanged) → implement → `bun run test`, svelte-check → commit `"Desktop: the map, live on rebuild"`.

### Task 3: Desktop edit commands, and proposals in the View

**Files:** create `crates/ig-desktop/src/edits.rs` (+ register in main.rs `generate_handler!`); modify `crates/ig-app/src/map/view.rs` (View gains `proposals`), `crates/ig-app/src/map/edit/journal.rs` only if a helper is needed; tests in `edits.rs` and view tests

**Interfaces:**
- View gains `proposals: Vec<Proposal>` (serde camelCase, skipped when empty): `Proposal { n: u64, by: "claude" | "reading", summary: String, ids: Vec<String> }` for every journal entry not kept and not undone; `summary` is one short line (`add usecase Billing`, `move Refund → Billing`, `Order: description`); `ids` are the box ids (and arrow ids `from->to`) the edit touches, for highlighting.
- Commands, each running in the project root, each returning `Result<serde_json::Value /* the fresh View */, String /* message */>`:
  `edit_apply(edit: Edit)` → `record(By::Person, &edit)`; `edit_keep(n: Option<u64>)`; `edit_drop(n: u64)`; `edit_drop_all()` (drops every not-kept entry, newest first, stopping at the first refusal with a message naming it); `edit_undo()`; `edit_redo()`. Errors: the reader's / journal's messages joined with "; ".
- [ ] Failing tests (plain functions on a temp repo: apply Add → View has the box `origin: build`; a Claude-recorded entry appears in `proposals`; keep removes it from proposals; drop reverts the file; undo/redo round trip; a refused edit returns Err and the files are unchanged) → implement → `cargo test -p ig-desktop -p ig-app`, clippy → commit `"Desktop: edit commands; proposals in the View"`.

### Task 4: The editing toolbar and keyboard

**Files:** `viewer/src/api/index.ts` (+ `EditApi`), `viewer/src/api/tauri.ts`, create `viewer/src/map/edit/Toolbar.svelte`, `viewer/src/map/edit/tool.svelte.ts` (the current tool state), modify `viewer/src/map/Map.svelte`, `viewer/src/map/Controls.svelte` (layout), tests `viewer/test/map-edit.test.ts`

**Interfaces:**
- `EditApi { apply(edit: Edit): Promise<View>; keep(n?: number): Promise<View>; drop(n: number): Promise<View>; dropAll(): Promise<View>; undo(): Promise<View>; redo(): Promise<View> }`; `Edit` TS type mirrors the Rust enum (`op` tag, lowercase, same fields).
- `Map` gets `edit?: EditApi` and `onview?: (v: View) => void` (App replaces its view with the returned one). Toolbar (top left, under the breadcrumb): Select (V), + Box (B), → Arrow (A), separator, Undo (⌘Z), Redo (⌘⇧Z), per the EditAdd artboard; shown only with `edit`. Tool state: `select | box | arrow`; Esc returns to select. A refused call shows its message in a small toast near the toolbar for 6 s (and in an `aria-live="polite"` region).
- [ ] Failing tests (no `edit` → no toolbar; with a fake `edit`: B/A/V switch tools, ⌘Z calls undo and the returned view replaces the map's, shortcuts ignored while an input has focus, a rejected call shows the toast text) → implement → viewer tests, svelte-check → commit `"Map: the editing toolbar and shortcuts"`.

### Task 5: Add a box

**Files:** create `viewer/src/map/edit/AddBox.svelte`; modify `Map.svelte`; tests in `map-edit.test.ts`

- With the Box tool, a click on a box's inside (or on the system ground) opens the add form (EditAdd artboard) anchored at the click, inside that parent: Kind (a `<select>` of the kinds allowed at that depth: under the system `app, service, library, worker, store, queue, external, person`; under a container `usecase, domain, port, folder, component, entry`), Name (required), Code lives at (prefilled: the parent's first `at` glob with its trailing `**` replaced by `<slug(name)>/**`; `<slug(name)>/**` when the parent has none; editable; for `external`/`person` empty and hidden).
- Enter submits `Add { parent, kind, id: <slug(name), de-clashed against existing ids>, title: name, at, before: null }`; Esc closes. The new box is placed by the layout (not at the click point).
- [ ] Failing tests (prefill for a parent with `apps/api/**` → `apps/api/billing/**`; kinds offered under the system vs under a container; submit calls `apply` with that Add; Esc sends nothing) → implement → viewer tests → commit `"Map: add a box"`.

### Task 6: Draw an arrow, and the box menu

**Files:** create `viewer/src/map/edit/ArrowDraw.svelte`, `viewer/src/map/edit/BoxMenu.svelte`, `viewer/src/map/edit/MovePicker.svelte`; modify `Map.svelte`, `MapBox.svelte` (handles on the selected box), `DetailsCard.svelte` (a "⋯" menu button); tests

- Arrow: with the Arrow tool (or dragging a selected box's handle), press on a box and release on another: the target lights (`--map-green`), then the popover (EditArrow artboard) offers kind chips `calls, depends, data, network, event` (preselect the last kind used this session, else `calls`) and an optional label; Enter → `Arrow { from, to, kind, label }`. Releasing on empty ground or the same box cancels.
- Box menu (right-click on a box, or the details card's "⋯" button): Rename (F2: inline input over the title; Enter → `Set { id, key: "title", value }`), Move to… (M: a searchable list of the boxes that can hold it; pick → `Move { id, parent, before: null }`), Add box inside (B with that parent), Arrow from here (A from that box), Delete (⌫ → `Remove { id }`; the reader's refusal names what blocks it).
- [ ] Failing tests (drag A→B then Enter calls apply with the Arrow; F2 + typing + Enter calls Set title; Move to… lists only valid parents and calls Move; Delete calls Remove and shows the refusal text; right-click opens the menu, Esc closes) → implement → viewer tests, svelte-check → commit `"Map: draw an arrow; rename, move, delete"`.

### Task 7: Keep or drop proposals

**Files:** create `viewer/src/map/edit/Proposals.svelte`; modify `Map.svelte`, `looks.ts` (a proposed box/arrow: edge `var(--map-trace)` dashed + a tag chip with the author), `DetailsCard.svelte` (Keep / Drop for the selected box's proposals), `types.ts` (`View.proposals`); tests

- Bar (EditProposals artboard), shown when `view.proposals` is not empty: "Claude proposed N edit(s) · AI reading M", Review (opens the list), Drop all (`dropAll`), Keep all (`keep()`). List: one row per proposal (author tag, summary, Drop, Keep); hovering a row lights its `ids` on the map (as search does). Boxes and arrows in any proposal's `ids` get the proposal look and a tag chip `Claude` / `reading`.
- [ ] Failing tests (no proposals → no bar; two authors → counts; Keep on a row calls keep(n); Drop all calls dropAll; a proposed box has the tag chip; hover lights the box) → implement → viewer tests, svelte-check, `bun run build` → commit `"Map: keep or drop proposals"`.

### Task 8: Try it in the app

- [ ] `make app-dev`; on a copy of the shop fixture: the map shows; add a box, draw an arrow, rename, move, delete (and its refusal), undo/redo; ask Claude in the chat to add a box → it appears gold in the bar; keep one, drop one; edit a file in an editor → the map updates. Note anything off for the final review (no commit unless a fix is needed, then a focused commit).

## Not in this step

- Dragging a box into another box to move it (Move to… covers it; dragging stays "rearrange inside the parent").
- Placing a new box where it was clicked (the layout places it; hand moves can follow).
- Editing `at` globs, tech, description, logo in the details card (Rename only; the rest by ig_edit, hand or a later step).
