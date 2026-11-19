# ig reference

Everything `ig` does, command by command. The [README](../README.md) is the tour.

## A folder of projects

`ig view` on a folder that holds several projects, and is not a project itself, draws them on one map: a box per project with its own map inside, what runs the others on a band below, and dashed links where a project's files name another. A project need not be a git repo: a manifest or Terraform files are enough. Worktrees, archives, downloaded releases and folders without source are left out; the Folders panel says why. Secrets are never opened (`.env*`, `*.tfstate*`, `*.tfvars`, keys). `ig view` writes no `ig/`; `ig analyse` on the folder reads each project that has none and writes its `ig/`.

## Build

`make build` (needs pnpm, node and cargo): the viewer first (`viewer/`, TypeScript, Svelte and Vite,
built into one offline HTML file), then `cargo build --release -p intent-graph`, which embeds it. `make test` builds the
viewer, runs its tests, then `cargo test --workspace`. Dependency build scripts stay off; the supply-chain settings are in `viewer/pnpm-workspace.yaml`.
Without a built viewer, `cargo build` still works and `ig view` writes a placeholder page with the data as JSON.

Desktop app: see [Desktop app](#desktop-app). The workspace includes it, so
`cargo build` and `cargo test --workspace` compile Tauri too (on Linux that needs webkit2gtk).

## Desktop app

`crates/ig-desktop` is a Tauri 2 app: the map, editable (boxes, arrows, rename, move, delete, undo), beside a chat panel that runs Claude Code on your project. `ig app [<dir>]` opens it on a folder: it runs `$IG_APP` if set, else the app built next to this `ig` (`target/<profile>/ig-desktop`), else the installed `ig.app`.
Each release ships it for macOS (Apple silicon and Intel), unsigned and without auto-update:

```sh
brew install --cask mrzdevcore/tap/ig-app   # ig.app in /Applications
```

The cask clears the quarantine flag so Gatekeeper opens the unsigned app. The zips are also on the release page; one unzipped by hand needs `xattr -dr com.apple.quarantine ig.app` once.

From source:

```sh
make app                               # target/release/bundle/macos/ig.app (unsigned)
make app TARGET=x86_64-apple-darwin    # target/x86_64-apple-darwin/release/bundle/macos/ig.app
make app-dev                           # run it in development
```

The app ships its own `ig` (`Contents/MacOS/ig`; `target/debug/ig` in development) and puts it first on the agent's `PATH`, so the plugin's skills and `ig mcp` match the app even when it is opened from Finder.

- **Needs:** Rust, pnpm, node and npm (for the Claude adapter), and the Claude CLI logged in (run `claude`, then `/login`). The app never sees your credentials.
- **Adapter:** on the first chat, if it is missing, the app offers to install `@agentclientprotocol/claude-agent-acp@0.95.0` into its own data dir (never global).
- **Trust screen:** before any Claude session the app lists what will run: hooks from `.claude/settings*.json`, permission settings (including bypass modes), `AGENTS.md`/`CLAUDE.md`/`CLAUDE.local.md`, MCP servers from `.mcp.json`, the ig plugin, and the project's skills, commands and agents (with any tools they pre-approve or hooks they run). Every file under `.claude/` is hashed (files over 1 MB by size only). You decide, and confirm natively. It asks again when any of them changes.
- **Answers on the map:** when the agent calls `ig_show`, the map flies to the boxes and arrows in its answer, lights them with step numbers and fades the rest; a card lists the steps (←, → to walk, Esc to clear). If you are editing, the answer waits for you. `ig view` opens on an answer less than an hour old. The app writes no `ig/` when it opens a project: run `ig analyse` first.
- **Plugin:** the app always uses its own ig plugin (re-exported on each run into its data dir). Run `ig init` after upgrading ig to refresh a project's copy for terminal use.
- **Settings:** sessions load the project and local settings plus the ig plugin only. Your own skills and user settings do not load.
- **Chat:** streaming replies, tool cards, diffs, permission and question cards, mode and model selectors (switching to a bypass mode asks to confirm) and `/` commands.

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

## `ig apply`

```
ig apply [--yes] [--allow-bash] [--agent claude] [-C <dir>] [-q] [<id>...]
```

Builds what `ig plan` lists, as `terraform apply` follows `terraform plan`. The items are the plan's "to build" boxes and arrows and its moves, or only the ids named: a box by its id, an arrow as `from->to` (quote it: `ig apply 'web->api'`; it picks every arrow between that pair), the ids `ig accept` takes. An id the plan does not hold is an error that lists the plan's ids. With nothing to build it says so and starts no agent.

It prints the items and asks before going on, because the agent writes code: `--yes` skips the question, and when stdin is not a terminal and there is no `--yes` it stops (exit 2). Claude then gets the apply skill's instructions and, for each item, the text `ig/` gives it (title, tech, description, `context`; an arrow's `label`, `note`, `context`). Run it only on an `ig/` you trust: its text reaches the agent as instructions. Its tools are `Read`, `Grep`, `Glob`, `Edit`, `Write`, `ig_plan` and `ig_state`; `Bash` only with `--allow-bash`, to build and run the tests. `ig/` and `ig.yaml` are denied to `Edit` and `Write`, `ig_edit` and `ig_show` are denied, and `Read` is denied the secret files as in `ig analyse`. `--allow-bash` gives an otherwise unrestricted shell: only `git push`, `git commit`, `ig accept` and `ig fmt` are denied, and a command can still write `ig/` or read a secret file. The repo's own settings and hooks do not load.

