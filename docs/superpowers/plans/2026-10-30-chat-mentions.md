# @ mentions in the chat Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** In the desktop app, the user points Claude at map boxes (`@` in the message box, or "Add to chat" on the selected box); Claude gets each box's place, files and arrows, and the boxes it names in replies light up and fly to on the map.

**Architecture:** A pure module (`model/mentions.ts`) turns box ids into the context Claude receives and matches Claude's `@Title` refs back to ids. The Composer completes `@` with the map search (`map/search.ts` `find`) and keeps the picked boxes as chips; the session sends the user's text to the transcript and text plus context to Claude. `App` owns the loaded view and wires Map ↔ Chat: Map gets `lit` and `onmention`, Chat gets `view` and exposes `mention()`.

**Tech Stack:** Svelte 5 (runes), TypeScript, Vitest with happy-dom, bun.

**Spec:** `docs/superpowers/specs/2026-10-29-ig-design-mode-design.md`, section "1. @ mentions: boxes in the chat". Design: the "@ mentions in the chat" artboard of https://claude.ai/artifact/Wny722GMWPs6bRBu7KZy9k.

## Global Constraints

- Styles use design tokens only: colours `var(--color-*)` / `var(--map-*)` from `viewer/src/styles/themes/*.css`, sizes from `viewer/src/styles/tokens.css`, rem not px (hairline borders excepted). No raw hex in components; a new colour role gets a token in every theme file.
- Map and repo text is rendered as text, never `{@html}`. Claude's markdown keeps going through `renderMarkdown` (sanitized) and `linkNodes` (DOM nodes only).
- Chips use the gold of the trace token: `--map-trace` for border/text and `color-mix(in srgb, var(--map-trace) 15%, transparent)` for the fill.
- Run viewer checks from `viewer/`: `bun run test`, `npx svelte-check --threshold warning`, `bun run build`.

## Review Focus

- Two boxes with the same title (the fixture has two "Stripe" and several "Src"): picking both sends both; Claude's `@Stripe` then links to neither (ambiguous), while `` `api/infra-stripe` `` (an id) still links.
- An `@` inside a word (`a@b.com`) opens no completion; only `@` at the start or after whitespace does.
- The `/` command completion keeps working exactly as before when no `@` token is being typed.
- No view (the `ig view` page has no chat; a desktop map that failed to load): the chat sends normally, `@` lists nothing, nothing throws.
- The transcript shows exactly what the user typed, never the context block sent to Claude.

---

### Task 1: The mentions model

**Files:**
- Create: `viewer/src/model/mentions.ts`
- Test: `viewer/test/mentions.test.ts`

**Interfaces:**
- Consumes: `View`, `MapBox` from `viewer/src/map/types.ts`.
- Produces:
  - `export type Mention = { id: string; title: string }`
  - `export function boxContext(view: View, ids: string[]): string` — `""` when `ids` is empty or none is in the view.
  - `export function refMatcher(view: View): (text: string, code?: boolean) => string | null`

- [ ] **Step 1: Write the failing tests** (fixture: `test/fixtures/shop-view.json`)

```ts
test("the context names each box, where it sits, its files and arrows, and asks for @refs", () => {
  const c = boxContext(view, ["api/application-refund"]);
  expect(c.startsWith("<ig-boxes>")).toBe(true);
  expect(c.trim().endsWith("</ig-boxes>")).toBe(true);
  expect(c).toContain("@Refund (id `api/application-refund`): usecase, in Api");
  expect(c).toContain("Files: apps/api/src/application/refund/**");
  expect(c).toContain("Uses: Ports (depends), Stripe (calls refund)");
  expect(c).toContain("Used by: Routers (1 call · 1 import)");
  expect(c).toContain("`@Title`");
  expect(boxContext(view, [])).toBe("");
  expect(boxContext(view, ["no-such-box"])).toBe("");
});

test("two boxes with one title are both sent, each with its id", () => {
  const c = boxContext(view, ["stripe", "api/infra-stripe"]);
  expect(c).toContain("(id `stripe`)");
  expect(c).toContain("(id `api/infra-stripe`)");
});

test("Claude's refs: a unique @Title in code, or an id; ambiguous titles stay text", () => {
  const m = refMatcher(view);
  expect(m("@Refund", true)).toBe("api/application-refund");
  expect(m("api/infra-stripe", true)).toBe("api/infra-stripe");
  expect(m("@Stripe", true)).toBeNull();
  expect(m("@Refund")).toBeNull(); // only inline code is a ref, not plain words
  expect(m("@Nope", true)).toBeNull();
});
```

- [ ] **Step 2: Run to verify they fail**

Run: `cd viewer && bun run test test/mentions.test.ts`
Expected: FAIL, module `../src/model/mentions` not found.

- [ ] **Step 3: Implement `boxContext` and `refMatcher` in `viewer/src/model/mentions.ts`**

