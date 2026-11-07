# ig diff step 3: the change drawn on the map

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `ig diff` writes `.ig/diff.html`, the head's map with the change marked in direction A and a card that steps through it, and opens it.

**Architecture:** The Rust side builds the head's `View` next to the `Diff` and fills a new `/*IG_DIFF*/` marker in the same viewer page `ig view` uses. In the viewer, a pure function turns `(View, Diff)` into the `Answer` the map already knows how to fly through and fade around, plus the marks direction A draws (colours, file counts, "removed here" tags, ghost arrows). The same page is the future embed: nothing in it depends on being opened by `ig`.

**Tech Stack:** Rust (ig-app, ig), Svelte 5 + TypeScript + Vite single-file build, vitest with happy-dom.

**Spec:** `docs/superpowers/specs/2026-11-06-ig-diff-design.md`, section "On the map". Design: https://claude.ai/artifact/7R1w8x77NjkJaY1xKFhrmM, row 1 board A and row 2 (picked 2026-11-07: direction A; the page doubles as the embeddable widget).

## Global Constraints

- Colours only from the themes: `--color-changed`, `--color-added`, `--color-removed` (both `viewer/src/styles/themes/*.css` define them); sizes from `tokens.css`, in rem, never px.
- `ig view` afterwards shows the plain map; the diff belongs to the `ig diff` page only (`.ig/diff.html`, never `.ig/view.html`).
- stdout carries results only; notices on stderr. `--json` prints JSON only and writes no page.
- Exit codes as the CLI design: 0 ok, 2 usage or error.
- No `.rs` file over 600 lines; as few code comments as possible (a non-obvious why, one line).
- Data in the page is escaped like `IG_DATA`: every `<` becomes `<`.

## Review Focus

1. A diff with no change: the page shows the plain map, nothing faded, and a card reading "No change to the architecture". (Task 3 test.)
2. A removed box whose parent was removed too: counted on its nearest ancestor that is still on the map. (Task 3 test.)
3. An added or removed arrow between boxes nested in different parents: matched or drawn at the level the map draws it (the view lifts arrows and keeps the written ones in `parts`). (Task 3 test.)
4. Titles holding `</script>` in the diff: the page still loads. (Task 1 test.)
5. `ig diff A B` with a head commit: the page draws commit B's map, not the working tree's. (Task 1 test.)

---

### Task 1: `ig diff` writes `.ig/diff.html`

