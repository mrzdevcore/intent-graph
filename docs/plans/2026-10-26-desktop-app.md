# Desktop App Implementation Plan (Design workspace, plan 2 of 3)

**Goal:** A Tauri desktop app for ig, a cockpit next to the IDE. It has a file tree, the graph views, a read-only code view, and a custom Claude chat driven over ACP. It runs on the user's own Claude login and starts from a "trust this project?" screen.

**Architecture:**
- **Backend:** the app lives in `crates/ig-desktop` (a Tauri 2 binary). It links `ig-app` plus the adapters (`ig-fs`, `ig-git`, `ig-checkers`, `ig-lang-ts`), so the app needs no running `ig serve`.
- **Frontend:** the existing TypeScript viewer (`viewer/`), moved to Svelte 5 in Task 0, gains an **API client module**. It has three implementations, embedded data, HTTP (`ig serve`) and Tauri `invoke`, so the same screens run in the browser and in the app.
- **Claude:** Claude is the user's real Claude Code. Rust spawns the `claude-agent-acp` adapter as a sidecar and speaks ACP to it over stdio, using the `agent-client-protocol` crate. Rust owns sessions, permission decisions and policy, and forwards typed events to the webview.

**Tech Stack:** Rust 2024, Tauri 2, the `agent-client-protocol` crate, `@agentclientprotocol/claude-agent-acp`, TypeScript, **Svelte 5** (replacing Lit in Task 0), Vite, bun, and CodeMirror 6.

**Spec:** `docs/specs/2026-10-25-ig-design-workspace-design.md` (sections "The app" and "Agent and login"). This plan builds on the hexagonal workspace (`docs/plans/2026-10-26-hexagonal-workspace.md`), which must be merged first.

**Evidence:** the ACP spike of 2026-10-26, in a scratch folder at `acp-spike/REPORT.md`. Its facts are summarised under Global Constraints.

## Global Constraints

- **Login belongs to the agent, never to ig.**
  - Never read, store, log or pass on Claude credentials or tokens.
  - Never modify the Claude Code binary or the adapter.
  - When `_auth/status_update` reports `kind: "none"`, show a "Run `claude` and `/login` in a terminal, then retry" screen.
  - No API-key entry UI in this plan.
- **Settings policy (decided with the user): project settings plus ig only.**
  - Every session is created with the ig plugin passed explicitly: `_meta.claudeCode.options.plugins = [{type: "local", path: "<project>/.claude/skills/ig"}]`. The spike verified that this makes `ig:setup`, `ig:explore`, `ig:plan`, `ig:apply` and `ig:view` available.
  - The adapter hard-codes `settingSources: ["user","project","local"]`. Task 6 must verify whether `_meta.claudeCode.options.settingSources: ["project","local"]` overrides it. If it doesn't, the app documents that user settings also apply, and the plan records it.
- **A trust gate before any session.** The adapter has none: project hooks and `AGENTS.md` apply at once. The app shows its own trust screen per project first, listing what will run (hooks from `.claude/settings*.json`, `AGENTS.md`/`CLAUDE.md`, MCP servers, the ig plugin). The decision is stored in the app's data dir and keyed by the project's canonical path plus a hash of those files. If any of them change, the app asks again.
- **No distribution.** The app is built locally (`make app`). There is no signing, notarization or auto-update in this plan, pending Anthropic's written confirmation (see the spec).
- **Naming:** the product may say "runs Claude Code" in plain text, but never uses Claude/Anthropic names or logos in its own name or icon. The icon is the ig owl (`assets/brand/icon.svg`, added with the brand export).
- **Facts from the ACP spike** (adapter `@agentclientprotocol/claude-agent-acp@0.95.0`, sdk `2.0.2`, protocol 1):
  - **Session modes:** Manual (default), acceptEdits, plan, auto and bypassPermissions. `configOptions` cover model, effort and fast mode.
  - **Permission request:** `toolCall` (with diff content) plus options `allow-once`, `allow-with-updates` and `reject`. A reject ends the tool call with status `failed`.
  - **Events:** `agent_message_chunk`, `tool_call`, `tool_call_update` (diff `{path, oldText, newText}`), `usage_update` (cost and rate limits), `session_info_update`, `available_commands_update`. The `plan` event was not observed.
  - **Session control:** `session/cancel` stops a prompt in about 10 ms. `session/list` works once a session has had its first prompt; `load` and `resume` are advertised but untested.