`boxContext` output, exactly this shape (one line per box, in `ids` order; "in" is the parents' titles joined by " › ", system box excluded, "the system" when top level; Files is `madeOf` joined by ", " or "none"; Uses / Used by list the other end's title and the arrow's `label ?? kind`, joined by ", ", lines omitted when empty):

```
<ig-boxes>
The user mentioned these boxes of ig's map. When you name one, write it as `@Title` in inline code (or its id when titles repeat).
- @Refund (id `api/application-refund`): usecase, in Api. Files: apps/api/src/application/refund/**. Uses: Ports (depends), Stripe (calls refund). Used by: Routers (1 call · 1 import).
</ig-boxes>
```

`refMatcher`: with `code` true, `@Title` maps to the id of the one box with that title (null if none or several), and a bare exact id maps to itself; with `code` false, always null (plain words are never refs, so prose like "@here" stays text).

- [ ] **Step 4: Run to verify they pass**

Run: `cd viewer && bun run test test/mentions.test.ts`
Expected: PASS (3 tests).

- [ ] **Step 5: Commit**

```bash
git add viewer/src/model/mentions.ts viewer/test/mentions.test.ts
git commit -m "Chat mentions: the context Claude gets for a box, and its refs back"
```

### Task 2: The session sends context Claude sees and the transcript does not

**Files:**
- Modify: `viewer/src/components/chat/session.svelte.ts` (`send`, around line 195)
- Test: `viewer/test/session.test.ts`

**Interfaces:**
- Produces: `ChatSession.send(text: string, files: string[] = [], context = ""): Promise<boolean>` — dispatches `Sent` with `text`; calls `agent.prompt(context ? text + "\n\n" + context : text, files)`.

- [ ] **Step 1: Write the failing test** (with the file's `fake()` helper, which records `"prompt " + text`)

```ts
test("context goes to the agent, not to the transcript", async () => {
  const f = fake({ trusted: true });
  const chat = new ChatSession(f.agent, f.inv, f.trust);
  await chat.send("Move @Refund", [], "<ig-boxes>\n…\n</ig-boxes>");
  expect(f.prompts().at(-1)).toBe("prompt Move @Refund\n\n<ig-boxes>\n…\n</ig-boxes>");
  expect(chat.s.items.find((i) => i.kind === "text" && i.role === "user")).toMatchObject({ text: "Move @Refund" });
});
```

- [ ] **Step 2: Run to verify it fails**

Run: `cd viewer && bun run test test/session.test.ts`
Expected: FAIL, the prompt lacks the context.

- [ ] **Step 3: Add the `context` parameter to `send`** as in Interfaces.

- [ ] **Step 4: Run to verify it passes**

Run: `cd viewer && bun run test test/session.test.ts`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add viewer/src/components/chat/session.svelte.ts viewer/test/session.test.ts
git commit -m "Chat: a turn can carry context the transcript does not show"
```

### Task 3: `@` completion and chips in the Composer

**Files:**
- Modify: `viewer/src/components/chat/Composer.svelte`
- Test: `viewer/test/composer.test.ts` (new; mount with happy-dom like `test/map-canvas.test.ts`)

**Interfaces:**
- Consumes: `find(view, q, limit)` from `viewer/src/map/search.ts`; `Mention` from Task 1.
- Produces: Composer props gain `view?: View` and `mentions = $bindable<Mention[]>([])`; `onsend` becomes `(text: string, mentions: Mention[]) => void`. After a send, `mentions` is emptied.

Behaviour: the `@` token is the text from the last `@` that sits at the start of the text or after whitespace, up to the caret, with no whitespace in it. While there is one and `view` is set, the list shows `find(view, token, 8)` (title, then kind · where in muted text) in place of the command list. ↑/↓ move, Enter or Tab picks, Esc closes it (as for commands). A pick replaces `@token` with `@Title ` and appends `{ id, title }` to `mentions` (a box already picked is not added twice). Chips sit above the textarea, one per mention, each with a "Remove" × button; the chips are what is sent (editing the text does not change them).

- [ ] **Step 1: Write the failing tests**

```ts
test("@ completes boxes; a pick becomes text and a chip, and both are sent", () => {
  // mount Composer with { commands: [], running: false, view: shop, onsend }
  type("Move @refu");
  expect(list()).toContain("Refund");
  key("Enter");
  expect(textarea().value).toBe("Move @Refund ");
  expect(chips()).toEqual(["@Refund"]);
  key("Enter");
  expect(onsend).toHaveBeenCalledWith("Move @Refund", [{ id: "api/application-refund", title: "Refund" }]);
  expect(chips()).toEqual([]);
});

test("an @ inside a word opens nothing; / commands still complete", () => {
  type("mail a@b.co");
  expect(list()).toEqual([]);
  // mount with commands: [{ name: "ig:plan", description: "Plan" }]
  type("/ig");
  expect(list()).toContain("/ig:plan");
});

test("a chip's × removes it; without a view the chat still sends", () => {
  // pick two boxes, click the first chip's Remove: one chip left
  // mount without view: type "@ref", list is empty; Enter sends ("@ref", [])
});
```

- [ ] **Step 2: Run to verify they fail**

Run: `cd viewer && bun run test test/composer.test.ts`
Expected: FAIL (no `@` list, `onsend` called with one argument).

- [ ] **Step 3: Implement the `@` token, list, chips and two-argument `onsend` in `Composer.svelte`**

Read the caret from the textarea's `selectionStart` on `input` and `keydown`. Keep the existing `/` logic for when no `@` token is active. Chip styles per Global Constraints; the list reuses the existing `.complete` markup and styles.

- [ ] **Step 4: Run to verify they pass**

Run: `cd viewer && bun run test test/composer.test.ts && bun run test`
Expected: PASS, and the whole suite still passes.

- [ ] **Step 5: Commit**

```bash
git add viewer/src/components/chat/Composer.svelte viewer/test/composer.test.ts
git commit -m "Chat: @ completes map boxes into chips"
```

### Task 4: Wire map and chat (send, Add to chat, Claude's refs light and fly)

**Files:**
- Modify: `viewer/src/components/App.svelte`, `viewer/src/components/chat/Chat.svelte`, `viewer/src/components/chat/Markdown.svelte`, `viewer/src/map/Map.svelte`, `viewer/src/map/DetailsCard.svelte`
- Test: `viewer/test/map-canvas.test.ts`, `viewer/test/markdown.test.ts`

**Interfaces:**
- Consumes: `boxContext`, `refMatcher`, `Mention` (Task 1); `send(text, files, context)` (Task 2); Composer `view`, `mentions`, two-argument `onsend` (Task 3).
- Produces:
  - `Map.svelte` props gain `lit?: string | null` (that box's slot gets class `lit`: a dashed `--map-trace` outline) and `onmention?: (id: string) => void`; `export function show(id: string): void` (same as clicking the box: selects it and flies there).
  - `DetailsCard.svelte` prop `onmention?: (id: string) => void`; when set, an "Add to chat" button under the meta line calls it with the box id.
  - `Markdown.svelte` prop `onhover?: (id: string | null) => void`, called on `pointerover` / `pointerout` of a `button[data-node]`.
  - `Chat.svelte` props gain `view?: View | null`, `onref?: (id: string) => void`, `onhover?: (id: string | null) => void`; `export function mention(m: Mention): void` appends `@Title ` to the draft and the chip (once).
  - `App.svelte` loads the view once into state (keeping the "Could not read the map" error) and passes: Map `lit`, `onmention` → `chatRef.mention`; Chat `view`, `onref` → `mapRef.show`, `onhover` → `lit`. On send, Chat calls `c.send(text, [], boxContext(view, mentions.map((m) => m.id)))`; Claude's replies render with `match={refMatcher(view)}`, `onnode={onref}`, `onhover`.

- [ ] **Step 1: Write the failing tests**

```ts
// map-canvas.test.ts
test("Add to chat hands the selected box to the chat; a lit box is outlined; show() opens a box", () => {
  // mount Map with props { view, camera, size, lit: "stripe", onmention }
  expect(box(target, "stripe")!.classList).toContain("lit");
  // click the api box, then the card's "Add to chat"
  expect(onmention).toHaveBeenCalledWith("api");
  m.show("postgres");
  flushSync();
  expect(target.querySelector(".details b")!.textContent).toBe("Postgres");
});

// markdown.test.ts
test("a ref button reports hover in and out", () => {
  // mount Markdown with text "see `@Refund`", match = refMatcher(shop), onhover
  // pointerover the button → onhover("api/application-refund"); pointerout → onhover(null)
});
```

- [ ] **Step 2: Run to verify they fail**

Run: `cd viewer && bun run test test/map-canvas.test.ts test/markdown.test.ts`
Expected: FAIL (no `lit` class, no "Add to chat", no `show`, no `onhover`).

- [ ] **Step 3: Implement the props, `show`, the button and the App wiring** as in Interfaces.

- [ ] **Step 4: Run all checks**

Run: `cd viewer && bun run test && npx svelte-check --threshold warning && bun run build`
Expected: all tests pass, 0 errors and 0 warnings, build succeeds.

- [ ] **Step 5: Try it in the desktop app** (`make app-dev`): type `@` and pick a box, send, check Claude's reply links the box, hover it (lit on the map), click it (flies there), and use "Add to chat" from the details card. Note anything off for the review.

- [ ] **Step 6: Commit**

```bash
git add viewer/src
git add viewer/test
git commit -m "Chat and map: Add to chat, context sent with mentions, Claude's refs light and fly"
```
