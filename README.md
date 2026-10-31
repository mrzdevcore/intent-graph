<p align="center"><picture><source media="(prefers-color-scheme: dark)" srcset="assets/brand/lockup-dark.svg"><img alt="ig — intent-graph" src="assets/brand/lockup.svg" width="360"></picture></p>

# intent-graph (`ig`)

**ig** (say *"eye-gee"*, short for *intent graph*) — code graph and change checker for TypeScript/JavaScript, Python and Rust repositories, written in Rust.

`ig` parses the repo (oxc for TypeScript/JavaScript, tree-sitter for Python and Rust) into a graph of modules, classes, functions, methods, tests and your own
pattern nodes, and draws it as a **system map**: a canvas you walk, from the whole system down to the
functions, with each box named and described by an AI agent you choose to run (or by ig's own guess).
It is also an MCP server, so a coding agent can show its answers on the map.

Design: [docs/specs/design.md](docs/specs/design.md)

## Install

```sh
make install   # builds the viewer page (viewer/dist/index.html), then `cargo install --path crates/ig --locked`
```

`ig` needs `git` on `PATH` and nothing else. The AI reading needs one of the `claude`, `gemini` or `codex` CLIs, only when you ask for it.

## Build

`make build` (needs bun and cargo): the viewer first (`viewer/`, TypeScript, Svelte and Vite,
built into one offline HTML file), then `cargo build --release -p intent-graph`, which embeds it. `make test` builds the
viewer, runs its tests, then `cargo test --workspace`. Without bun, outside make: `cd viewer && npm install && npm run build`.
Without a built viewer, `cargo build` still works and `ig view` writes a placeholder page with the data as JSON.

Desktop app: see [Desktop app (local build)](#desktop-app-local-build). The workspace includes it, so
`cargo build` and `cargo test --workspace` compile Tauri too (on Linux that needs webkit2gtk).

## Desktop app (local build)

`crates/ig-desktop` is a Tauri 2 app: the map, editable (boxes, arrows, rename, move, delete, undo), beside a chat panel that runs Claude Code on your project. `ig .` (or `ig app [<dir>]`) opens it on a folder: it runs `$IG_APP` if set, else the app built next to this `ig` (`target/<profile>/ig-desktop`), else the installed `ig.app`.
It is built locally and not distributed (no signing, no auto-update), pending Anthropic's written confirmation.

```sh
make app       # target/release/bundle/macos/ig.app (unsigned)
make app-dev   # run it in development
```

The app ships its own `ig` (`Contents/MacOS/ig`; `target/debug/ig` in development) and puts it first on the agent's `PATH`, so the plugin's skills and `ig mcp` match the app even when it is opened from Finder.

- **Needs:** Rust, bun, node and npm (for the Claude adapter), and the Claude CLI logged in (run `claude`, then `/login`). The app never sees your credentials.
- **Adapter:** on the first chat, if it is missing, the app offers to install `@agentclientprotocol/claude-agent-acp@0.95.0` into its own data dir (never global).
- **Trust screen:** before any Claude session the app lists what will run: hooks from `.claude/settings*.json`, permission settings (including bypass modes), `AGENTS.md`/`CLAUDE.md`/`CLAUDE.local.md`, MCP servers from `.mcp.json`, the ig plugin, and the project's skills, commands and agents (with any tools they pre-approve or hooks they run). Every file under `.claude/` is hashed (files over 1 MB by size only). You decide, and confirm natively. It asks again when any of them changes.
- **Plugin:** the app always uses its own ig plugin (re-exported on each run into its data dir). Run `ig init` after upgrading ig to refresh a project's copy for terminal use.
- **Settings:** sessions load the project and local settings plus the ig plugin only. Your own skills and user settings do not load.
- **Chat:** streaming replies, tool cards, diffs, permission and question cards, mode and model selectors (switching to a bypass mode asks to confirm) and `/` commands.

## Quick start

```sh
ig init            # ig.yaml, ig/ (an empty system in an empty repo), the skills, an AGENTS.md snippet, .gitignore entries
ig .               # open the desktop app on this folder
ig view            # build the map, write .ig/view.html and open it
ig view --read     # the same, and let an AI agent read the architecture first
```

## `ig view`

```
ig view [--no-open] [--json] [--read | --no-read] [--agent claude|gemini|codex|ollama] [--root <dir>]
```

`ig view` builds the map in three stages and writes `.ig/view.html`, one offline page:

1. **Facts (always, no AI).** The graph gives packages, the folders inside them, imports, calls and
   file counts. Every box and arrow exists after this stage, laid out and drawn.
2. **Reading (optional, an AI agent).** The agent reads files with read-only tools and fills in each
   box's title, description, kind, tech, badges, logo and the pattern of its inside (`onion`,
   `hexagonal`, `layered`, or none). Every value in the map carries its source (`agent` or `ig`) and a confidence:
   `extracted` (read from a file), `inferred` (a judgment) or `ambiguous` (the agent was unsure).
3. **Layers (always).** Words drawn on boxes and arrows: `rules` (imports breaking a box's pattern,
   `ig.yaml` rules) and `risks` (ig's own code analysis), computed each run; and your own, from
   `.ig/layers/*.json` (below).

**The spec `ig/` is committed.** The first `ig view` writes the intended architecture as `ig/*.kdl`:
from `.ig/map.json` when there is one (its readings and corrections kept), else from the code and
the AI reading. After that `ig/` is yours: edit it by hand and commit it. ig changes it only
through edits you ask for (`ig accept`, `ig fmt`, `ig view --read`, Claude's `ig_edit`), each
in place: the nodes an edit touches change, every other line and comment keeps its text. Every run
compares it with the code: what both have is built, what only `ig/` says is still to build, what
only the code has is drift, and a box whose `at` changed since the last commit with the code still
at the old place is to move. `.ig/map.json` is no longer written; delete it once `ig/` is committed. A broken
`ig/` is left as is, with the file and line of each problem, and the map shows the code alone.

### Reading and consent

An agent only runs when you say so:

- `ig view --read`: it runs, without asking, on every box, described ones included.
- `ig view --no-read`: it never runs.
- once `ig/` exists, the reading reads the drift and the boxes with no description and proposes
  edits to `ig/` (adds and new titles, descriptions, tech): they are written, journaled as the
  reading's, and you review them with `git diff ig/`.
- plain `ig view` in a terminal: if boxes lack a reading, ig asks `Let an AI agent (claude) read the
  architecture? [Y/n]` and runs it on a yes. Without a terminal (a script, CI) it does not run.

`--agent` picks the CLI; otherwise `agent:` in `ig.yaml`, otherwise the first of `claude`, `gemini`,
`codex` on `PATH`. The agent gets read-only tools and skips the repo's own agent configuration where
its CLI allows: Claude runs with `--restricted --safe-mode --tools Read,Grep,Glob`; Codex with
`--sandbox read-only`; Gemini with default approvals and no extensions. The Gemini and Codex command
lines come from their documentation and have not been run against the real CLIs. A bad answer is
retried once; whatever is still not valid is dropped, with a notice. `--json` prints the map
and the notices instead of writing the page.

### Layers from files

Any tool can draw on the map by writing `.ig/layers/<name>.json`, at most 512 KB and 5,000 marks:

```json
{
  "layer": "health",
  "marks": [
    { "target": "api", "state": "warn", "badge": "p95 480 ms", "value": "480", "note": "slow since Tuesday" },
    { "target": "api->shared", "state": "down", "badge": "timeouts" }
  ]
}
```

`target` is a box id (`api`, `api/auth`) or an arrow id (`api->shared`). `state` is `ok`, `warn`, `bad`
or `down`; `badge`, `value` and `note` are optional text. Unknown fields, unknown targets and broken
files are dropped with a notice, never fatal. The layer appears in the **Layers** menu.

### The canvas

- **Wheel or trackpad pinch** zooms about the pointer; **drag** pans.
- **Click a box** to glide into it and open its details card (description, tech, kind, source and
  confidence of each value, marks). Zoomed in far enough, a box shows its inside: components, then
  functions.
- **Esc** closes the card and gives the focus back to the box.
- **Breadcrumb** (top left) jumps to any level above you.
- **Layers** (top right) turns each layer on and off; **-**, **Fit** and **+** zoom, and Fit returns to the whole system.
- Boxes are buttons: Tab moves between them, Enter opens one.

## Commands

| Command | Does |
|---|---|
| `ig view [--no-open] [--json] [--read \| --no-read] [--agent <name>]` | The system map (above). |
| `ig init [claude\|codex\|all]` | Scaffolding: `ig.yaml`, the explore and view skills for the chosen agents (default all), an `AGENTS.md` snippet, `.gitignore` entries. Never overwrites your edits. |
| `ig plan` | Writes `ig/` from the code the first time; then prints the plan: boxes and arrows still to build, boxes to move (their `at` changed since the last commit), drift (code in no box, arrows `ig/` does not have) and code ig does not read (outside every package). |
| `ig check` | The plan, read-only: exit 1 when anything is left (CI), 2 on a broken or missing `ig/`. |
| `ig fmt [--check]` | Re-indents `ig/*.kdl` (four spaces a level, at most one blank line between nodes); comments and order stay. `--check` writes nothing and exits 1 when a file would change. |
| `ig accept [--all \| <id>...]` | Takes drift into `ig/`: each box or arrow id from `ig plan` (quote arrow ids: `ig accept 'web->api'`), written one by one. |
| `ig mcp` | MCP server on stdio: `ig_state` and `ig_show` (a coding agent shows an answer, a path or subgraph with steps, in the viewer), `ig_plan` (the plan) and `ig_edit` (Claude edits `ig/`: add, remove, move, rename, set, arrow, unarrow; its edits are proposals you review with `git diff ig/`). |
| `ig plugin export <dir>` | Writes the current Claude plugin (skills, `.mcp.json`) into `<dir>`, overwriting. The desktop app uses this for its own copy. |
| `ig --version` | Prints the version. |

Every command accepts `--root <dir>`. Exit codes: 0 ok, 2 usage or error.

**Edits and the journal.** Every edit to `ig/` (`ig accept`, the reading's, Claude's) is recorded
in `.ig/journal.json`: who made it (person, claude or reading), the edit and the edit that undoes it,
whether you kept it and whether it is undone. It is local and not committed (`.ig/` is ignored);
losing it loses undo, never the spec.

## Config (`ig.yaml`)

```yaml
agent: claude                 # optional: the CLI for the AI reading (claude | gemini | codex)
layers:
  ui: ["@acme/ui"]            # a workspace package name, or a glob
  core: ["packages/core/**"]
rules:
  - forbid: "core -> ui"      # core must not import, call or render ui; shown on the map as a rules mark
embedded: [".vue"]            # single-file components
autoImports: [".nuxt/imports.d.ts", ".nuxt/components.d.ts"] # framework auto-imports (generated d.ts; run `nuxt prepare`)
ignore: ["**/generated/**"]
patterns:                     # ast-grep patterns that become graph nodes
  - kind: route
    match: "app.$METHOD($PATH, $$$)"
    where: { METHOD: [get, post] }
    id: "{METHOD} {PATH}"
```

`tests` and `testCalls` (what counts as a test file and test call) also exist. Keys from the removed
checker (`complexity`, `checkers`, `severity`, `blockOn`, `editor`, `hookMaxRounds`, `entryPoints`,
`review`) are ignored with a notice. All keys are described in the [design](docs/specs/design.md#igyaml).

## Integrations

**MCP:** `ig init claude` adds `.claude/skills/ig/.mcp.json`; without the plugin, `claude mcp add ig -- ig mcp`.

## Development

```sh
cargo test --workspace             # unit tests plus golden fixture tests
UPDATE_GOLDEN=1 cargo test         # rewrite crates/ig/tests/golden/ after an intended change
cd viewer && bun run test          # viewer model tests (Vitest); bun run typecheck for tsc
ig view --json > viewer/dev-data.json && cd viewer && bun run dev   # the viewer live in Vite
```

Layout: a Cargo workspace in `crates/`. `ig-core` is pure domain plus the ports; `ig-app` holds the use cases;
`ig-lang-ts`, `ig-lang-py`, `ig-lang-rs`, `ig-fs` and `ig-git` are adapters; `ig` is the binary that wires them.
The dependency rule (core and adapters depend only on `ig-core`) and a 600-line file limit are enforced by
`crates/ig/tests/architecture.rs`. A language is a `LanguageFrontend`: it says which files are its source, which packages the tree declares (npm/pnpm workspaces, `pyproject.toml`, Cargo workspaces) and which manifests fence a nested project off, then builds their nodes and edges; core and `ig-fs` know no language. A new one goes in `ig_wire::FRONTENDS`. Still TS-shaped: `prepare_checkout`'s `link_auto_imports` flag, and outside systems known by one list of npm, Python and Rust client library names.
See [Architecture](docs/specs/design.md#architecture).
