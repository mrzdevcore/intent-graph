# ig's building blocks: a small, general vocabulary, defined in files

Status: design, for review. Date: 2026-10-30. Builds on `2026-10-30-ig-core-model-design.md`
(the `ig/` folder, KDL, ids, origins, attributes and providers).

## Intent

ig's map should be a language for building systems: a short list of blocks (the nouns), arrows
(the verbs) and properties (the modifiers), general enough for any software system, small enough
to learn in one sitting, and close to clean architecture. Today the kinds are an enum in Rust
(`BoxKind`, `crates/ig-app/src/map/facts.rs`) and their rules (where a kind may sit, what it may
point at, its icon, the kinds the add form offers, the list in Claude's prompt) are scattered
across Rust, the MCP schema, the agent prompt and the viewer. Borrowing what n8n does well, each
block gets **one definition**, written in a file, that every part of ig reads: the reader and its
errors, `ig_edit`, Claude's prompt, the add-box form, the details card, the map's look.

Why now: the user is designing real systems with Claude on the map (automation, shop-b2b),
and the gaps show. There is no block for a scheduled job or an AI model; `app` means both a backend
and a desktop app; containers cannot be grouped (ig refused "Invoice collector" around four layers;
`micro` hit the 12-children cap and Claude put a new micro-front in the wrong place). The next
project, live status of a deployed system on the local machine (a separate design), needs blocks
that know whether they run and how to check them.

## Decided with the user

- **The vocabulary below**, in clean-architecture terms: blocks that run, blocks for design
  inside them, `group`, seven arrows.
- **`app` is what a person opens or runs** (web, mobile, desktop, CLI, terminal UI); **`service`
  runs on its own** and answers other programs. "UI" was rejected as sounding visual only.
- A CLI a person types is an `app`; a command the clock or CI starts is a `job`; each command is
  an `entry` (`kind="command"`) inside it.
- **AI is first-class:** `model` (a run block: an AI model the system calls, hosted or local) and
  `agent` (a design block: an LLM loop that chooses which tools to call). Vector stores, datasets,
  training and evaluation map onto `store` and `job`.
- **Extensible, with one rule:** a team may define its own kinds in its repo, and each one
  **extends a built-in kind** (`kind "iot-device" is="service"`). It inherits where it may sit,
  its arrows, its checks and its live status; it adds a name, an icon and properties.
- Deployment stays with the user's tools (Docker Compose, Ansible, Terraform, Kubernetes, a
  script): ig never deploys. The first live case runs entirely on the local machine.

## The vocabulary

**Blocks that run** (deployed; the live-status project checks them):

| Block | Is | Examples |
|---|---|---|
| `service` | a long-running program that answers other programs | API, backend, worker |
| `job` | runs, then exits | cron task, batch, migration, scheduled CLI |
| `app` | a program a person opens or runs | web app, mobile, desktop, CLI, TUI |
| `store` | data at rest | Postgres, SQLite, files, bucket, cache, vector DB, dataset |
| `queue` | messages in flight | queue, topic, stream, event bus |
| `model` | an AI model the system calls | Claude API, OpenAI, Ollama, llama.cpp |
| `external` | something used but not owned | Gmail, Stripe |
| `person` | someone who uses or runs the system | you, a customer, an admin |

**Blocks for design inside a running block:**

| Block | Is |
|---|---|
| `entry` | how work arrives; `kind` = `route`, `command`, `schedule` or `message` |
| `usecase` | one thing the system does |
| `domain` | the core data and rules |
| `port` | an interface the inside needs |
| `adapter` | implements a port with real tech |
| `agent` | an LLM loop: its model is a `calls` arrow to a `model`, its tools are `calls` arrows to use cases or ports; its prompt is a property |
| `module` | shared code that is none of the above |

**Grouping:** `system` (the root; one per map) and `group` (a folder at any depth, holding any
block allowed where the group sits).

**Arrows:** `calls` (asks and waits: HTTP, RPC, a function), `sends` (posts a message and moves
on), `reads` and `writes` (data, to or from a store), `uses` (depends on it: an import, a library,
a person using an app), `implements` (adapter → port), `triggers` (starts it: a schedule starts a
job, a message starts a handler). `bundle` stays as today: drawn when several kinds merge, never
written by hand.

**Properties every block may take:** `title` (required), `description`, `tech`, `at` (its code),
`owner`. `runs` is reserved for the live-status project and refused with a clear "not yet" until it
lands. Each built-in kind may add its own (for example `model` takes `provider`, `model`,
`hosted`; `entry` takes `kind`; `agent` takes `prompt`).

## Definitions, in files