- No new runtime services. `ig serve` and `ig view` keep working unchanged.
- Commits: plain messages, no AI attribution, explicit paths, never push. Never stage the screenshot files at the repo root.
- `make test`, `cargo clippy --workspace --all-targets -- -D warnings` with 0 errors, and `bun run typecheck` must all stay green.

## Review Focus

1. **Node or the adapter is missing.** `node` not on PATH, or the adapter not installed: the chat panel shows the exact install command and the rest of the app works. *(Task 6)*
2. **An untrusted project never starts a session,** even through "Plan this" or a deep link, and a changed hook re-prompts. *(Task 5)*
3. **Concurrent permission requests** from sub-agents (`parent_tool_use_id`) are each answered exactly once, and a closed window rejects any pending ones. *(Task 6)*
4. **The adapter crashes mid-turn:** the session shows "agent stopped", offers a restart, and keeps the transcript. *(Task 6)*
5. **A large repo** (the size of shop or bigshop): the file tree and graph load lazily, with the first paint in under 2 s. *(Tasks 3–4)*

---

## File Structure

- `crates/ig-desktop/`: `Cargo.toml`, `tauri.conf.json`, `build.rs`, `icons/`, and `src/{main.rs, project.rs, trust.rs, agent/{mod.rs, session.rs, events.rs, permissions.rs}, commands.rs}`.
- `viewer/src/api/`: `index.ts` (`IgApi` interface), `embedded.ts`, `http.ts` and `tauri.ts`.
- `viewer/src/app/`: the desktop shell components, all Svelte: `Desktop.svelte`, `Files.svelte`, `Code.svelte`, `Chat.svelte`, `Trust.svelte`, `Login.svelte`.
- `Makefile`: `make app` (viewer build, then `cargo tauri build` for ig-desktop) and `make app-dev`.

---

### Task 0: Move the viewer components to Svelte 5 (parity port; decided with the user on 2026-10-26)

**Why:** the coming UI is app-like and stateful: chat streaming, cards, approvals, trust and login screens, and later the design canvases. Svelte 5's runes and components handle that with far less ceremony than Lit. Svelte also compiles away, so the single-file offline `ig view` stays small, and it pairs well with Tauri.

- **Unchanged:** `viewer/src/model/*` (pure TypeScript: call chain, architecture rows, blueprint, tree, icons, notes, graph) and its Vitest tests. Also `viewer/src/data.ts`, the build output contract (one self-contained `viewer/dist/index.html` with the `/*IG_DATA*/` and `/*IG_LIVE*/` markers), the `ig-inline-safe` guard and the bun/Vite toolchain.
- **Rewritten:** the Lit components in `viewer/src/components/` become Svelte 5 components in `viewer/src/components/*.svelte`, using runes (`$state`, `$derived`, `$effect`). Remove the `lit` dependency. Add `svelte` and `@sveltejs/vite-plugin-svelte`, pinned exactly. `svelte-check` joins `bun run typecheck`.
- **Graph canvas:** Cytoscape stays. It is mounted in an `$effect` with explicit setup and teardown. Its event handlers and the `applyLinks` and architecture-row placement code stay in the model modules.
- **Parity is the acceptance bar,** as in the Lit port. List every feature first, then compare the old Lit page and the new Svelte page side by side in headless Chrome, light and dark, on the same fixture data, and record each item in the report. That covers: modes, drill-down and breadcrumb, Focus, Blueprint, search, the tree with its filter, the side panel with code diffs and "Open wide", Calls down with depth, the overlay, the plan panel with notes and pins, design interactions (comment, drag to row, shift-link, remove, "+"), the Links toggle, icons, themes, live reload and editor links.
- **Bundle size:** report the size change of `viewer/dist/index.html`; it should not grow.

