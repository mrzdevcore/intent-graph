# ig: the system map (minimal restart)

Design mockups: https://claude.ai/artifact/4w5FRpxbX4otp7G7xaADg6. Screen 4 (onion, press Play and
click Checkout) and the "The box" sheet are the target look. Screens 1–3 show the flows in the older
plain style.

## Intent

ig is a **system design tool for a team**, not an editor. People open their real editor (Zed, VS
Code) to write code. They open ig to **understand how the system works**, discuss it, and later
sketch changes that Claude implements and review merge requests as pictures.

The previous iteration grew too many parts: graph modes, a file tree, design boards, screens, the
bridge and a plan view. Its sigma.js graph read worse than Cytoscape. The user's verdict was
"becoming crappy". This restart goes back to **one canvas and one chat**, and adds features one at a
time, each only after the previous one proves useful in real use.

## The one rule of the map

**The canvas never shows thousands of nodes.** There is always a bigger picture first: a handful of
boxes a person can read. You zoom into a box only when you need to understand that part. Claude's
answers follow the same rule: "how does X work?" shows the few boxes and arrows that matter.

## Goals (this restart, in order; each ships and gets used before the next starts)

1. **The map.** Open a repo, see the whole system as a few boxes arranged in the shape of its
   architecture, and zoom smoothly into any box to see what is inside.
2. **Ask, see the answer drawn.** Ask Claude in the chat "how is X related to Y?" or "how does O
   work?". The relevant boxes light up on the map, opened just deep enough, with numbered steps. A
   Mermaid sequence diagram appears in the chat when the question is about a flow over time.

Later, not in this restart's plan, and each needing its own go-ahead:

3. **Changes on the map:** a branch or merge request drawn as new, changed and removed boxes, plus
   broken rules.
4. **Sketch then implement:** draw a new box or arrow on the map; Claude implements it and opens the
   merge request.
5. **Team sharing.**
6. **Links on boxes** (docs, dashboards).
7. **Live map:** probes in production report onto the map. A box turns red or gets a red badge when
   errors climb, an arrow turns amber when it gets slow, and a box that stops responding shows
   "down". Real latency comes from traces (OpenTelemetry, Datadog, Sentry). This restart only has to
   keep it possible: see "Ids production data can match" below.

## Purpose behind every choice

ig exists to help teams build **robust, scalable and reliable** software when AI writes much of
the code. Claude works at the code level, fast. The human steers at the container and component
levels. The danger is quiet erosion of boundaries: domain code importing the database, two
components growing the same job, the web app calling Postgres directly. ig keeps those levels
visible and checked, so erosion shows up as a red arrow on the map, not as an incident.

## What a box is: four levels (C4)

A box is anything with a boundary someone should protect. Which boxes you see depends on how far
you have zoomed in.

1. **System:** your system, the people who use it, and outside systems (Stripe, an identity
   provider).
2. **Containers:** things that **run or store**: a web app, an API, a worker, Postgres, Redis, a
   queue. Real logos live here. Reliability badges live here: sync or async, stateful, single point
   of failure, retries.
3. **Components:** blocks of responsibility inside one container (Checkout, Order, PayGateway,
   billing). **The architecture pattern (onion, layered, hexagonal) applies here,** per container.
   A monorepo usually has several containers, each with its own pattern.
4. **Code:** classes and functions, from ig alone.

Files and folders are never boxes; they are a box's "made of" detail. Single imports are never
drawn alone; they only appear bundled into arrows.

The top of the map is level 1–2: containers and outside systems, never rings. Zooming into a
container shows its components in the shape of its pattern. *(This corrects mockup screen 4, which
put the rings at the top.)*

## Arrows

One look per kind of connection (see the "Building blocks" sheet in the mockups). Each arrow
carries at most one label, sitting on the line in a pill of the arrow's color:

| Kind | Look | Label |
|---|---|---|
| Depends on (import) | grey, thin | none |
| Calls | ink, solid | the function, in code type |
| Network request | blue, dashed | method and route, or the procedure |
| Event | purple, dotted, open head | the event's name, with a bolt |
| Implements | green, dashed, hollow head | "implements", pointing at the port |
| Reads and writes | teal, one head per direction | the table or collection |
| Bundle | width grows with the count | "14 calls · 3 requests"; splits when zoomed in |
| Breaks a rule | red, heavier | the rule |
| Performance risk | amber | the risk ("query in a loop") |
| Claude's answer | gold, heavier | the step number |

Far away, arrows are bare lines (bundles show only their width); labels appear from mid zoom; hover
shows the rest.

Containers on the canvas are **rings** (onion), **bands** (layered) and **groups** (a feature or
bounded context, its name in a tab on the border). **Notes** are team comments pinned to a box or an
arrow; they come with goal 5.

## Performance

Performance lives mostly on arrows, at every level:

- **Containers:** the hops on a request path, and sync chains (a request waiting on 3 services is as
  slow as all 3 together).
- **Components:** fan-out, and chatty boundaries (a bundle of 40 calls where one would do).
- **Code:** a database or HTTP call inside a loop (N+1), queries with no limit, and work repeated
  per item.

