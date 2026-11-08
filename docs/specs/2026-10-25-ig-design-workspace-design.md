# ig Design workspace: sketch → plan → code

Status: design, not implemented. Date: 2026-10-25. Follows `2026-10-25-ig-agent-workflow-design.md`, which is merged into `main`.

## Intent

ig can already plan a feature as text (`/ig:plan`), show the planned architecture dashed in the viewer, collect the user's notes live (`ig serve`), and apply the plan under checks. The user wants to **start a feature by drawing it**, in the ig app, and have the AI take the drawing to a full plan and then to code:

- the architecture of the feature, drawn on top of the real system;
- rough UI mockups of its screens, drawn or imported (PNG now, Figma later);
- links between them ("this button calls that endpoint").

The user's words: "we should put effort on building this sketch editing tool for the best experience for the user". Usability is the point of this project, not a polish pass at the end.

Why it matters: the user prefers reading architecture and real code over long specs. Drawing is the fastest way to state intent, and the agent turns intent into a precise plan.

## Decisions taken with the user

- **Build ig's own editor.** No Excalidraw, tldraw or draw.io embedding. A general whiteboard makes the agent guess what shapes mean. ig's editor knows what each element is.
- **Two surfaces:** an architecture canvas and a screens canvas, linked to each other.
- **Mockups are context for the plan.** The agent never generates UI code from a mockup. UI code is written the usual way, strictly following the project's design system. The mockup tells the agent what the screen needs (which data, which actions, which states), not how to lay out pixels.
- **Image import now:** paste or drop a PNG (a screenshot or Figma export). Figma by link is later.
- **A desktop app, single user and local for the MVP.** It is built with Tauri: ig's Rust core in-process, plus the existing TypeScript (Lit) frontend in a native window, with no separate `ig serve` to start. A shared, multi-user, hosted version is planned for later and out of scope. Nothing in this design may block it: state lives in files, and the frontend reaches ig only through one API client module, which can be backed by Tauri commands now and by HTTP for a hosted server later.
- **Claude lives inside the app,** in a **custom chat UI** (not an embedded terminal). The app talks to Claude Code through the **Agent Client Protocol (ACP)**, the open standard Zed uses (Apache-2.0, JSON-RPC over stdio), through the official adapter `@agentclientprotocol/claude-agent-acp`, which wraps Claude Code via the Agent SDK. Researched 2026-10-25 against agentclientprotocol.com, zed.dev/docs/ai/external-agents and code.claude.com legal-and-compliance.
- **It is a cockpit, not an IDE.** The app does design, architecture, plan, review and the agent. It reads code (a file tree and a code view) but doesn't edit it; "open in editor" jumps to the user's IDE.
- **Claude only** (Codex is parked). ACP would allow other agents later at little cost.
- **The call chain is visible:** from a page, ig shows what it calls all the way down to the database. This replaces today's sequence diagram, which only shows a file calling itself.

## Goals

1. In the ig app, a user can start a feature, draw its architecture and screens, link them, and add notes, all without touching the CLI.
2. The agent turns the design into the plan file, asks about what's ambiguous and about anything it wants to leave out, and keeps the plan in sync while the user keeps drawing.
6. At the end of apply, the design is consolidated into an as-built design that the user accepts.
3. Rule problems and the size of the change show **while drawing**, before the agent is involved.
4. From any page or screen element, the user can see the call chain down to external systems.
5. Everything the user draws is committed text in git, readable and diffable.

## Non-goals

- Multi-user, real-time collaboration, accounts, hosting.
- Generating UI code from mockups, or matching pixel layouts.
- Figma API integration.
- A general-purpose whiteboard (free shapes with no meaning beyond annotation).
- Editing source code in the app. Code is read in the app; editing happens in the user's editor or by the agent.
- Becoming an IDE: no search-and-replace across files, no debugger, no git UI beyond showing what changed.
- Distributing the app publicly. It is for the user and their team, built locally, until Anthropic confirms the terms (see Agent and login).