- [ ] **Step 1:** Write the parity checklist from the current components, and capture headless screenshots of the Lit build as the baseline.
- [ ] **Step 2:** Port the components one at a time, in this order: app shell, tree, side panel, overlay, plan panel, then graph. The Vitest model tests must stay green throughout.
- [ ] **Step 3:** `make test`, `bun run typecheck` (tsc plus svelte-check), and the side-by-side parity run.
- [ ] **Step 4:** Commit: `Viewer components to Svelte 5 (parity port)`

### Task 0b: Make ig-app ready for a second host (from the refactor's final review)

- **Typed errors:** `ig_app::Error`, a `thiserror` enum covering ConfigUnreadable, BaselineUnreadable, HeadRootMissing, NoApplyInProgress, StaleApply, PlanParse, Port(PortError) and Other(anyhow::Error) for the rest. Use cases return `Result<_, ig_app::Error>`. The binary keeps printing exactly the same text: `Display` must match today's messages, and the golden and byte-comparison tests prove it. The Tauri side can then match on variants instead of strings.
- **Thread safety:** add `Send + Sync` bounds to `Vcs` and `CheckerRunner` (and check `Files` and `LanguageFrontend`), so an `Env` can live in Tauri state across threads.
- **Domain logic out of the binary:** move the `diff --json` shaping (`crates/ig/src/main.rs` around lines 135–152) and `mcp::query` into ig-app use cases (`diff_json`, `query`), so that MCP, serve and the desktop app share them.
- **Tests:**
  - an app test asserting the `Display` of each new variant equals the old message;
  - a compile-time test that `Env` is `Send + Sync`;
  - the existing MCP tests unchanged.
- **Commit:** `ig-app: typed errors, Send+Sync ports, diff and query use cases`.

### Task 1: The API client module

**Files:** `viewer/src/api/*`, `viewer/src/main.ts`, `viewer/src/data.ts`, and tests in `viewer/test/api.test.ts`.

**Interfaces:**

```ts
export interface IgApi {
  viewData(opts?: { plan?: string; noPlan?: boolean }): Promise<ViewData>;
  onRebuild(cb: () => void): () => void;           // unsubscribe
  notes?: NotesApi;                                 // present when the backend supports notes (serve, desktop)
}
export function pickApi(): IgApi;                   // embedded when window has inline data; tauri when window.__TAURI_INTERNALS__; else http
```

- `embedded` reads the inline JSON, as today.
- `http` calls the `ig serve` routes. Use the existing `/api/*` and SSE; add `GET /api/view` returning `view::data` JSON.
- `tauri` calls `invoke("view_data")` and listens to the `rebuild` event.

- [ ] **Step 1: Write the failing Vitest tests.** `pickApi` returns embedded when inline data exists. The `http` implementation calls `/api/view` (mock `fetch`).
- [ ] **Step 2: Run them.** Expected: FAIL.
- [ ] **Step 3: Implement.** Route every data read in `viewer/src` through `IgApi`. Add `GET /api/view` to `crates/ig/src/serve.rs`.
- [ ] **Step 4: Verify.** `make test` and a manual check that `ig view` and `ig serve` behave as before.
- [ ] **Step 5: Commit:** `Viewer: API client module (embedded, http, tauri)`

### Task 2: The Tauri skeleton

**Files:** `crates/ig-desktop/*` and the `Makefile`.

- Tauri 2. Pin the exact version at implementation time (latest 2.x). The window loads the built `viewer/dist`.
- **CSP:** strict, with no remote content, and Tauri capabilities limited to the commands defined in Tasks 3–6.
- **Opening a project:** a native folder picker plus a recent-projects list stored in the app data dir. The project root must be a git repo or contain an `ig.yaml`; otherwise show "not an ig project: run `ig init`".
- **Command:** `view_data(plan?: String, no_plan: bool) -> serde_json::Value`. It calls the `ig-app` use case through `ig-fs`, `ig-git` and `ig-lang-ts`.
- **Live rebuilds:** a `notify` watcher triggers a debounced rebuild (same rules as `ig serve`) and emits `rebuild`.