ig finds these from code alone and shows them as amber badges and arrows, the same way broken rules
show red. In this restart: **calls inside loops** and **sync hop count on a request path** only
*(my call: the two with the clearest signal)*. Runtime numbers wait for goal 7.

## Ids production data can match

Every box id is stable and matchable to production data:
- a container's id equals its service name, as it would appear in traces (`service.name`), and is
  read from the package name or deploy config when present;
- a component's id is a slug, plus the routes or span names it owns, listed in `map.json`.

This costs nothing now, and it is what makes goal 7 a data join rather than a redesign.

## Non-goals

- Code editing, autocomplete, a file tree, or anything an IDE already does well.
- A view of every file or function at once.
- The previous design boards, screens canvas, bridge and plan view. They leave the interface; their
  code stays in git history.

## Anti-goals (a failure even if it works)

- A screen that looks like a hairball. If the top level needs more than about 12 boxes, the grouping
  is wrong; it is not a reason to draw more.
- A map that jumps or reflows when you zoom. Zoom is a camera move only.
- An analysis the team can't correct. If Claude reads the repo wrong, a sentence in the chat fixes
  it, and the fix sticks.

## How the map is made: ig + Claude

**ig counts. Claude reads.**

- **ig (Rust, deterministic, fast, runs on every save):** packages, folders, files, functions,
  imports and calls, the direction of every dependency, and the outside services used (from
  packages, env vars and URLs). Most of this already exists in ig-core, ig-lang-ts and ig-app.
- **Claude (via Claude Code in the app, on demand):** reads ig's facts plus folder names and a few
  files, then:
  - names the architecture pattern (onion/clean, layered, hexagonal, feature-sliced, or "none clear");
  - places each part in its ring, layer or slot;
  - writes each box's title, kind, tech line and one-line description;
  - picks the icon (a real logo when a product is behind it).
  It explains its reading in the chat ("onion, because `src/domain` imports nothing…").
- **ig checks Claude's reading** where it can be checked. For example, in an onion reading no
  dependency may point outward. Violations show as "rule broken" badges, not as silent errors.
- **The result is a file in the repo:** `.ig/map.json`, committed, so the whole team sees the same
  map and corrections persist. Claude reruns only when the structure changes: new or removed
  packages or top-level folders, or a new outside service. ig detects that and offers "Refresh the
  map". *(The file name and format are my call; the "committed file" decision follows from "as a
  team" and "the fix sticks".)*
- **Without Claude,** ig still draws a plain map from its own facts (packages → folders, grouped by
  layer config if any). It is never blank. *(my call)*

### Who owns what *(my calls, settled after the builder check)*

- **`.ig/map.json` holds Claude's reading only:**
  - the pattern;
  - the top-level parts and which ring, layer or slot each belongs to;
  - each box's id, title, kind, tech line, description and icon;
  - which code paths (globs) make up each box.
  It holds **no coordinates**. ig computes positions deterministically from the reading and the
  pattern on every run, so the committed file only changes when the reading changes (no diff noise)
  and `ig view` draws the same map as the app.
- **Depth:** Claude's reading covers levels 1–3 (system → containers → components). Level 4 (code inside a component,
  inside a step) come from ig's own facts: members and call order from the call graph, grouped by
  folder, then by name prefix, to stay at ≤12. Their descriptions are the code's own names and doc
  comments. Claude adds a sentence only when asked in the chat. "Every box holds its inside" is
  ig's job, so it is always current.
