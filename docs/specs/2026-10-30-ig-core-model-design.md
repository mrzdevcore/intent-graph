# ig's core model: the intended architecture, as a spec

Status: design, not implemented. Date: 2026-10-30. Replaces
`2026-10-29-ig-design-mode-design.md` (its "change" becomes an edit to the file below) and the
plan `docs/plans/2026-10-30-chat-mentions.md` (mentions now go through the core).
Examples to read alongside: `docs/specs/system-text/` (`shop.kdl`, `queries.md`).

## Intent

ig's value is that it describes the **intended architecture** of a system, not the code itself. The
architecture is written down as a file, the way HTML is the text of a DOM: a spec people and agents
read, review and edit, that the code is checked against, and that an agent can build a whole system
from (`ig apply`).

Everything else in ig renders or uses that model: the map, Claude through MCP, the CLI, CI. None of
them works out facts on its own.

The user's rule: few features, built very well.

## Decided with the user

- **A DOM-like model in ig's core** is the source of truth. Renderers only render it.
- **Text form: KDL** (v2), node-based like the DOM. (The user asked for the comparison with HTML and
  YAML, `system-text/shop.*`; KDL is the working choice, read and parsed with a real parser.)
- **CSS-like selectors** query the model, for every user of it.
- **One API, two doors:** MCP tools for Claude and desktop-app commands for the UI call the same
  core functions.
- **Attributes are kept apart from structure.** Attributes (telemetry, owner…) are set by anyone in
  their own namespace; structure changes only through edits to the file.
- **Attribute values are written to files.**
- **The file holds the whole architecture**, not only what people add.
- **Architecture depth only:** system, apps and services, stores, queues, externals, people, and
  their parts (use cases, domain, ports, folders, UI components). Files, functions, classes,
  methods and component parts stay in the live model (selectors, map, Claude see them) and are never
  written to the file.
- **The file is the spec; it wins.** When code drifts from it, ig reports drift; the file changes
  only when someone accepts it.
- **You check what you write.** What the file describes must exist; code inside a node's `at` that
  the file does not describe is fine. The file's depth is the team's strictness.
- **Split into files:** one `ig/` folder, one file per top-level node.

## The model

- **Nodes in a tree.** Each node has a kind (like a tag name: `system`, `app`, `service`,
  `library`, `worker`, `store`, `queue`, `external`, `person`, `usecase`, `domain`, `port`,
  `folder`, `component`, and live-only `file`, `function`, `class`, `method`), a stable **id**, a
  readable **path** (`/shop/api/checkout`), attributes, and children.
- **Ids are stable across renames and moves** (the inode idea): a node keeps its id when its name or
  parent changes, so ig can tell "moved" from "deleted + created". Ids are written in the file.
- **Arrows are first-class links** between nodes: a kind (`calls`, `depends`, `data`, `network`,
  `event`, `implements`, `bundle`), an optional label, and, live, a count. ig writes an arrow of
  several kinds as its main kind; a hand-written `bundle` is satisfied by code of any kind.
- **Properties vs attributes.** A node's own properties (`title`, `tech`, `description`, `at`, and
  any the team writes, such as `owner.team`) are part of the spec in the file, changed through
  `ig_edit`. **Attributes** written by providers (`telemetry.p95-ms`, later `deploy.*`) live in
  `.ig/attrs/` and are set through `ig_set`; they never enter the committed spec. Both read the same
  way in selectors. (My call, from the review.)
- **Every node and arrow carries its origin:** in the file and found in the code; in the file only
  (to build); in the code only (drift: to remove or accept); or moved (same id, another place).
  Nothing in the file marks "planned" or "moved": the file only says how things should be, and ig
  works out the origin. To name a move, ig compares the node's `at` with its `at` in the last
  committed `ig.kdl` (git); code that left the old place and appears at the new one is "moved", not
  "removed + built". (My call.) Ids are unique across the system. Today's kinds map as: `api` →
  `service`, `group` → `folder`, `entry` stays, `parts` and Ansible `task` are live-only.

## The files

- An `ig/` folder at the repo root, committed (decided with the user): `ig/system.kdl` holds the
  `system` node and what sits directly in it without a file of its own (stores, externals, people);
  every other file holds exactly one top-level node, named like the file (`ig/api.kdl` →
  `service api { … }`). ig assembles the tree from the folder. A new top-level node gets a new file;
  a move between two top-level nodes edits both files. CODEOWNERS can own `ig/api.kdl`. `ig.yaml`
  keeps the analysis settings it has today. (Below, "the file" and `ig.kdl` mean this folder.)
- KDL v2. A node: kind, then id, then properties, then a block of children; arrows are written in
  their source node (`calls stripe label="refund"`). Example: `system-text/shop.kdl`.
- ig edits it in place, keeping comments and the user's order (kdl-rs is formatting-preserving), and
  formats what it adds canonically, so map edits, Claude's edits and hand edits never fight.