- [ ] **Step 1: Write the failing Rust test.** `commands::view_data_for(root)` on a scratch copy of `crates/ig/tests/fixtures/monorepo` returns JSON with `nodes`.
- [ ] **Step 2: Run it.** Expected: FAIL.
- [ ] **Step 3: Implement** the skeleton.
- [ ] **Step 4: Verify.** `make app-dev` opens the monorepo fixture and shows the graph. Record a screenshot. Then `make test` and clippy.
- [ ] **Step 5: Commit:** `ig-desktop: Tauri shell that opens a project and shows the graph`

### Task 3: The file tree

- **Command:** `file_tree() -> Vec<Entry {path, kind: file|dir, ignored: bool}>`. It lists every file, not only graph modules.
  - It respects `.gitignore`: use `git ls-files -co --exclude-standard` through a new `Vcs::list_files` port method (record the port change), with a fallback walk outside git.
  - Directories load lazily: the command takes `dir: Option<String>`.
- **UI:** `ig-files` shows the tree with filter and keyboard navigation. A file that is in the graph is linked both ways with graph selection: select in one, highlight in the other.
- **Test:** on the monorepo fixture, `file_tree(None)` includes `ig.yaml` and `README.md` (non-graph files) and excludes anything gitignored.
- **Review Focus 5:** a test with 20,000 synthetic files where the first `file_tree(None)` returns in under 200 ms.

**Steps:**
- [ ] Write the tests and confirm they fail.
- [ ] Implement.
- [ ] Verify, then commit `ig-desktop: file tree`.

### Task 4: The code view (CodeMirror 6, read-only)

- **Command:** `read_file(path) -> {text, language}`, with the same path rules as the API: repo-relative, no `..`, no absolute paths. Rejected paths return an error.
- **UI:** `ig-code` uses CodeMirror 6 with `@codemirror/lang-javascript` (TS/JSX), `lang-css`, `lang-html` (Vue as HTML for now) and `lang-json`. Plus:
  - read-only mode;
  - line numbers and folding;
  - a merge view (`@codemirror/merge`) for the changed nodes' before/after, using the diff data the viewer already has;
  - clicking a function name that is a graph node selects it in the graph.
- **"Open in editor"** keeps working through the existing editor link setting.

**Steps:**
- [ ] Vitest: the language is chosen by extension.
- [ ] Rust test: `read_file("../x")` returns an error.
- [ ] Implement.
- [ ] Verify, then commit `ig-desktop: read-only code view with diffs`.

### Task 5: The trust gate

- **Rust (`trust.rs`):**
  - `pub fn inventory(root) -> Inventory { hooks: Vec<HookSummary>, instructions: Vec<String> /* AGENTS.md, CLAUDE.md paths */, mcp: Vec<String>, ig_plugin: bool, hash: String }`.
  - `pub fn is_trusted(root, inv) -> bool` and `pub fn trust(root, inv)`. Decisions are stored as JSON in the app data dir, keyed by canonical root.
- **UI:** `ig-trust` shows the inventory in plain words: "these hooks will run after each Claude turn: `ig hook claude`". It has two buttons, Trust and Cancel.
- **Wiring:** every agent entry point calls `is_trusted` first (Review Focus 2).
- **Tests:**
  - changing `.claude/settings.json` changes the hash, so `is_trusted` becomes false;
  - `agent::start_session` on an untrusted project returns `Err(NotTrusted)`.

**Steps:**
- [ ] Write the tests and confirm they fail.
- [ ] Implement.
- [ ] Verify, then commit `ig-desktop: per-project trust gate`.

### Task 6: The agent backend (ACP over stdio)

- **Crate:** `agent-client-protocol`. Pin the version, and confirm it targets protocol version 1 and the same message shapes as the spike.
- **Sidecar:** spawn `npx --no-install claude-agent-acp`, or the resolved `node_modules/.bin/claude-agent-acp` path. Install it into the app data dir on first use, after the user confirms. Pin the adapter version (0.95.0 or later, tested). The working directory is the project root.
- **Sessions (`session.rs`):**
  - `start(root, feature: Option<slug>)` calls `session/new` with `_meta.claudeCode.options = { plugins: [ig plugin], settingSources: ["project","local"] /* verify; see Global Constraints */ }`.
  - `prompt(text, attachments)`, `cancel`, `list`, `load`.
  - The map from feature slug to session id is stored in `.ig/sessions.json`, which is git-ignored by `.ig/*`.