**Files:**
- Modify: `viewer/index.html` (add `<script id="ig-diff" type="application/json">/*IG_DIFF*/</script>` after `ig-show`)
- Modify: `crates/ig/build.rs` (same tag in `PLACEHOLDER`)
- Modify: `crates/ig-app/src/map/diff/build.rs` (`run` also returns the head's `View`)
- Modify: `crates/ig/src/render.rs` (`diff_page`, `write_diff`)
- Modify: `crates/ig/src/cli.rs` (`DiffArgs.no_open`)
- Modify: `crates/ig/src/diff.rs` (write and open the page in text mode)
- Test: `crates/ig/src/render.rs` tests, `crates/ig/src/tests/diff.rs` (new, registered in `tests/mod.rs`)
- Modify: `README.md` (`ig diff` section and the commands table: the page, `--no-open`)

**Interfaces:**
- Produces: `ig_app::map::diff::build::run(env, base, head, at, view_at: Option<&dyn Fn(Option<&Path>) -> crate::Result<View>>) -> crate::Result<(Diff, Option<View>)>`; `view_at(None)` is the working tree, `view_at(Some(root))` a head checkout; `--json` passes `None` and gets `None`.
- Produces: `render::diff_page(view: &Value, diff: &Value) -> String` (fills `IG_DATA`, `IG_DIFF`; `IG_SHOW` is `null`), `render::write_diff(w: &Wired, v: &View, d: &Diff) -> Result<PathBuf>` writing `.ig/diff.html`. `page()` fills `/*IG_DIFF*/` with `null`.
- Produces: the page's `#ig-diff` element holds the `Diff` JSON exactly as `ig diff --json` prints it.

- [ ] **Step 1: Write the failing tests**

In `render.rs` tests:

```rust
#[test]
fn the_diff_page_carries_view_and_diff_safely() {
    let html = diff_page(&json!({"nodes": []}), &json!({"summary": "</script>"}));
    assert!(html.contains(r#"id="ig-diff" type="application/json">{"summary":"</script>"}<"#));
    assert!(html.contains(r#"id="ig-show" type="application/json">null<"#));
    assert!(!html.contains("/*IG_DIFF*/"));
    assert!(page(&json!({}), None).contains(r#"id="ig-diff" type="application/json">null<"#));
}
```

Extend `page_fills_the_markers_in_the_viewer_and_the_placeholder` to assert `!html.contains("/*IG_DIFF*/")` for both.

In `tests/diff.rs`, on a temp git repo (two commits; the second adds a TS file under a new folder so a box is added), running the binary's `diff::run` with `DiffArgs { base: Some(first), head: Some(second), json: false, no_open: true }`:
- `diff_writes_the_head_map_and_the_diff`: `.ig/diff.html` exists; its `ig-diff` JSON has `boxes.added` naming the new box; its `ig-data` view has that box (head commit's map).
- `diff_json_writes_no_page`: with `json: true`, `.ig/diff.html` does not exist.
- `diff_with_a_head_commit_draws_that_commit`: after committing `second`, change the working tree (add another folder, uncommitted); `ig diff first second` draws a view without the uncommitted box.

- [ ] **Step 2: Run them, expect failures**

Run: `cargo test -p intent-graph diff` — Expected: FAIL (`diff_page` not found, `no_open` field missing).

- [ ] **Step 3: Implement**

`build::run` builds the head view while the head checkout still exists (it is dropped at the end of `run`). In `crates/ig/src/diff.rs`, `view_at` is `|root| match root { None => view::run(&w.env()), Some(r) => view::run(&Wired::new(r).env()) }`. Text mode: print the text as now, then `render::ignore_ig_dir`, `write_diff`, `say(quiet, "ig: wrote .ig/diff.html")`, and `render::open` unless `args.no_open`. `DiffArgs`: `#[arg(long, help = "Write the page without opening it")] pub no_open: bool`.

- [ ] **Step 4: Run tests**

Run: `cargo test -p intent-graph && cargo test -p ig-app diff` — Expected: PASS.

- [ ] **Step 5: Commit**

`git commit -m "ig diff writes .ig/diff.html: the head's map and the diff, opened unless --no-open"`

### Task 2: the viewer reads the diff

**Files:**
- Create: `viewer/src/model/diff.ts`
- Modify: `viewer/src/api/index.ts` (`IgApi.diff?(): Diff | null`), `viewer/src/api/embedded.ts`
- Test: `viewer/test/diff-model.test.ts`

**Interfaces:**
- Produces: `type Diff` mirroring the Rust `Diff` JSON field for field (`base`, `head: string | null`, `source`, `summary`, `boxes: { added: BoxRef[]; removed: BoxRef[]; changed: ChangedBox[] }`, `arrows: { added: ArrowRef[]; removed: ArrowRef[] }`, `intent`, `problems: { new: Problem[]; fixed: Problem[] }`), with `BoxRef = { id; title; parent: string | null }`, `ChangedBox = BoxRef & { files: string[]; filesChanged: number; direct: boolean }`, `ArrowRef = { from; to; kind; fromTitle; toTitle }`, `Problem = { kind: "rule" | "drift"; text; boxes: string[] }`, `Intent` as in `crates/ig-app/src/map/diff.rs`.
- Produces: `parseDiff(raw: unknown): Diff | null` — null unless `base` and `summary` are strings and `boxes`, `arrows` have their arrays; missing optional arrays become `[]`.

- [ ] **Step 1: Write the failing tests**

```ts
test("parseDiff reads ig diff --json and refuses anything else", () => {
  const d = parseDiff(sample);            // a literal copied from a real `ig diff --json`
  expect(d!.boxes.changed[0].filesChanged).toBe(28);
  expect(parseDiff(null)).toBeNull();
  expect(parseDiff({ summary: "x" })).toBeNull();
  expect(parseDiff({ ...sample, problems: undefined })!.problems).toEqual({ new: [], fixed: [] });
});
test("the embedded host reads #ig-diff, null when the page has none", () => { ... });
```

- [ ] **Step 2: Run** `cd viewer && pnpm vitest run test/diff-model.test.ts` — FAIL.
- [ ] **Step 3: Implement** `parseDiff` and `embedded().diff` (reads `#ig-diff`, `null` on missing or bad JSON, like `shown`).
- [ ] **Step 4: Run** — PASS.
- [ ] **Step 5: Commit** `git commit -m "The viewer reads the diff ig diff writes into its page"`

### Task 3: from a diff to steps and marks

**Files:**
- Modify: `viewer/src/map/answer.ts` (`Step` gains optional `group?: string; kind?: DiffKind; meta?: string`)
- Create: `viewer/src/map/diff.ts`
- Test: `viewer/test/map-diff-model.test.ts` (on `test/fixtures/shop-view.json`)

**Interfaces:**
- Consumes: `Diff` (Task 2), `Answer`, `Step` (`answer.ts`).
- Produces: `type DiffKind = "changed" | "added" | "removed" | "problem"`.
- Produces: `type Marks = { boxes: Map<string, "changed" | "added">; files: Map<string, number>; removedHere: Map<string, BoxRef[]>; addedArrows: Set<string>; ghostIds: Set<string> }` (arrow keys as `answer.ts`'s `key(from, to)`).
- Produces: `diffAnswer(view: View, d: Diff): { answer: Answer; marks: Marks; view: View }` — the returned `view` is the input plus one ghost `MapArrow` per removed arrow whose two ends are still on the map (`id: "ghost:" + from + "|" + to`, `ghost: true`, `parent` and lifted ends as the view draws arrows between those boxes); `MapArrow` in `types.ts` gains `ghost?: boolean`.

The steps, in this order (the spec's order; the card shows the group as a heading):
1. `group: "ig/"` — when `intent.status` is `changed`: one step each for added, removed, moved, reworded (`changed`), arrows (added + removed), skipping empty ones; title as on the design ("13 boxes declared", "4 boxes dropped", "25 boxes reworded", "Arrows", meta "39 declared, 10 dropped"). When `added`, `removed` or `baseBroken`: one step with `intent.note` as title.
2. `group: "New problems"` — one step per `problems.new`, `kind: "problem"`, boxes = its boxes on the map.
3. `group: "Boxes"` — a box gets its own step unless an ancestor is also changed, added or removed; changed first by `filesChanged` descending, then added, then removed. Meta: "75 files · + Mcp, + Tests" (files and the folded children). A removed box's step targets its nearest ancestor still on the map.
4. `group: "Arrows"` — added arrows grouped by `from` (one step per source, title "Ig wire → 5 boxes" when more than one, "Intent graph → Ig fs" when one), then removed arrows the same way; groups in each by size descending.

`answer.boxes` is every box the diff touches (changed, added, and the ancestors holding removed boxes); `answer.edges` the added arrows and ghost ids; `answer.title` "What this change does".

- [ ] **Step 1: Write the failing tests** (shop fixture ids; build small diffs inline):
- `steps follow ig/, problems, boxes by size, then arrows`: a diff with one intent change, one problem, changed A (3 files), changed B (10 files), added C, removed D, two added arrows from A → step groups in order `["ig/", "New problems", "Boxes", "Boxes", "Boxes", "Boxes", "Arrows"]`, B before A.
- `a change inside a changed box folds into its parent's step`: changed parent P and its child K → one Boxes step for P with meta naming K's count.
- `a removed box is counted on the nearest ancestor on the map`: removed `x/y/z` with `x/y` also removed, `x` present → `marks.removedHere.get("x")` has both.
- `a removed arrow with both ends present becomes a ghost, one with an end gone does not`: → one `ghost: true` arrow in the returned view.
- `arrows between nested boxes match the drawn arrow`: an added arrow between children of two different parents marks the lifted view arrow (via `parts`) as added.
- `no change: no steps, nothing marked`: empty diff → `answer.steps.length === 0`, `marks.boxes.size === 0`.

- [ ] **Step 2: Run** `pnpm vitest run test/map-diff-model.test.ts` — FAIL.
- [ ] **Step 3: Implement** in `viewer/src/map/diff.ts`, reusing `parents`/`line` from `answer.ts` (export them).
- [ ] **Step 4: Run** — PASS, and `pnpm vitest run` all green.
- [ ] **Step 5: Commit** `git commit -m "A diff becomes the map's steps and marks: ig/ first, then problems, boxes by size, arrows"`

### Task 4: the diff on the map, direction A

**Files:**
- Modify: `viewer/src/map/Map.svelte` (prop `diff?: Diff | null`; when set it wins over `show`)
- Modify: `viewer/src/map/AnswerCard.svelte` (group headings, kind glyph, meta, an optional `sub` line)
- Modify: `viewer/src/map/MapBox.svelte` or the `.slot` in `Map.svelte` (classes and chips)
- Modify: `viewer/src/map/looks.ts` (ghost and added arrow looks)
- Modify: `viewer/src/components/App.svelte` (`diff={host.diff?.() ?? null}`)
- Test: `viewer/test/map-diff.test.ts`

**Interfaces:**
- Consumes: `diffAnswer` (Task 3), `parseDiff` (Task 2).

What the design's board A fixes:
- Changed box: 2px outline `--color-changed`, chip "N files" (text `--color-changed` on its 15% mix). Added box: outline and chip "new" in `--color-added`. Untouched boxes fade as answers do (`.faded`).
- "−N removed here" tag on `removedHere` boxes: `--color-removed` text, 1px dashed border, on the box's bottom edge; in the current step's text the removed titles are listed.
- Added arrows: `--color-added`, solid. Ghost arrows: `--color-removed`, dash `5 4`.
- Card: title "What this change does", a `sub` line "`159c1b6 → fc0a6a6` · <summary>" (head `working tree` when null), then a muted line "ig/ unchanged · no new problems" when those groups are empty; group headings in small caps; each row: number, kind glyph (`~` changed, `+` added, `−` removed, `!` problem) in its colour, title, muted meta; ‹ › and "← → step · Esc clears" as answers.
- A legend under the map (bottom-left): changed, added, removed counted on its parent, new arrow, removed arrow.
- No change: no card steps; the card reads "No change to the architecture" and the `sub` line; nothing fades.

- [ ] **Step 1: Write the failing tests** (mount `Map` with the shop fixture and a crafted `Diff`, as `map-show.test.ts` does):
- `a diff marks changed and added boxes and fades the rest`: classes `diff-changed`, `diff-added`, `faded` on the right `[data-box]` slots; chip text "3 files".
- `the card walks the steps with the arrow keys`: card `[aria-label="What this change does"]` lists group headings in order; `ArrowRight` moves `aria-current` to step 2.
- `removed boxes show on their parent`: the parent slot contains "−2 removed here".
- `a ghost arrow is drawn dashed`: a `[data-arrow^="ghost:"]` element exists and its path has a `stroke-dasharray`.
- `no change shows the plain map`: no `.faded`, card text "No change to the architecture".
- `a diff wins over a show`: with both props, the card is the diff's.

- [ ] **Step 2: Run** `pnpm vitest run test/map-diff.test.ts` — FAIL.
- [ ] **Step 3: Implement.** Keep `AnswerCard` one component: the new fields are optional and absent for `ig_show` answers, so `map-show.test.ts` stays green unchanged.
- [ ] **Step 4: Run** `pnpm vitest run && pnpm run build` — PASS, and the build's inline-safe check passes.
- [ ] **Step 5: Look at it.** `make install`, then in this repo `ig diff 159c1b6 fc0a6a6` and check the page against board A in both themes. Note differences for the user rather than guessing.
- [ ] **Step 6: Commit** `git commit -m "The diff on the map: changed, added and removed told apart, the rest faded, a card that walks the change"`

### Task 5: CI keeps the page

**Files:**
- Modify: `.gitlab-ci.yml` (`ig-diff` job)

- [ ] **Step 1:** replace `ig view --no-open` and `ig-view.html` with `./ig-x86_64-linux diff --no-open "$CI_MERGE_REQUEST_DIFF_BASE_SHA" HEAD` writing `.ig/diff.html`, kept as artifact `ig-diff.html` (`cp .ig/diff.html ig-diff.html`); keep `ig-diff.json`.
- [ ] **Step 2:** `ruby -ryaml -e 'YAML.load_file(".gitlab-ci.yml")'` parses.
- [ ] **Step 3: Commit** `git commit -m "GitLab: the ig-diff job keeps the diff page"`
