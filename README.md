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

## A folder of projects

`ig view` on a folder that holds several projects, and is not a project itself, draws them on one map: a box per project with its own map inside, what runs the others on a band below, and dashed links where a project's files name another. A project need not be a git repo: a manifest or Terraform files are enough. Worktrees, archives, downloaded releases and folders without source are left out; the Folders panel says why. Secrets are never opened (`.env*`, `*.tfstate*`, `*.tfvars`, keys). `ig view` writes no `ig/`; `ig analyse` on the folder reads each project that has none and writes its `ig/`.

## Build

`make build` (needs pnpm, node and cargo): the viewer first (`viewer/`, TypeScript, Svelte and Vite,
built into one offline HTML file), then `cargo build --release -p intent-graph`, which embeds it. `make test` builds the
viewer, runs its tests, then `cargo test --workspace`. Dependency build scripts stay off; the supply-chain settings are in `viewer/pnpm-workspace.yaml`.
Without a built viewer, `cargo build` still works and `ig view` writes a placeholder page with the data as JSON.

Desktop app: see [Desktop app (local build)](#desktop-app-local-build). The workspace includes it, so
`cargo build` and `cargo test --workspace` compile Tauri too (on Linux that needs webkit2gtk).

## Desktop app (local build)

`crates/ig-desktop` is a Tauri 2 app: the map, editable (boxes, arrows, rename, move, delete, undo), beside a chat panel that runs Claude Code on your project. `ig app [<dir>]` opens it on a folder: it runs `$IG_APP` if set, else the app built next to this `ig` (`target/<profile>/ig-desktop`), else the installed `ig.app`.
It is built locally and not distributed (no signing, no auto-update), pending Anthropic's written confirmation.

```sh
make app       # target/release/bundle/macos/ig.app (unsigned)
make app-dev   # run it in development
```

The app ships its own `ig` (`Contents/MacOS/ig`; `target/debug/ig` in development) and puts it first on the agent's `PATH`, so the plugin's skills and `ig mcp` match the app even when it is opened from Finder.

- **Needs:** Rust, pnpm, node and npm (for the Claude adapter), and the Claude CLI logged in (run `claude`, then `/login`). The app never sees your credentials.
- **Adapter:** on the first chat, if it is missing, the app offers to install `@agentclientprotocol/claude-agent-acp@0.95.0` into its own data dir (never global).
- **Trust screen:** before any Claude session the app lists what will run: hooks from `.claude/settings*.json`, permission settings (including bypass modes), `AGENTS.md`/`CLAUDE.md`/`CLAUDE.local.md`, MCP servers from `.mcp.json`, the ig plugin, and the project's skills, commands and agents (with any tools they pre-approve or hooks they run). Every file under `.claude/` is hashed (files over 1 MB by size only). You decide, and confirm natively. It asks again when any of them changes.
- **Answers on the map:** when the agent calls `ig_show`, the map flies to the boxes and arrows in its answer, lights them with step numbers and fades the rest; a card lists the steps (←, → to walk, Esc to clear). If you are editing, the answer waits for you. `ig view` opens on an answer less than an hour old. The app writes no `ig/` when it opens a project: run `ig analyse` first.
- **Plugin:** the app always uses its own ig plugin (re-exported on each run into its data dir). Run `ig init` after upgrading ig to refresh a project's copy for terminal use.
- **Settings:** sessions load the project and local settings plus the ig plugin only. Your own skills and user settings do not load.
- **Chat:** streaming replies, tool cards, diffs, permission and question cards, mode and model selectors (switching to a bypass mode asks to confirm) and `/` commands.

## Quick start

```sh
ig init                          # ig.yaml, the agent skills, an AGENTS.md snippet, .gitignore entries
ig analyse                       # read the code and write ig/ for you to review
ig view                          # draw the map, write .ig/view.html and open it
ig explore "how does X work"     # the answer on stdout, drawn on the map
```

`ig analyse` is the only command that writes `ig/` from the code. Review it with `git diff ig/` and commit it. `ig app .` opens the desktop app instead of the browser.

## `ig analyse`

```
ig analyse [--agent claude|gemini|codex|ollama] [--no-agent] [-C <dir>] [-q]
```

`ig analyse` (also `ig analyze`) is the one command that writes `ig/` from the code. `ig init`, `ig view`, `ig plan` and the desktop app never do.

### Reading and consent

An agent only runs when you say so:

- No `ig/` yet: the agent reads every box and `ig analyse` writes the whole `ig/`.
- `--agent <name>` runs that agent without asking. `--no-agent` never runs one: a new `ig/` is ig's reading of the code alone.
- Plain `ig analyse` in a terminal asks `Let an AI agent (claude) read the architecture? [Y/n]`. A "no" writes the code-only `ig/`. Without a terminal (a script, CI) and without `--agent` or `agent:` in `ig.yaml`, it runs as `--no-agent`, with a notice.
- `ig/` exists: the agent reads the drift and the boxes with no description and proposes edits to `ig/` (adds and new titles, descriptions, tech). They are written, journaled as the reading's, and you review them with `git diff ig/`.
- `ig/` exists and no agent runs (`--no-agent`, a "no", no agent installed): nothing is written, a notice says so, and the exit is 0, so a script can run it twice.
- On a folder of projects, it reads each project that has no `ig/`.

`--agent` picks the CLI; otherwise `agent:` in `ig.yaml`, otherwise the first of `claude`, `gemini`,
`codex` on `PATH`. The agent gets read-only tools and skips the repo's own agent configuration where
its CLI allows: Claude runs with `--restricted --safe-mode --tools Read,Grep,Glob`; Codex with
`--sandbox read-only`; Gemini with default approvals and no extensions. The Gemini and Codex command
lines come from their documentation and have not been run against the real CLIs. A bad answer is
retried once; whatever is still not valid is dropped, with a notice.

## `ig explore`

```
ig explore [--agent claude] [--no-open] [-C <dir>] [-q] "<question>"
```

Asks Claude a question about the code. Claude gets `Read`, `Grep`, `Glob`, `ig_state` and `ig_show` and nothing else; the repo's own hooks and settings do not load. The short answer goes to stdout, `ig_show` draws it, and ig writes `.ig/show.json` and `.ig/view.html` and opens the map on the answer (`--no-open` skips that). Claude is the only agent for now. It works before `ig analyse`, on the map of the code.

## `ig diff`

```
ig diff [<base> [<head>]] [--json] [-C <dir>] [-q]
```

Shows what a change does to the architecture. It builds the map on both sides and compares them: boxes added, removed or changed (a box changes when a file under it does), arrows added or removed, what changed in `ig/`, and the problems the change brings or fixes. The same code gives no change.

`<base>` is any commit; without it, the point where this branch left `main` (or `HEAD`, with a notice). `<head>` is a commit too; without it, the working tree, uncommitted files included. Both sides are built with the head's `ig.yaml` (the working tree's, or the head commit's when `<head>` is given), so a config change is not an architecture change. Each commit is read in a temporary checkout: ig never touches your branches, index or working tree. Boxes get their titles from `ig/` when both sides have one, and from the code otherwise.

```
03d4b9d..c251685: 2 boxes changed, 1 new arrow Web → Core
  changed  Src (core/root) (1 file)
  changed  Src (web/root) (1 file)
  arrow +  Web → Core (bundle)
```

Two boxes with the same title get their ids after it. `--json` prints the same as one object: `base`, `head`, `source`, `summary`, `boxes`, `arrows`, `intent` and `problems`; `base` and `head` are full commit shas (`head` is null for the working tree). Exit is 0 whatever the diff holds, 2 on a bad revision, outside git, or in a folder of projects.

## `ig view`

```
ig view [--no-open] [--json] [-C <dir>] [-q]
```

`ig view` is read-only: it never runs an agent and writes only `.ig/view.html`, one offline page. Without an `ig/` it draws the map of the code and says `run ig analyse`. It builds the map in three stages:

1. **Facts (always, no AI).** The graph gives packages, the folders inside them, imports, calls and
   file counts. Every box and arrow exists after this stage, laid out and drawn.
2. **Reading (an AI agent, from `ig analyse`).** The agent reads files with read-only tools and fills in each
   box's title, description, kind, tech, badges, logo and the pattern of its inside (`onion`,
   `hexagonal`, `layered`, or none). Every value in the map carries its source (`agent` or `ig`) and a confidence:
   `extracted` (read from a file), `inferred` (a judgment) or `ambiguous` (the agent was unsure).