A definition says, for one kind: its name and one-line meaning (shown in the add form and given to
Claude), its icon, where it may sit, what it may hold, which arrows may leave it and to which
kinds, its own properties (name, required, one-line meaning), and how ig checks it in code (today's
behaviour per kind: `at` globs, entries, ports and adapters). Sketch, for the builder to settle:

```kdl
kind "model" title="AI model" icon="sparkles" means="An AI model the system calls, hosted or local" {
    sits-in "system" "group"
    property "provider" means="Who serves it: anthropic, openai, ollama…"
    property "model" means="Its name, like claude-opus-5-5 or llama-3-8b"
    property "hosted" means="local or hosted" one-of="local" "hosted"
}

// ig/kinds/iot-device.kdl, a team's own kind
kind "iot-device" is="service" title="IoT device" icon="cpu" {
    property "firmware" required=#true
}
```

- **Built-in kinds ship inside ig** as files in this same format (embedded in the binary,
  `crates/ig/assets/kinds/`). Teams read them as examples; they cannot change them. (My call.)
- **A team's kinds live in `ig/kinds/*.kdl`**, committed with the map. A kind must name a built-in
  base with `is=`; it may add properties, a title, a meaning and an icon; it may not add arrow kinds
  or change where its base may sit. (My call, for the first version: new arrow kinds are where
  vocabularies fall apart.) Its name may not shadow a built-in one.
- One loader turns definitions into a table that the reader, `ig_edit`, the MCP schema, the agent
  prompt (`crates/ig/assets/prompts/read.md`), the viewer (add form, details card, icons) and the
  layout use. No other place may list kinds. Colours stay with the theme (by role: run block,
  design block, group), not in definitions, so themes keep working.
- The viewer receives the table with the view, so a team's kinds show and can be added on the map.

## Where a block may sit (replaces the three fixed levels)

Today a container must sit in the system and a component in a container (`file.rs`), so nothing
can group containers. With definitions:

- Run blocks sit in the `system` or in a `group` that sits where they may.
- Design blocks sit inside a run block (or a `group` inside one), and `usecase`/`port`/`domain`/
  `adapter`/`agent`/`module` may hold design blocks too (a domain holding its entities).
- `group` sits anywhere and holds what may sit where the group sits. Groups are for reading: they
  do not change which code a block owns and add no drift of their own.
- The **12-children limit becomes a warning** in `ig check` and the map, not an error: it keeps
  maps readable without forcing a wrong placement. (My call; the cap is about layout.)
- The layout still needs a level per box (outer, inner, code). It derives it from the definition
  (run block or design block) and the depth, not from a fixed level in the file.

### The placement table (my call)

| Block | May sit in |
|---|---|
| `service`, `job`, `app`, `store`, `queue`, `model`, `external`, `person` | `system`, an outer `group` |
| `entry` | `service`, `job`, `app`, an inner `group` |
| `usecase`, `domain`, `port`, `adapter`, `agent`, `module` | `service`, `job`, `app`, an inner `group`, and each other (a `domain` holding entities, a `module` holding a `port`) |
| `group` | anywhere; it is **outer** when no run block is above it, **inner** when one is |

So a run block never sits inside another run block (a store "inside" a service is drawn as an
arrow), `person` and `external` may be grouped, `module` never sits at the top level, and a group
never mixes levels: an outer group holds only run blocks and outer groups, an inner group only
design blocks and inner groups. Every map written today fits this table (today's containers are
run blocks in the system; today's components are design blocks in a container).

### Levels for the layout (my call)

A box's level is set by its nearest run-block ancestor: no run block above it (the box itself
being a run block, or an outer group) → the outer level; a run block above it → the inner level;
live-only kinds → the code level. Arrows are lifted to the boxes of their level as today
(`arrows.rs`), with groups taking part like any box. Drift ownership is unchanged: a group owns no
code, so code under its children belongs to them.

## How it lives in the code (my call)

- **The built-ins stay a Rust enum** (`BoxKind`, renamed variants per the table below). Each box
  gains `custom: Option<String>`: a team kind is stored on the box as its base kind plus its custom
  name. Layout, colours, checks and arrows read the base; titles, icons, properties and the add form
  read the definition. This keeps the change to the ~185 uses of `BoxKind` mechanical.
- "No other place lists kinds" is enforced by a test: every enum variant has exactly one built-in
  definition file and every built-in file names a variant. The MCP schema, the agent prompt's kind
  list and the viewer's kind list are generated from the table, never written by hand.
- **In `ig/`** a team kind is written by its own name (`iot-device "sensor-1" …`). A name that is
  neither built in nor defined in `ig/kinds/` is a read error naming the missing file, like any
  other bad `ig/` (deleting a kind file while boxes use it is caught by `ig check` and the map's
  error banner, never by a silent fallback).