Afterwards ig prints Claude's summary and the new plan, also when Claude failed or timed out (after 30 minutes): what is still to build and the drift the change brought, for you to `ig accept` or fix. When `ig/` or `ig.yaml` changed during the run, ig says so and exits 2, leaving the change for you to review with `git diff ig/ ig.yaml`. Exit 0 when every chosen item was built, 1 when some are left (or you said no), 2 on error. ig never commits. Claude is the only agent for now; the skill (`/ig:apply`, `$ig-apply` in Codex) works in either.

## `ig diff`

```
ig diff [<base> [<head>]] [--json] [--no-open] [-C <dir>] [-q]
```

Shows what a change does to the architecture. It builds the map on both sides and compares them: boxes added, removed or changed (a box changes when a file under it does), arrows added or removed, tables and views added, dropped or changed column by column, what changed in `ig/`, and the problems the change brings or fixes. The same code gives no change.

`<base>` is any commit; without it, the point where this branch left `main` (or `HEAD`, with a notice). `<head>` is a commit too; without it, the working tree, uncommitted files included. Both sides are built with the head's `ig.yaml` (the working tree's, or the head commit's when `<head>` is given), so a config change is not an architecture change. Each commit is read in a temporary checkout: ig never touches your branches, index or working tree. Boxes get their titles from `ig/` when both sides have one, and from the code otherwise.

```
03d4b9d..c251685: 2 boxes changed, 1 new arrow Web → Core
  changed  Src (core/root) (1 file)
  changed  Src (web/root) (1 file)
  arrow +  Web → Core (bundle)
```

A change to the database's schema adds a line per table:

```
  view  +  v_batches_notes_impacts
  table ~  batches_notes: + impacts varchar[], ~ gap_minutes (check), − impact
```

It then writes `.ig/diff.html`, the head's map with the change marked and a card that steps through it, and opens it (`--no-open` writes it only). `ig view` afterwards shows the plain map again.

Two boxes with the same title get their ids after it. `--json` prints the same as one object, and writes no page: `base`, `head`, `source`, `summary`, `boxes`, `arrows`, `tables` (only when a table changed), `intent` and `problems`; `base` and `head` are full commit shas (`head` is null for the working tree). Exit is 0 whatever the diff holds, 2 on a bad revision, outside git, or in a folder of projects.

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

A box drawn before its kind is chosen is kind `box` ("Not decided yet"): it sits wherever a block that runs or a design block may, holds either, and never draws a misplaced warning. `ig plan` lists it as `to build: box <id>, kind not decided`.

Any box can carry `context`: free text of any length for whoever builds it, written in its body (`context "…"`, or a KDL raw multi-line string, `context #"""` … `"""#`, when it has line breaks). `description` stays the one line shown on hover. An arrow takes `label`, a `note` (one line, at most 80 characters, shown on the map under the label) and a `context` (free text) and a `shape` (`circuit`, `elbow`, `curved` or `straight`: how the arrow is drawn; any other value is a read error): `calls "billing" label="charges" note="every checkout" context="…" shape="elbow"`. A longer or multi-line `note` is a read error; any other property on an arrow is a warning, and the arrow still reads.

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

### Databases

ig reads the tables a repo declares and draws each one inside the store it is kept in, as a card:
its name, then a row per column with its `PK` and `FK` badges and its type. An arrow joins a table
to each table its foreign keys point at, labelled with those columns; a view has an arrow to each
table it reads. It reads:

- **SQL**: `CREATE TABLE`, `ALTER TABLE`, `DROP TABLE` and `CREATE VIEW` in any `.sql` file.
  Migrations are applied in the order of their names (`V2__` before `V10__`), so the map shows the
  schema they end with; down and undo migrations are skipped.
- **Prisma**: the models of a `.prisma` schema (its own migrations are then not read again).
- **Django** and **SQLAlchemy** models, **Drizzle** tables and **TypeORM** entities.

The store is the database the code imports (PostgreSQL for `pg`, the provider of a Prisma
schema…), or else one named for the schema's dialect, added to the map. Never a cache, a bucket
or a search index. `ig diff` marks the columns a change adds or changes, and draws the ones it drops
struck through. Files under `ignore` or `tests` in `ig.yaml` are not read.

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
  functions; a database, its tables.
- **Esc** closes the card and gives the focus back to the box.
- **Breadcrumb** (top left) jumps to any level above you.
- **Layers** (top right) turns each layer on and off. Bottom left, **-** and **+** zoom, and **Fit** (or the zoom readout, or ⇧1) returns to the whole system.
- **Proposed edits** (bottom centre) lists the edits Claude or the AI reading wrote to `ig/` that nobody kept yet; hovering one lights it. The desktop app keeps or drops them; `ig view` only shows them and says to see them with `git diff ig/` and keep or drop them in `ig app`.
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
  apply    Let an agent build what the plan lists
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
| `ig init [claude\|codex\|all]` | `ig.yaml`, the explore, view, draw, plan and apply skills for the chosen agents (default all), an `AGENTS.md` snippet (an ig block you edited only gets the `/ig:apply` and `/ig:plan` lines it lacks), `.gitignore` entries. Never overwrites your edits, never writes `ig/`. |
| `ig analyse [--agent <name>] [--no-agent]` | Writes `ig/` (above). |
| `ig view [--no-open] [--json]` | Writes `.ig/view.html`. `--json` prints the map instead. |
| `ig explore [--agent claude] [--no-open] <question>` | Writes `.ig/show.json` and `.ig/view.html`; prints the answer. |
| `ig diff [<base> [<head>]] [--json] [--no-open]` | Prints what a change does to the map and writes `.ig/diff.html` with the change drawn on it; `--json` prints it as JSON, no page. Exit 2 outside git. |
| `ig plan [--json] [-v]` | Read-only. Prints the plan in sections: `+` boxes and arrows still to build, `~` boxes to move (their `at` changed since the last commit), `?` drift (code in no box, arrows `ig/` does not have) and `-` code ig does not read (outside every package). Arrows sit under the box they leave, each with the id `ig accept` takes; a summary line and the commands to run next end it. Colour in a terminal unless `NO_COLOR` is set. Notices are folded into one count line; `-v` prints them. `--json` keeps the flat lists. Exit 2 without `ig/`. The plan skill (`/ig:plan`, `$ig-plan` in Codex) reads the code behind each drift item and sorts it: take it into `ig/`, or fix the code. |
| `ig apply [--yes] [--allow-bash] [--agent claude] [<id>...]` | Claude writes the code for the plan's boxes and arrows to build and its moves (all, or the ids named), after you say yes; never `ig/` or `ig.yaml`. Prints its summary and the new plan. Exit 0 when every chosen item was built, 1 when some are left, 2 on error or when `ig/` changed. Run it only on an `ig/` you trust. |
| `ig check [--json] [-v]` | The plan, read-only, one line per item for CI logs and scripts: exit 1 when anything is left (CI), 2 on a broken or missing `ig/`. Notices folded as in `ig plan`. |
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
`review`) are ignored with a notice. All keys are described in the [design](specs/design.md#igyaml).

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
`ig-lang-ts`, `ig-lang-py`, `ig-lang-rs`, `ig-lang-go`, `ig-fs` and `ig-git` are adapters; `ig` is the binary that wires them.
The dependency rule (core and adapters depend only on `ig-core`) and a 600-line file limit are enforced by
`crates/ig/tests/architecture.rs`. A language is a `LanguageFrontend`: it says which files are its source, which packages the tree declares (npm/pnpm workspaces or the outermost nested `package.json` folders, `pyproject.toml`, Cargo workspaces, a folder per Go package) and which manifests fence a nested project off, then builds their nodes and edges; core and `ig-fs` know no language. A new one goes in `ig_wire::FRONTENDS`. Still TS-shaped: `prepare_checkout`'s `link_auto_imports` flag, and outside systems known by one list of npm, Python, Rust and Go client library names.
See [Architecture](specs/design.md#architecture).