- **Stable ids and corrections:** a box id is a short slug Claude picks once (`checkout`,
  `payments`). Its membership is the list of globs. A correction in the chat ("payments belongs in
  domain") is written by Claude into `map.json` and marked `"pinned": true` on that box. A later
  refresh keeps pinned boxes as they are and only re-reads the rest. A rename of a folder breaks a
  glob, ig flags the box as "needs refresh", and Claude repairs the glob on the next refresh.
- **Who decides grouping:** Claude does. ig's fallback (no `map.json` yet, or Claude unavailable)
  is the same output as the "none clear" pattern: layered bands by package type, then folders.
- **First open:** ig shows its fallback map instantly. A "Read the architecture" button runs
  Claude's reading; it is a button, not automatic, because it spends the user's Claude usage. The
  `ig` CLI can also write `map.json` headlessly via the MCP write tool, so goal 1 is testable
  without the app.
- **`ig_show`:** it accepts box ids **and** the existing graph node ids. A node id lights up its
  enclosing box, zoomed to the depth where that node is visible. Existing callers keep working.
- **Rules:** this restart checks only "dependencies point inward" for onion and "dependencies point
  down" for layered. Each is aggregated to box level as a "rule broken" badge. Clicking the badge
  zooms in to the offending import edges. Existing `ig.yaml` rules keep running through
  ig-checkers as before.
- **The new map is a fresh component.** The Cytoscape Graph, the boards and the rest are deleted in
  the restart's first task, not adapted.

## The map's shape follows the pattern

- **Onion / clean:** rounded rings around each other. Domain in the middle (gold), application
  around it (green), infrastructure and UI outside (blue-grey). Arrows point inward. A dashed arrow
  means "implements a port".
- **Layered:** horizontal bands top to bottom (apps → packages → services), like mockup screen 1.
- **Hexagonal:** a core with ports on its edge and adapters around it.
- **None clear:** layered bands by package type.

Each shape is a layout function: (map, pattern) → positions. New patterns are added one at a time,
as real repos need them. **Onion and layered come first** *(my call: onion is the user's example;
layered is the fallback every repo can use)*.

## The box: one component for every element

See the "The box" sheet in the mockups.

- **Anatomy:**
  - icon (a real logo via simple-icons, already a viewer dependency, else the kind's icon on a tile
    tinted with the kind's color);
  - title (Kalam, hand-drawn feel);
  - kind and tech line;
  - one- to two-line description;
  - badges: status on this branch, rules broken, and "N inside ›".
- **Kinds and colors:**
  - container kinds (web app, API, worker, store, queue, outside system): mostly identified by
    their logo;
  - component kinds: entry point blue `#3b6fd8`; use case green `#2f8f5b`; port green dashed;
    domain gold `#b8860b`;
  - outside systems: grey dashed pill. The full token set is in the mockup's
  `Box.dc.html`.
- **States:** as it is; part of Claude's answer (gold glow); new, changed or removed on this branch;
  sketched, not built (dashed blue); breaks a rule (red badge). Only "as it is", "Claude's answer"
  and "breaks a rule" are needed in this restart. The others come with goals 3 and 4.
- **Levels of detail by zoom:** far shows the icon and title; mid adds kind and description; near
  adds badges; closest shows the inside.

## Zoom

- Clicking a box glides the camera into it in about 750 ms with an ease-out curve. Its label fades
  as its inside fades in. The breadcrumb ("billing-service › application › Checkout") glides back
  out. Scrolling and pinching zoom freely; levels of detail switch on the on-screen size of each box.
- **Every box already holds its inside**, so zooming never reflows the map.
- **Depth** follows the four levels: system → containers → components (in the container's pattern
  shape) → code. The deepest view of a function shows its calls and the data it touches, with
  **"Open in editor"** one click away. ig never shows source code itself. *(my call, from "not an
  IDE")*
- **The inside of a box is never more than about 12 children either.** If there are more, ig groups
  them (by folder, then by name prefix) into sub-boxes.

## Ask (goal 2)

- The chat is the existing Claude Code chat in the app, docked on the right.
- Claude answers in text and calls the existing MCP tool `ig_show` with box ids from `.ig/map.json`
  (extended from graph node ids). The map highlights those boxes in gold, dims the rest, numbers the
  steps, and zooms so all highlighted boxes fit. A "Showing: <question> · Clear" chip sits at the
  top.
- For flow questions Claude also writes a Mermaid sequence diagram in its answer. The chat renders
  Mermaid (it already depends on `mermaid`).

## What stays, what goes

- **Stays:**
  - the Rust analysis crates (ig-core, ig-lang-ts, ig-app, ig-checkers, ig-git, ig-fs, ig-wire);
  - the `ig` CLI and its MCP server (`ig_show` and the rest);
  - the Tauri desktop app shell, project open/recent, and the docked Claude Code chat session;
  - the `ig view` single-file build, which shows the same map without chat.
- **Leaves the interface:**
  - graph modes (Architecture/Blueprint/Changed/Focus);
  - the file tree;
  - design boards and screens, the bridge, the plan view and the Inspector;
  - the Cytoscape graph.
  Their code is removed from the build. History keeps it. Crates used only by them (`ig-board`,
  `ig-design`) are dropped from the workspace. *(my call; say if you want them kept compiling)*
- `feat/design-boards` is not merged as is. The restart branches from it, so the analysis
  improvements and the Cytoscape revert carry over, and the removal happens in the restart's first
  task. *(my call)*

## Rendering *(my call)*

The map shows a few dozen boxes at most, so it is **plain DOM/SVG inside one transformed "world"
element**, exactly like the prototype: boxes are Svelte components, arrows are SVG paths, and zoom
is a CSS transform with a transition. There is no graph library, no WebGL and no wasm. Layout is
computed by ig-app (Rust) per pattern and shipped in the view data. That way `ig view` and the app
draw identically, and it is testable without a browser.

## Constraints

- TS/JS monorepos (what ig-lang-ts reads today).
- No new runtime dependencies are needed (Svelte, simple-icons and mermaid are already in). Any new
  one is pinned to an exact version.
- Never push without the user's explicit consent. No Claude attribution in commits.
- Each goal ends with the user using it on a real repo (afwa and front-vision-board scratch copies)
  before the next starts.

## Left to the builder

- The exact `.ig/map.json` schema, and how ig detects "structure changed".
- The layout math for each pattern (ring sizing, slot packing), within the ≤12-per-level rule.
- How Claude is prompted for the reading (a skill plus an MCP tool to write the map is the expected
  shape).
- The levels-of-detail thresholds.
- Arrow routing, as long as arrows never cross a box.