3. **Layers (always).** Words drawn on boxes and arrows: `rules` (imports breaking a box's pattern,
   `ig.yaml` rules) and `risks` (ig's own code analysis), computed each run; and your own, from
   `.ig/layers/*.json` (below).

**The spec `ig/` is committed.** The first `ig analyse` writes the intended architecture as `ig/*.kdl`:
from `.ig/map.json` when there is one (its readings and corrections kept), else from the code and
the AI reading. After that `ig/` is yours: edit it by hand and commit it. ig changes it only
through edits you ask for (`ig analyse`, `ig accept`, `ig fmt`, Claude's `ig_edit`), each
in place: the nodes an edit touches change, every other line and comment keeps its text. Every run
compares it with the code: what both have is built, what only `ig/` says is still to build, what
only the code has is drift, and a box whose `at` changed since the last commit with the code still
at the old place is to move. `.ig/map.json` is no longer written; delete it once `ig/` is committed. A broken
`ig/` is left as is, with the file and line of each problem, and the map shows the code alone.

The hidden `--no-read` is still accepted for one release, with a notice, because skills copied by older `ig init` runs pass it.

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

### Connectors: what is running

`connectors` in `ig.yaml` puts a live server on the map, next to the code and the Ansible:

```yaml
connectors:
  - { kind: incus, remote: homelab }          # the instances of `incus list homelab:`
  - { kind: traefik, file: traefik-dynamic.yml } # routes, from a file in the repo
```

- A service is one box: a role or box named like an instance (`services/gitlab` and `gitlab`) shows
  the instance's state, running or stopped. Names are matched exactly, never loosely.
- To link by hand, give the box `runs="incus:homelab/gitlab"` in `ig/*.kdl`; several instances are
  separated by spaces. A `runs=` instance that is not found gets a warning.
- An instance no box describes is drawn in a group named after its remote, marked "not in ig/".
- Traefik's routes sit inside its box, each wired to the box it reaches. A route that reaches
  nothing is warned: the file may be stale.
- A server that cannot be reached is a notice; the rest of the map still draws.
- `incus list` shows the remote's current project only, so instances in other projects are not drawn.

Connectors only read. They run in `ig view`, not in the desktop app, and never in `ig plan`,
`ig check`, `ig accept` or MCP. They never write to `ig/`. `ig.yaml` cannot name a program: `remote`
is a name of letters, digits, `_` and `-`, and `file` is a relative path with no `..`.
ig reads only each instance's name, type, status and addresses. It never reads an instance's
`config`, `devices`, environment variables or Docker env, and addresses stay out of the page.

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

```
Usage: ig [OPTIONS] [COMMAND]

Commands:
  init     Set this repo up: ig.yaml, the agent skills, the AGENTS.md snippet
  analyse  Read the code and write ig/ (or propose updates to it) for you to review [alias: analyze]
  view     Draw the map and open it
  explore  Ask a question; the answer is drawn on the map
  diff     Show what a change does to the architecture
  plan     Show what to build, move and accept to bring code and map together
  check    Exit 1 when the code and the map have drifted apart
  accept   Take drift into the map
  fmt      Rewrite the ig/ files in canonical form
  app      Open the desktop app on a project

Options:
  -C, --root <DIR>  Run in DIR instead of the current folder
  -q, --quiet       Print results only: no notices or warnings
  -h, --help        Print help
  -V, --version     Print version

Plumbing:
  mcp          Serve ig to an agent over MCP (stdio)
  plugin       Write the agent plugin files
  completions  Print a shell completion script
```

| Command | Reads and writes |
|---|---|
| `ig init [claude\|codex\|all]` | `ig.yaml`, the explore and view skills for the chosen agents (default all), an `AGENTS.md` snippet, `.gitignore` entries. Never overwrites your edits, never writes `ig/`. |
| `ig analyse [--agent <name>] [--no-agent]` | Writes `ig/` (above). |
| `ig view [--no-open] [--json]` | Writes `.ig/view.html`. `--json` prints the map instead. |
| `ig explore [--agent claude] [--no-open] <question>` | Writes `.ig/show.json` and `.ig/view.html`; prints the answer. |
| `ig diff [<base> [<head>]] [--json]` | Read-only. Prints what a change does to the map; `--json` prints it as JSON. Exit 2 outside git. |
| `ig plan [--json]` | Read-only. Prints the plan: boxes and arrows still to build, boxes to move (their `at` changed since the last commit), drift (code in no box, arrows `ig/` does not have) and code ig does not read (outside every package). Exit 2 without `ig/`. |
| `ig check [--json]` | The plan, read-only: exit 1 when anything is left (CI), 2 on a broken or missing `ig/`. |
| `ig accept [--all \| <id>...]` | Takes drift into `ig/`: each box or arrow id from `ig plan` (quote arrow ids: `ig accept 'web->api'`), written one by one. |
| `ig fmt [--check]` | Re-indents `ig/*.kdl` (four spaces a level, at most one blank line between nodes); comments and order stay. `--check` writes nothing and exits 1 when a file would change. |
| `ig app [<dir>]` | Opens the desktop app on a folder. |
| `ig mcp` | MCP server on stdio: `ig_state` and `ig_show` (a coding agent draws an answer on the map: boxes, arrows and steps; ids are box ids from `ig view --json` or code graph ids; it draws before `ig/` exists), `ig_plan` (the plan) and `ig_edit` (Claude edits `ig/`: add, remove, move, rename, set, arrow, unarrow; its edits are proposals you review with `git diff ig/`). |
| `ig plugin export <dir>` | Writes the current Claude plugin (skills, `.mcp.json`) into `<dir>`, overwriting. The desktop app uses this for its own copy. |
| `ig completions <shell>` | Prints a completion script (bash, elvish, fish, powershell, zsh). |

`ig <dir>` no longer opens the app: use `ig app <dir>`. Every command accepts `-C`/`--root <dir>` and `-q`; short options group (`-qC dir`) and `--` ends options. Exit codes: 0 ok, 1 `check` or `fmt --check` found something, 2 usage or error. Results go to stdout, notices and errors to stderr.

**Edits and the journal.** Every edit to `ig/` (`ig accept`, the reading's, Claude's) is recorded
in `.ig/journal.json`: who made it (person, claude or reading), the edit and the edit that undoes it,
whether you kept it and whether it is undone. It is local and not committed (`.ig/` is ignored);
losing it loses undo, never the spec.

## Config (`ig.yaml`)

```yaml
agent: claude                 # optional: the CLI for `ig analyse` (claude | gemini | codex)
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
cd viewer && pnpm run test         # viewer model tests (Vitest); pnpm run typecheck for tsc
ig view --json > viewer/dev-data.json && cd viewer && pnpm run dev   # the viewer live in Vite
```

Layout: a Cargo workspace in `crates/`. `ig-core` is pure domain plus the ports; `ig-app` holds the use cases;
`ig-lang-ts`, `ig-lang-py`, `ig-lang-rs`, `ig-fs` and `ig-git` are adapters; `ig` is the binary that wires them.
The dependency rule (core and adapters depend only on `ig-core`) and a 600-line file limit are enforced by
`crates/ig/tests/architecture.rs`. A language is a `LanguageFrontend`: it says which files are its source, which packages the tree declares (npm/pnpm workspaces, `pyproject.toml`, Cargo workspaces) and which manifests fence a nested project off, then builds their nodes and edges; core and `ig-fs` know no language. A new one goes in `ig_wire::FRONTENDS`. Still TS-shaped: `prepare_checkout`'s `link_auto_imports` flag, and outside systems known by one list of npm, Python and Rust client library names.
See [Architecture](docs/specs/design.md#architecture).