- Validated strictly; ig never writes a file it would reject.
- **Migration:** `.ig/map.json` (today's committed map) is converted once into `ig.kdl`, keeping its
  readings, pins and corrections; then it is no longer written.

## What a spike showed (2026-10-30, `kdl` crate)

- Parse then print gives back the identical text; a deleted node takes its own comment with it.
- Whitespace belongs to nodes: the line break after a `{` is part of the first child's `leading`,
  each node ends with its `terminator`, and nodes built in code have neither. Inserts, deletes and
  moves therefore need a small tidy step on the edited block (about 15 lines in the spike: first
  child starts with a line break, every node ends its line, comment lines take the block's
  indentation; a moved subtree is re-indented to its new depth).
- `-> stripe` and `calls -> stripe` are valid KDL v2 (a node named `->`), so arrows have room to
  grow if wanted; `calls stripe` stays the form.
- `kdl` 6.7.1 needs Rust 1.95; 6.5.0 works on 1.93. Pin the version the toolchain supports, or
  raise the toolchain. Its KQL query functions are commented out in 6.x: selectors are ig's own.
- Claude wrote valid KDL v2 in 8 of 8 tries; with an example in the prompt it followed ig's shape
  exactly, without one it invented its own (`path=` for `at=`, extra grouping nodes). ig gives
  Claude an example and validates every write.

## Truth and drift: the verbs

- **`ig init`** (or the first run in a repo without `ig.kdl`) writes the file from the code, so the
  spec matches reality on day one. The AI reading (titles, descriptions, kinds) fills it as today,
  shown and accepted like any edit.
- **After init, ig never changes the file on its own.** New code becomes drift; the AI reading of a
  new box arrives as a proposal (gold, like Claude's edits) to keep or drop. Who set what is in git
  and the journal, not stored per field in the file. (My call.)
- **`ig plan`**: the difference between the file and the code: to build, to remove, to move. Text in
  the terminal; on the map, the same as dashed (to build), struck through (to remove) and arrows
  (moved).
- **`ig apply`**: hands the plan to an agent (Claude) to build; on an empty repo, the whole
  system. Claude builds with its usual tools and permissions.
- **`ig check`**: never writes `ig/` (with none it exits 2: run `ig plan` or `ig view` first);
  exits non-zero while code and file differ; locally, in CI, and as "done" while
  Claude builds. **You check what you write** (decided with the user):
  - every node the file describes exists (code under its `at`), and every arrow the file lists
    exists: an arrow between two nodes is satisfied by any code edge from one's subtree into the
    other's (lifted, as arrows are today); an arrow touching a `person` is a declaration;
  - it fails on code under no node's `at`, and on arrows between described nodes that the file does
    not list;
  - code inside a node's `at` that the file does not describe is fine;
  - never checked: what ig cannot detect (people, labels, descriptions, `tech`); those are
    declarations.
- **`ig fmt`**: rewrites `ig/*.kdl` in canonical form (layout, indentation), keeping comments and
  the user's order; `ig fmt --check` fails in CI when a file is not formatted. The one command that
  rewrites spec files, and only their formatting, because the user asks (decided with the user).
- **`ig accept <selector>`**: takes drift into the file (the code wins, deliberately); Claude can
  propose the same edit. Either way it shows in review.
- A planned node says where its code will live (`at="src/billing/**"`); ig judges "built" with its
  own deterministic analysis, never an agent's word.
- **The spec drives the file system; ig never moves files itself** (decided with the user). When a
  node's `at` changes (it was moved or renamed in the spec), `ig plan` lists the concrete file
  operations: `move apps/api/src/refund/** → billing/src/refund/`, plus the folders to create for
  planned nodes. `ig apply` hands that list to the agent, which moves the files and fixes
  imports, configs and tests (a folder move alone breaks code, and fixing it is language work an
  agent already does). `ig check` then verifies the code sits where the spec says and has only the
  arrows the spec allows. ig decides what should be where and verifies it; it does not edit the
  repo's layout.

## Selectors

CSS selectors over the model, plus arrow operators:

| Selector | Picks |
|---|---|
| `service`, `#api`, `[tech=NestJS]` | kind, id or name, attribute |
| `service > usecase`, `service usecase` | child, descendant |
| `app -> store`, `store <- service`, `app -> * -> store` | an arrow out, in, two hops |
| `usecase:has(-> external)`, `:not(…)` | as in CSS |
| `:planned`, `:drift`, `:moved` | by origin |
| `service[telemetry.p95-ms > 200]`, `function[complexity > 15]` | numeric comparison |

Used by: Claude (`ig_query`), the map's search and layers, the CLI, and later rules and enrichment.
Exact grammar is the builder's, within this table.

## One API, two doors

- The operations live in Rust (`ig-app`), on the model: build it (file + code + attributes), query
  (selectors), get a node (its place, attributes, children, arrows in and out, origin), set
  attributes, edit structure (add, remove, move, rename, arrow), plan, check, accept.
- **MCP tools for Claude** (in ig's existing MCP server; they work in the desktop app and in a
  terminal alike): `ig_query`, `ig_get`, `ig_set` (attributes), `ig_edit` (structure: edits
  `ig.kdl`), `ig_plan`, `ig_check`.
- **Desktop-app commands** (Tauri) call the same functions for the UI. The viewer renders and sends
  references; it computes no system facts.
- The model announces what changed (like DOM mutation events), so an open map shows Claude's edits
  at once. In the desktop app the live model is held in the app; the CLI and a terminal Claude build
  it from the files.

## Editing, and Claude's edits

- The map edits the file through the API: add, remove, move, rename, draw an arrow, undo/redo,
  keyboard first.
- Every structural edit is journaled (who, what) in `.ig/journal.kdl` (not committed), giving
  undo/redo. Edits Claude made are written to the file at once (so `ig plan`, `ig check` and a
  terminal Claude see them) and shown gold until the user keeps them, one by one or all; dropping one
  reverts it through the journal. (My call.)
- Refactors are proposed by Claude: the user says what they want ("split api into orders and
  billing"); Claude proposes the edits; the user adjusts what is wrong and keeps it.

## @ mentions

- In the desktop chat, `@` completes nodes (selectors or names); "Add to chat" adds the selected
  box. The message carries references (path and id) only.
- Claude calls `ig_get` for a node's details, so a terminal session gets the same answer.
- Claude writes nodes as `` `@path` `` or `` `@Title` `` in inline code; the chat links unique ones:
  hover lights the box on the map, click flies there.

## Attributes and providers

- A provider is any program that writes `.ig/attrs/<provider>.kdl` (node id, values, time read; ids,
  not paths, so values and history survive renames and moves).
  ig loads those files as attributes. (My call: files make providers language-agnostic, like
  writing to a file system.)
- Each file holds the latest values; a history file keeps one snapshot per period (hourly by
  default, configurable).
- `.ig/attrs/` is not committed by default; a team commits a snapshot deliberately.
- Format example: `system-text/queries.md`, "Where attribute values live".

## Presentation is not the model

Like CSS, how the map looks is separate: themes (tokens), layout, hand moves, which layers show.
Hand moves become part of a committed `.ig/layout.kdl` keyed by node id (my call: today they are
kept per browser; a team should share them).

## Build order

Each step ships on its own and is excellent before the next.

1. The model and `ig.kdl`: build the model from code + file, `ig init` and the migration from
   `.ig/map.json`, the map rendering origins (to build, drift, moved), `ig plan` and `ig check`.
   `ig fmt` comes with step 4 (it shares the tidy step for edits).
2. Selectors and the read API: `ig_query`, `ig_get`, the CLI's `ig query`, the map's search on
   selectors.
3. @ mentions on `ig_get`.
4. Editing: `ig_edit`, the journal, the map's editing, Claude's gold edits, `ig accept`.
5. `ig apply`.
6. Attributes: `ig_set`, `.ig/attrs/` loading, layers from attributes.

## Later directions (not in this work; nothing above may block them)

- **Many repos as one system:** a `system/` repo whose `ig-system.kdl` mounts sibling repos into one
  tree (the `fstab` idea); arrows between repos from evidence (HTTP routes, `DATABASE_URL`,
  Compose/Kubernetes/Terraform files).
- **Rules (a linter for system design):** rules written with selectors (`app -> store` forbidden),
  set per team in config, run by `ig check`.
- **Providers:** telemetry (Prometheus and others), CODEOWNERS, incidents, cost.
- **Deployment:** infrastructure nodes and `deploy.*` attributes; ig reads existing infra files
  (Terraform, Kubernetes, `fly.toml`), checks architecture against infra, and later writes infra
  for the team's own tool to apply. ig never deploys by itself.
- A TypeScript API over the core for scripts and providers.

## Non-goals

- Inventing a language (KDL and CSS selectors are existing syntaxes).
- Deploying, or replacing Terraform, Pulumi or Kubernetes.
- Writing code-level nodes (files, functions) into `ig.kdl`.
- Editing code in the app; generating code without an agent.
- Multi-user real time, hosting.

## Anti-goals

- A file that silently follows the code (then it is a picture, not a spec).
- Two sources of truth: any part of ig computing system facts outside the core.
- Noisy drift: renames, moves and harmless code churn reported as architecture changes.
- Map edits that reformat or reorder the user's file.

## Left to the builder

- The exact KDL schema (names of properties, where ids go), the selector grammar within the table
  above, MCP tool schemas, CLI output.
- How ids are generated for new nodes, and how code found under an `at` is matched to a node.
- How the live model in the desktop app and MCP talk.
- Designs for every UI change on the canvas first (the project's design-before-code rule).
