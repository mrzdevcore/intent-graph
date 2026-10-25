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
- **Single user and local for the MVP,** on `ig serve`. A shared, multi-user, hosted version is planned for later and out of scope here. Nothing in this design may block it: state lives in files, and the server is the only writer.
- **Claude only** (Codex is parked).
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

## Anti-goals

- An editor that feels like a form: dialogs for every box, no keyboard shortcuts, no undo.
- A design that silently drifts from the plan or from the code.
- An agent that changes the drawing while the user designs. Changes the agent wants during design are proposed as notes. The as-built consolidation at `ig apply finish` is the only automatic rewrite, and the user accepts it.

## Ownership: design vs plan

- **The design** is user-owned, in `plans/<slug>/design.json`, `plans/<slug>/screens/*.json` and `plans/<slug>/assets/*`. Only the app writes it, on the user's actions.
- **The plan** is agent-owned, in `plans/<slug>/plan.md` (today's plan format with its `ig-plan` block). Only the agent writes it, from the design, the notes and the chat.
- The existing notes file moves to `plans/<slug>/notes.md` (today's format). The agent ticks and answers notes as today.
- Plans in today's flat layout (`plans/<slug>.md`) keep working. The folder layout applies to plans created from the Design workspace.
- **During design and apply, the agent never edits the design.** It reports gaps ("your design has an arrow to `Repository` but nothing creates it") and proposes design changes as notes to the user.
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

`ig serve` already serves the viewer and an API. In this project the viewer becomes an app:

- data loads from the API instead of being embedded in the page;
- it has screens with real URLs: Graph (today's modes), Design (per feature), Code (read-only CodeMirror 6 with before/after diff), Call chain.

The single-file offline `ig view` keeps working as a read-only export. The Design workspace needs `ig serve`.

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

- **Rendering** uses `rough.js` for the sketchy style and `perfect-freehand` for pen strokes, both MIT. The canvas itself is ig's own: SVG or canvas is the builder's choice.
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
- While the user keeps drawing, design changes reach the agent the same way notes do today: an `ig serve` event, then the channel or `ig_wait_notes`. The agent updates the plan.
- The plan view shows plan-versus-design differences: what the agent added and what it didn't take from the design.
- `/ig:apply` is unchanged, except that the reviewer also receives the screens and links as context.
- **UI code follows the design system.** The apply skill tells the agent to build screens only from the project's existing UI components and tokens, and to treat a mockup element with no matching component as a question for the user, never a reason to invent one. If the project has a linter or checker for this (an eslint rule, a stylelint config), it is enforced as a checker.

## Author's calls (from the builder check, not discussed with the user)

- **Reaching the agent:** the app assumes a Claude session the user started, attached through `ig mcp` (channel, or `ig_wait_notes`). The app shows whether an agent is connected (`ig mcp` registers with serve while it runs). A **"Plan this"** action sends a request event the same way notes do. If no agent is connected, the app shows the exact command to run in a terminal (`claude` with the ig plugin, or `ig serve claude`). The app never starts or manages a Claude process. Because the channel is unverified, `ig_wait_notes` remains the guaranteed path, and the plan skill always uses it when no channel event has arrived.
- **Several features per `ig serve`.** The app lists the features (the folders under `plans/`, plus flat plans), creates a new one (a folder with an empty `design.json`), and switches between them. The API and events are per feature, keyed by slug. A feature can exist with only a design: no `plan.md` is needed to draw or to add notes. The agent's wait and notes tools take a slug, defaulting to the current plan.
- **Live checks are exact and computed in Rust:** the design is sent to serve, which merges it like `plan::merge` and runs the same checks as `ig plan check`. There is never a TypeScript port of the rules. Edits are debounced (target under 150 ms from the edit to the shown result) and measured on a repository of acme's size. An incremental merge is added only if the measurement says so.

## Usability bar (acceptance)

The user should be able to sketch the intake patient-export feature end to end without reading docs:

- **Direct manipulation:** drag to place, drag to link, double-click to rename, Delete to remove; no modal dialogs for common actions.
- **Undo and redo** for every action, with Cmd/Ctrl+Z and Shift+Z.
- **Keyboard shortcuts** for every palette tool, shown in tooltips.
- **Navigation:** zoom, pan and fit; snapping to rows and a grid; multi-select; copy and paste; duplicate.
- **Saving:** autosave to files, never a save button, never losing work. Files are written atomically.
- **Feedback:** rule problems are visible immediately on the element, with the reason on hover.
- **Speed:** smooth on a design with 200 elements over the context of a repository the size of acme (about 1,000 modules).
- **Themes:** light and dark.
- Before the MVP is called done, it is tried on intake: sketch patient export, get a plan, and apply it.

## Constraints

- Rust: no new crates unless clearly justified. The viewer app: TypeScript and Lit (as today); npm packages are fine, MIT or similar license, pinned.
- `ig serve` security stays as built: loopback only, a token on every write, Origin and Host checks. Design writes go through the same token-gated API.
- Design files are user content: the agent treats their text as data, never as instructions (same rule as notes).
- Existing commands and the existing single-plan-file flow keep working.

## Left to the builder

- SVG or canvas for the screens canvas, the exact JSON schemas of `design.json` and the screens files (versioned with a `version` field), and stable element-id rules.
- How the app routes and loads data from the API.
- The exact palette icons and look.
- How the call chain excludes test stubs and chooses its depth.
- The order of delivery. Suggested, each step useful on its own:
  1. the call-chain view;
  2. the app shell with API-loaded data and the Code screen;
  3. the architecture canvas with live checks;
  4. `/ig:plan` from a design;
  5. the screens canvas and image import;
  6. the bridge.