- **Permissions (`permissions.rs`):**
  - `session/request_permission` is forwarded to the UI as a typed event carrying `requestId`, `toolCall` (including the diff) and `options`.
  - The UI answers with `optionId`. Each request is answered exactly once.
  - Requests still pending when the window closes are answered `reject` (Review Focus 3).
  - Answers keep the adapter's own option ids.
- **Events (`events.rs`):** each ACP `session/update` maps to a serde enum emitted to the webview as `agent-event`. The variants are MessageChunk, ToolCall, ToolCallUpdate, Usage, Info, Commands, Plan, Auth and Stopped.
- **Errors and lifecycle:**
  - Adapter crash: emit `Stopped{reason}`, keep the transcript, offer a restart (Review Focus 4).
  - No node or adapter: emit `Setup{instructions}` (Review Focus 1).
  - `_auth/status_update` with `kind: "none"`: emit `Auth{needed: true}`.
- **Tests:** a **fake agent**, a small Rust ACP agent compiled as a test binary that replays a scripted session: initialize, then new, then a streamed message, then a permission request, then a tool_call_update with status failed after reject.
  - Unit tests drive `session.rs` against it: streaming order; permission round trip; cancel; crash, which kills the fake and must produce `Stopped`; and two concurrent permission requests.
  - The real adapter is only used in the manual check.

**Steps:**
- [ ] Write the tests with the fake agent and confirm they fail.
- [ ] Implement.
- [ ] Verify by hand with the real adapter on a scratch project:
  - `/ig:explore what is in this repo?` streams;
  - a write request gets rejected;
  - cancel works;
  - record whether the `settingSources` override took effect (check whether user-only skills appear in `available_commands_update`).
- [ ] Commit: `ig-desktop: ACP agent backend with sessions, permissions and events`.

### Task 7: The chat panel

- **`Chat.svelte`, docked on the right, renders:**
  - streaming markdown text;
  - tool cards, collapsed, showing the title, a status chip and the location;
  - diff cards, through the CodeMirror merge view, with an "open in code view" action;
  - permission requests as an inline card with the adapter's options as buttons ("Yes", "Yes, allow all edits this session", "No");
  - `AskUserQuestion` as option buttons;
  - a usage meter (cost and rate-limit use) from `usage_update`;
  - a session title and a session switcher (per feature);
  - mode and model selectors from `configOptions`;
  - the login screen (`ig-login`) when `Auth{needed}` arrives;
  - a setup screen for a missing node or adapter.
- **Context from the views:** "Attach selection" adds the selected graph node(s) or file(s) to the next message as `embeddedContext` resources (`file://` with the path, plus the node id in the text). "Plan this" on a selection sends `/ig:plan <selection summary>`.
- **Composer:** `/` autocompletes from `available_commands_update` (ig commands first). Enter sends, Shift+Enter adds a newline, Esc cancels a running turn.
- **Tests:** Vitest on a pure reducer `chatReducer(state, event) -> state`, using a recorded event stream from Task 6's fake agent. Chunks concatenate, tool updates merge by id, a permission request becomes a pending card, and a reject resolves it.
- **Accessibility:** buttons are real `<button>` elements, live regions announce streamed text politely, and focus moves to permission cards.

**Steps:**
- [ ] Write the tests and confirm they fail.
- [ ] Implement.
- [ ] Verify by hand in `make app-dev` on a real project; record screenshots.
- [ ] Commit: `ig-desktop: chat panel`.

### Task 8: Docs and the local build

- **README:** a "Desktop app (local build)" section covering prerequisites (node and the Claude CLI logged in), `make app`, the trust screen, the settings policy, and that it isn't distributed.
- **`docs/specs/design.md`:** a desktop section.
- **`make app`** produces the `.app` bundle on macOS. Do a clean build from a fresh clone and record it.
- **Commit:** `Desktop app docs and build`.