## Anti-goals

- An editor that feels like a form: dialogs for every box, no keyboard shortcuts, no undo.
- A design that silently drifts from the plan or from the code.
- An agent that changes the drawing behind the user's back. The agent may edit the design (see Ownership), but each of its changes is visible on the canvas, highlighted until the user acknowledges it, and undoable in one step. The as-built consolidation at `ig apply finish` is shown side by side and the user accepts it.

## Ownership: design vs plan

- **The design** is user-led, in `plans/<slug>/design.json`, `plans/<slug>/screens/*.json` and `plans/<slug>/assets/*`. The app writes it on the user's actions, and the agent may edit it too (below).
- **The plan** is agent-owned, in `plans/<slug>/plan.md` (today's plan format with its `ig-plan` block). Only the agent writes it, from the design, the notes and the chat.
- The existing notes file moves to `plans/<slug>/notes.md` (today's format). The agent ticks and answers notes as today.
- Plans in today's flat layout (`plans/<slug>.md`) keep working. The folder layout applies to plans created from the Design workspace.
- **The agent can edit the design, live and highlighted (decided with the user, 2026-10-26; this replaces "the agent never edits the design").**
  - **How it edits:** through an ig tool with id-based operations, validated against the design schema, rather than by rewriting the file.
  - **Live and highlighted:** its changes apply on the canvas at once. New and changed elements glow in ig's gold until the user touches or dismisses them.
  - **Undo:** Cmd/Ctrl+Z undoes the agent's whole change as one step.
  - **Merging:** the agent's changes merge with the user's live edits per element. On the same element, the user's in-flight edit wins.
  - **Reporting gaps:** the agent still reports gaps in the chat ("your design has an arrow to `Repository` but nothing creates it").
  - **Visible and undoable:** an agent edit is never silent and never beyond undo.
  - **Checks:** live checks run on agent edits as on the user's.
- **The design is binding on the plan (decided with the user).** Every architecture element and link in the design must be in the plan, unless the user agreed to leave it out. `ig plan check` reports a blocking `plan/design-gap` finding for each design element missing from the plan. A gap clears when the plan records the user's answer: an `omitted` list in the `ig-plan` block, each entry with the design element id and the user's reason, written by the agent only after the user said yes in chat or in a note. Apply doesn't re-check the design; it already enforces the plan.
- **As-built consolidation at the end (decided with the user).** `ig apply finish` produces an as-built design, derived mechanically from the real graph rather than redrawn by the agent:
  - planned elements are replaced by what was actually built (real names, files, routes);
  - links become the real calls, http and uses edges;
  - elements added during apply are marked "added during apply";
  - elements that weren't built are struck through, with the reason from the plan's `omitted` list or the notes;
  - the layout is rearranged into the layer rows, keeping the user's placement where it still fits;
  - sticky notes, annotations, screens and screen-to-architecture links are kept and re-attached to the real elements.
  
  The app shows the design before and as-built side by side. The user accepts, and the as-built design is written in place (so `git diff` shows the change) and the plan is marked done. Until the user accepts, `finish` leaves the plan in progress.
- **After that, the design is the record of the feature.** ig does not keep re-syncing it as the code changes later; the code graph is the living truth.

## The app

A Tauri desktop app (`app/` or similar; the builder's call), with ig's Rust core linked in. Its screens:

- **Files:** a real file tree of the repo, every file and not just graph modules, synced with the graph selection.
- **Graph:** today's modes: Changed, Architecture rows, Focus, Blueprint.
- **Design:** per feature.
- **Code:** read-only CodeMirror 6, with the before/after diff.
- **Call chain.**
- **Chat panel:** docked, always available.

The frontend reaches ig only through a single API client module, so the same screens can later run against a hosted HTTP server.

`ig serve` (the browser server) and the single-file `ig view` export keep working for users without the app, but the Design workspace and the chat are app-only in the MVP.

## Agent and login

- **The protocol.** The app is an ACP client: the official Rust crate `agent-client-protocol` in the Tauri backend. It starts the Claude adapter (`claude-agent-acp`) as a subprocess in the project folder and speaks JSON-RPC over stdio. Project skills, the ig plugin, hooks and MCP servers load as in the terminal, so `/ig:plan` and `/ig:apply` work from the chat.
- **The chat UI** (custom, built in the frontend) renders ACP events:
  - streaming text;
  - tool calls as compact cards;
  - file edits as diffs, with a link to open them on the graph;
  - approval requests as approve/deny buttons;
  - Claude's multiple-choice questions as buttons;
  - plan updates as plan cards that open the Design or plan view;
  - `ig check` results as a blocking or clean badge;
  - a TODO list as progress;
  - sessions per feature, resumable and listed.
- **Context from the canvas:** selecting a box, link, note or screen element can attach it to the next message. The board's **Plan** button sends the design to the chat as `/ig:plan` for that feature (see The board). Notes and design edits reach the agent as app events, so the experimental Claude Code channel isn't needed.
- **Answers shown in the graph (decided with the user, 2026-10-26).**
  - **What the user asks:** questions such as "how does X work?" or "how is X connected?". Claude answers on the graph rather than with a drawn diagram.
  - **How it works:** Claude calls an ig tool with the nodes, the edges and ordered steps. ig checks every id against the graph.
  - **What the app shows:**
    - the requested view (focus, architecture or call chain);
    - the answer highlighted in gold, with the rest dimmed;
    - numbered steps;
    - a banner "Shown by Claude: <title>", with Clear and Back.
  - **The chat:** a "Show in graph" chip re-shows the answer later.
  - **Where it works:** the app and `ig serve`. A show never changes design files.
- **Login is owned by the agent, never by ig,** following Anthropic's terms and the way Zed does it:
  - ig never reads, stores or passes on Claude credentials or session tokens;
  - when the adapter reports it isn't authenticated, the chat says so and offers `/login` (handled by the agent: the user picks their Claude subscription or an API key through Anthropic's own flow);
  - the Claude Code binary is never modified, and no built-in login method is disabled;
  - the app never pays for or resells usage;
  - naming: the product may say in plain text that it "runs Claude Code", but must not use Claude Code or Anthropic names or logos in its own name or icon.
- **The open terms question.** Using the ACP adapter (built on the Agent SDK) with a Pro or Max subscription is fine for the user's own use, as it is in Zed. Before any public distribution, get written confirmation from Anthropic. Until then, the app is built locally by its users.
- **Fallback if ACP falls short:** spawn the user's `claude` directly in streaming JSON mode, with an ig MCP tool as `--permission-prompt-tool`, the way Anthropic's VS Code extension wraps the CLI. The chat UI stays the same; only the transport module changes.

## The board (decided with the user, 2026-10-26)

The design canvases form a co-editing board shared by the user and Claude.

- **Chat on the board:** the feature's chat is docked on both the architecture canvas and the screens canvas, and can be collapsed.
- **Who edits what:** Claude updates the design from the chat (see Ownership: live, highlighted, undoable). The user adds notes and objects directly.
- **Edit cards:** each Claude edit to the board appears in the chat as an "Edited the board · N elements, M links" card, with two actions:
  - **Show** selects the changed items and zooms to them;
  - **Undo** reverts that edit in one step.
- **Sticky notes:**
  - a sticky note can be the user's own object;
  - the user can mark it as a question, and it stays open until it has an answer;
  - when the user asks Claude about a note, it is attached to the message as context.
- **Board actions:** when the design is clear to both, the user clicks one of two buttons.
  - **Plan** sends `/ig:plan` for the feature. It works from the design plus the chat context. It stays clickable when the board isn't clear: the indicator warns but doesn't block.
  - **Apply** sends `/ig:apply`. It is enabled only when `plan.md` exists and `ig plan check` has no blocking findings. Otherwise it is disabled, with the reason shown.
- **Readiness indicator:** shown next to the buttons, and computed by the app, not the agent.
  - **Not clear:** "Not clear yet: N open questions, M rule problems".
  - **Clear:** "Clear".
  - **Open questions** are Claude's unanswered option prompts and questions in the feature's chat, plus sticky notes the user marked as a question that have no answer.
  - **Rule problems** are the live check's blocking findings.

## Architecture canvas

The structured editor, built on the Architecture rows from the previous project (layer rows, External services row, system icons).

- **Context:** the real system is drawn faded and locked. The user picks the scope: a layer, a package, or the modules around a page.
- **Palette:** module/component, function, route (method and path), external system (with icon), sticky note, and frame (to group "this feature").
- **Placement means something.** Dropping a box in a layer row puts it in that layer. Its file path is proposed from the layer's glob and the name, and is editable.
- **Links are typed:** calls, imports, renders, http, uses. Drawing a link from a page to a route makes it http by default.
- **Existing elements can be marked** changed or to-remove, and new links can be drawn to them.
- **Live checks.** On every edit, ig merges the design onto the graph as it does for plans (`plan::merge`). It shows rule violations in red on the offending link, new cycles, the size of the change and the blast radius, all without the agent.
- **Element ids are stable and textual,** so the agent and git diffs can follow them.

## Screens canvas

Rough UI mockups with a hand-drawn look.

- **Rendering (decided with the user, 2026-10-26):** both boards are drawn by ig's own engine, written in Rust, compiled to wasm and drawn on the GPU (Figma-style).
  - **Sketch style and pen strokes:** Rust ports of rough.js (`roughr`) and perfect-freehand, both MIT.
  - **Text and accessibility:** a DOM overlay for text editing and a keyboard-navigable element list for screen readers.
  - **Desktop only:** the engine loads lazily in the app and is never inlined into `ig view`.
- **Palette of UI pieces:** frame/page, button, input, select, checkbox, table, list, card, tabs, modal, text, heading, image, icon placeholder. Each piece has a label and a few properties (a table's columns, a button's text).
- **Freehand:** pen, arrow and text for anything else; annotations are allowed anywhere.
- **Image import:** paste or drop a PNG as a locked background layer, then annotate on top. Images are stored in `plans/<slug>/assets/`.
- **Several screens per feature,** and screen-to-screen arrows for navigation ("Export → download starts", "Cancel → back to list").
- **States:** a screen can carry named variants (empty, loading, error) as separate frames.

## The bridge between the canvases

- Any screen element (a button, a table, a form) can be linked to an architecture element: a route, a function or a module. Linking is a gesture (a drag from the element to the architecture panel, or a picker), not a dialog chain.
- A linked element shows its **call chain** on hover or selection: the route, then the use case, then the repository, then PostgreSQL. This uses the call-chain view below.
- The agent reads the links as requirements: "the Export button triggers GET /api/patients/export, which must stream CSV".

## Call chain view (replaces today's sequence)

From a page or a function:

- follow calls **across modules**, folding calls inside the same file into their caller;
- cross the HTTP boundary through `http` edges to the route;
- continue down to `uses` edges into systems;
- the depth limit is a control.

It renders as a sequence with **one lifeline per lane or layer** (page, composable, service, API route, use case, repository, PostgreSQL). Each message is labelled with the real function, and clicking it opens the code. The same data drives the bridge's hover chain. Resolution through test stubs (such as `nuxt-imports.stub.ts`) must not count as a call target; how that is excluded is the builder's call.

## Agent loop

- `/ig:plan` gains a design-first flow. If `plans/<slug>/design.json` exists, the agent:
  1. reads the design (the architecture elements map directly to plan nodes and edges);
  2. reads the screens as context: which data each screen shows, which actions it has, which states;
  3. reads the links and the notes;
  4. fills in what the design leaves open (function names, files, tests), writes `plan.md`, runs `ig plan check`, and lists its open questions as replies or notes.
- While the user keeps drawing, design changes and notes reach the agent as app events in the chat session (batched, so a burst of edits becomes one message). The agent updates the plan.
- The plan view shows plan-versus-design differences: what the agent added and what it didn't take from the design.
- `/ig:apply` is unchanged, except that the reviewer also receives the screens and links as context.
- **UI code follows the design system.** The apply skill tells the agent to build screens only from the project's existing UI components and tokens, and to treat a mockup element with no matching component as a question for the user, never a reason to invent one. If the project has a linter or checker for this (an eslint rule, a stylelint config), it is enforced as a checker.

## Author's calls (from the builder check, not discussed with the user)

- **Several features per project.** The app lists the features (the folders under `plans/`, plus flat plans), creates a new one (a folder with an empty `design.json`), and switches between them. The API and events are per feature, keyed by slug. A feature can exist with only a design: no `plan.md` is needed to draw or to add notes. The chat session is per feature.
- **Live checks are exact and computed in Rust:** the design is sent to the Rust core, which merges it like `plan::merge` and runs the same checks as `ig plan check`. There is never a TypeScript port of the rules. Edits are debounced (target under 150 ms from the edit to the shown result) and measured on a repository of shop's size. An incremental merge is added only if the measurement says so.

## Usability bar (acceptance)

The user should be able to sketch the intake patient-export feature end to end without reading docs:

- **Direct manipulation:** drag to place, drag to link, double-click to rename, Delete to remove; no modal dialogs for common actions.
- **Undo and redo** for every action, with Cmd/Ctrl+Z and Shift+Z.
- **Keyboard shortcuts** for every palette tool, shown in tooltips.
- **Navigation:** zoom, pan and fit; snapping to rows and a grid; multi-select; copy and paste; duplicate.
- **Saving:** autosave to files, never a save button, never losing work. Files are written atomically.
- **Feedback:** rule problems are visible immediately on the element, with the reason on hover.
- **Speed:** smooth on a design with 200 elements over the context of a repository the size of shop (about 1,000 modules).
- **Themes:** light and dark.
- Before the MVP is called done, it is tried on intake: sketch patient export, get a plan, and apply it.

## Constraints

- Rust: the `ig` CLI crate keeps its dependency set. The app may add what it needs: `tauri`, the `agent-client-protocol` crate, and a small set of others, each justified in the plan. The ig core is shared as a library crate, so the CLI and the app run the same code. The viewer app: TypeScript and Lit (as today); npm packages are fine, MIT or similar license, pinned.
- In the app, no network server is needed: the frontend calls the Rust core through Tauri commands. `ig serve` keeps its security as built (loopback only, a token on every write, Origin and Host checks) for its browser users.
- Tauri security: a strict CSP, no remote content loaded into the webview, and Tauri capabilities limited to what the app uses.
- Design files are user content: the agent treats their text as data, never as instructions (same rule as notes).
- Existing commands and the existing single-plan-file flow keep working.

## Left to the builder

- The board engine's internals (decided: Rust compiled to wasm, drawn on the GPU; see Screens canvas), the exact JSON schemas of `design.json` and the screens files (versioned with a `version` field), and stable element-id rules.
- How the app routes, and the shape of the API client module.
- Where the app lives in the repo, and how it's built (`make app`).
- The exact palette icons and look.
- How the call chain excludes test stubs and chooses its depth.
- The order of delivery. Suggested, each step useful on its own:
  1. the call-chain view (works in today's viewer too);
  2. the Tauri app shell (file tree, graph, Code screen) through the API client module;
  3. the chat panel over ACP (login, streaming, approvals, diffs, sessions);
  4. the architecture canvas with live checks;
  5. `/ig:plan` from a design, through the chat;
  6. the screens canvas and image import;
  7. the bridge;
  8. as-built consolidation at finish.