- **`.ig/map.json`** (the code-derived cache, still written for `ig view` without `ig/`) takes the
  new names on its next write and reads the old ones as aliases; it is a cache, so nothing migrates
  it by hand.
- **Properties stay strict**, as today (`unknown property` is already an error): a property is
  allowed when the kind's definition, its base's, or the common list names it. `runs` gets its own
  message: "`runs` comes with live status; not yet".

## Arrows: which are checked (my call)

The definitions say which arrows may leave a kind and to which kinds (for example `reads`/`writes`
to `store`, `implements` from `adapter` to `port`, `sends` to `queue`, `triggers` to `job` or
`entry`). In this first version a mismatch is a **warning** in `ig check` and on the map, never a
read error, so no map written today stops reading after the arrow renames below.

## Code checks per kind (my call)

Unchanged: every kind with `at` is checked the way boxes are today (its globs own code; an `at`
with no code is "to build"; code in no box is drift). `job`, `model` and `agent` follow the same
rule; a hosted `model` usually has no `at` and is never "to build". `implements` arrows are checked
against the code's implements edges, as today. Code-derived maps never produce `job`, `model` or
`agent` in this version; they produce `service` where today's reader says `api` or `worker`, and
`app` where it says `app`.

## Changing a kind

`ig_edit`'s `set` takes `key: "kind"` (it rewrites the node's name in place, keeping its id,
children, comments and order) and is refused when the new kind may not sit where the box is. The
app → service proposal below is such a `set`, shown in the proposals list like any other edit.

## Old maps keep working

Every `ig/` written today must read without edits. Old kind names are read as the new ones, and
`ig fmt` rewrites them once to the new names (the file stays the user's: comments and order kept):

| Old | New | Note |
|---|---|---|
| `api`, `worker` | `service` | |
| `library` | `module` | |
| `folder` | `group` | |
| `component` (UI component in a container) | `module` | |
| `app` | `app` | name kept, meaning narrows to "a person runs it": see below |
| arrow `depends` | `uses` | |
| arrow `network` | `calls` | the protocol stays in the label |
| arrow `event` | `sends` | |
| arrow `data` | `reads` | the code rarely tells reads from writes; the user or Claude changes it to `writes` where it is (my call) |

An old `app` that no `person` uses and that has route entries is most likely a backend: ig does
not rename it silently. The AI reading, or Claude through `ig_edit`, proposes `service`, which the
user keeps or drops like any other proposal (the 4b review flow). Live-only kinds (`file`,
`function`, `class`, `method`, `parts`, Ansible `task`) are unchanged and still never written to
`ig/`.

Code-derived maps (`ig view` without an `ig/`) produce the new kinds directly (packages that serve
routes → `service`, packages with a UI → `app`, and so on), so a first `ig init` writes the new
vocabulary.

## Non-goals

- Live status, `runs`, readers for Compose/Terraform/Kubernetes: the next design.
- Deploying anything, generating deploy files.
- New arrow kinds from teams; kinds that do not extend a built-in one.
- Exporting the vocabulary to LikeC4 or Structurizr (a later direction the report recommends;
  nothing here may block it).

## Anti-goals

- A vocabulary that grows with every request: a new built-in kind needs a case none of the
  existing blocks can express without losing meaning.
- Two lists of kinds anywhere in the code after this work.
- An old map that stops reading, or a rename ig does without the user seeing it.
- Kinds whose meaning lives in their name only: every kind has a one-line meaning Claude reads.

## Each kind looks like what it is (added with the user)

Run blocks get a shape of their own, so a map reads at a glance: `store` a cylinder, `queue` a
pipe, `service` a plain rounded box, `app` a window frame (a terminal frame for a CLI), `job` a box
marked as running then exiting, `model` and `agent` shapes that set AI apart, `external` a dashed
outline, `person` a figure (as today), `group` a thin dotted frame with its name on the edge.
Design blocks stay cards with their icons. Shape says the kind; colour keeps saying the status
(built, to build, drift). Every shape keeps the same footprint, so the layout is unchanged. A
team's kind takes its base's shape. The shapes are sketched and approved on the canvas first.

## Design before code

The add-box form (kinds grouped as "runs" / "inside", each with its one-line meaning and icon), the
icons of the new kinds, the look of `group` and the 12-children warning on the map are sketched on
the Claude Design canvas ("ig map: details panel", next to the 4b rows) and approved before any
viewer change.

## Left to the builder

- The exact definition grammar (node and property names), within the shape above.
- Icon names and drawings for `job`, `model`, `agent`, `adapter`, `group`, from the viewer's
  existing icon set.
- How the definition table travels to the viewer (in the view JSON, or once per session).
- The order of the work, provided each step leaves `cargo test --workspace` and the viewer tests
  green and every existing `ig/` readable.
