# intent-graph (`ig`)

Code graph and change checker for TypeScript/JavaScript monorepos, written in Rust.

`ig` parses the repo with oxc into a graph of modules, classes, functions, methods, tests and your own
pattern nodes, then checks each change against it: architecture layers, complexity, `@intent` coverage,
your linters (SARIF) and structural regressions. The result shows up in the terminal, in an HTML viewer,
over MCP, in a Claude Code Stop hook and in a GitLab MR note.

Design: [docs/specs/design.md](docs/specs/design.md)

## Install

```sh
make install   # builds the viewer page (viewer/dist/index.html), then `cargo install --path crates/ig --locked`
```

`ig` needs `git` on `PATH` and nothing else.

## Build

`make build` (needs bun and cargo): the viewer first (`viewer/`, TypeScript, Lit and Vite,
built into one offline HTML file), then `cargo build --release`, which embeds it. `make test` builds the
viewer, runs its tests, then `cargo test --workspace`. Without bun, outside make: `cd viewer && npm install && npm run build`.
Without a built viewer, `cargo build` still works and `ig view` writes a placeholder page with the data as JSON.

## Quick start

```sh
ig init            # ig.yaml, intents.md, Stop hook, ig skills (/ig:setup, $ig-setup), rules.md, AGENTS.md snippet, .gitignore, baseline
ig setup claude    # let Claude Code (or: ig setup codex) explore the repo and populate ig.yaml
ig check           # findings that touch your change (exit 1 if blocking)
ig check --agent   # the same, written for a coding agent
ig view            # open the change as a graph in the browser
```

## Commands

| Command | Does |
|---|---|
| `ig check [--base <ref>] [--head <ref>] [--all] [--json] [--agent]` | Builds the head and base graphs, diffs them, and reports findings. Exit 1 if any finding is at or above `blockOn`. |
| `ig brief [--md] [--out <file>]` | The change as architecture (by lane/layer), each changed function's own diff and each new function's code in architecture order, and sequence diagrams (Mermaid in `--md`). |
| `ig diff [--base <ref>] [--head <ref>] [--json]` | Changed nodes grouped by file (`+` added, `~` changed, `-` removed). |
| `ig review [--all] [--json] [--agent]` | Ranked structural problems: cycles, hubs, unstable dependencies, low cohesion, change coupling, unused exports, oversized modules. |
| `ig query <id> [--in\|--out\|--both] [--depth N] [--json]` | Neighbours of a node (default both, depth 2) as `kind id (file:line)` and `from -kind-> to`. Unknown id: suggestions, exit 1. |
| `ig plan check <file> [--json]` | Checks a plan file before any code is written (see [Plans](#plans)). Exit 1 if it has errors. |
| `ig plan notes [--wait] [--json]` | The open notes on the current plan (`plans/<slug>.notes.md`). `--wait` blocks until a new one arrives (exit 1 after 30 min without one). |
| `ig apply start [<plan>]` | Starts applying a plan (default: the current one): records the base commit and the guarded files in `.ig/apply.json` (see [Apply](#apply)). |
| `ig apply accept` | Re-records the guarded files after the user approved a change to one (`ig/tamper`). |
| `ig apply review` | Stores the reviewer agent's JSON array (stdin) in `.ig/agent-review.json`. |
| `ig apply finish` | The final check; lists the exceptions added during the apply. Exit 0 marks the plan `Status: done` and ends the apply. |
| `ig apply abort` | Ends the apply without checking (removes `.ig/apply.json`), e.g. when its state is stale. |
| `ig except <fingerprint> --reason <text>` | Records an exception with its reason in `.ig/baseline.json`; `ig brief` lists it. Tool and review findings only (`ig/plan` and `ig/tamper` are refused). |
| `ig focus <id>...` | Writes `.ig/focus.json`, the ids the viewer should highlight. |
| `ig view [--watch] [--no-open] [--json] [--checkers] [--plan <file> \| --no-plan]` | Writes `.ig/view.html`. `--watch` is `ig serve` that also opens the browser. `--json` prints the page's data instead. Checkers don't run: their findings come from the last `ig check` (those still in the change); `--checkers` runs them. |
| `ig serve [--port N] [--checkers] [--plan <file> \| --no-plan]` | The live viewer on `http://127.0.0.1:7420/` (see [Live plan co-design](#live-plan-co-design)). If 7420 is taken by another program it uses a free port; an explicit `--port` that is taken is an error. |
| `ig serve claude` | Prints and runs `claude --dangerously-load-development-channels server:ig`, so viewer notes may reach the session (unverified, see below). |
| `ig report --gitlab` | Checks `HEAD` and creates or updates one MR note (needs `IG_GITLAB_TOKEN` or `GITLAB_TOKEN`). |
| `ig mcp` | MCP server on stdio with `check`, `get_findings`, `get_intents` and `graph_query`, plus `ig_state`, `ig_notes`, `ig_note_done`, `ig_focus` and `ig_wait_notes`, which talk to a running `ig serve`. |
| `ig hook claude` | Claude Code Stop hook: blocks the agent on blocking findings, up to `hookMaxRounds`. |
| `ig init [claude\|codex\|all]` | Scaffolding, including the `ig-setup` skill for the chosen agents (default all). Never overwrites anything. |
| `ig setup [--json]` | Facts for writing `ig.yaml`: packages, directories, page candidates, unlayered modules, unmapped external packages, HTTP calls, arch violations. |
| `ig setup claude\|codex` | Starts that coding agent on the `ig-setup` skill (`/ig:setup` in Claude Code, `$ig-setup` in Codex), which populates and validates `ig.yaml`. |

Every command accepts `--root <dir>`. Exit codes: 0 ok, 1 blocking findings, 2 usage or error.

**Base selection:** `--base`, else `CI_MERGE_REQUEST_DIFF_BASE_SHA`, else the merge-base of `HEAD` with
`origin/HEAD`, `main` or `master`, else `HEAD`. Base graphs are cached in `.ig/cache/`.

## Plans

A plan is a Markdown file, usually `plans/<slug>.md`, written by the `/ig:plan` skill (`$ig-plan`):
a `# Title`, an optional `Status: done` line, a numbered list under `## Steps`, and one fenced
block tagged `ig-plan`:

```yaml
nodes:
  - id: packages/core/src/export.ts#exportRows   # graph ids: file, file#Name, file#METHOD /path
    status: new                                  # new | changed | removed
    kind: function                               # optional
    exported: true                               # optional
  - id: packages/core/src/repo.ts#Repo.find
    status: changed
edges:
  - from: packages/core/src/export.ts#exportRows
    to: packages/core/src/repo.ts#Repo.find
    kind: calls                                  # default calls
systems: [PostgreSQL]
```

`ig plan check` merges the planned nodes and edges onto the current graph and reports the volume
(new files and nodes, changed and removed nodes, new systems, new cross-layer and cross-lane links),
the blast radius (nodes that reach a changed or removed node, by layer), `ig/arch` findings on
planned edges, structural regressions, and `plan/unknown-node` / `plan/unknown-edge-end` errors for
ids that are not in the graph. It records the plan in `.ig/current-plan`.

## Apply

`/ig:apply` (`$ig-apply`) implements a checked plan: `ig apply start`, `ig check --agent` after every
step, then a reviewer agent judges the change against `rules.md`. In Claude the `ig:reviewer`
subagent (installed by `ig init` in `.claude/skills/ig/agents/`) runs; in Codex the skill calls
`codex exec --sandbox read-only`. Its JSON array is piped into `ig apply review` (stored in
`.ig/agent-review.json`, findings `review/<rule>` block like any other). `ig apply finish` exits 0
only when nothing blocks and the review matches the current change. Codex has no Stop hook, so the
skill runs `ig check --agent` itself before ending a turn.

## Workflow

| Skill (Claude / Codex) | Does |
|---|---|
| `/ig:setup` / `$ig-setup` | Populates and validates `ig.yaml`. |
| `/ig:explore` / `$ig-explore` | Explains how code fits together from the graph; read-only. |
| `/ig:plan` / `$ig-plan` | Writes a plan and runs `ig plan check`. |
| `/ig:apply` / `$ig-apply` | Implements the checked plan, then reviews it. |
| `/ig:view` / `$ig-view` | Opens `ig view` and says what it shows. |

`ig view` shows a state in the header: `Planning` (a checked plan, no apply yet: dashed nodes are
planned), `Applying` (`ig apply start` ran: solid nodes are built, dashed ones are still missing),
`Done` (the plan has `Status: done`: code diffs) or no plan. `ig focus <ids>` makes the next
`ig view` open in Focus mode on those ids, once. `ig view --watch` follows plan and apply changes.

### Live plan co-design

`ig serve [--port N]` (default 7420) serves the viewer on loopback and rebuilds on every save,
re-reading the current plan, its notes and the apply state. In the Planning state you can
right-click a box or link to comment on it or remove it, drag a box into another layer row,
shift-drag from one box to another to draw a link, and press `+` on a layer row to add a box.
Each of these is a note: one line in `plans/<slug>.notes.md`, committed with the plan:

```md
- [ ] N1 comment on <id>: <text>
- [ ] N2 move <id> to layer <layer>
- [ ] N3 link <from> -> <to> (<kind>)
- [ ] N4 remove <id>
- [ ] N5 add <kind> <name> in <file or layer>
```

The agent applies a note to the plan, ticks it (`- [x]`) and may append ` — <reply>`. It reaches
the server through `ig mcp` (the plugin's `.mcp.json`, written by `ig init claude`). Started with
`ig serve claude`, Claude Code may also get each new note pushed into the session as a channel event.
This is **unverified**: channels are a research preview (interactive sessions only) and the launch flag
has not been tested against a real session. `server:ig` needs an `ig` MCP server in scope, e.g. from
`claude mcp add ig -- ig mcp`; when the server comes only from the plugin the value may be `plugin:ig:ig`
(run `claude --dangerously-load-development-channels plugin:ig:ig` yourself). Without the channel the
plan skill waits with `ig_wait_notes`, which works either way.

The API (`GET /api/state`, `GET /api/notes`, `POST /api/notes`, `POST /api/notes/<id>`,
`POST /api/focus`, `GET /api/events`) answers only on `127.0.0.1` or `localhost`. Every POST needs
the `X-IG-Token` of the run (in `.ig/serve.json` and in the served page) and a local `Origin`, so
other web pages cannot post notes to your agent.

Trust: open notes in a committed `plans/*.notes.md` are relayed to the agent as instructions to apply
to the plan, so anyone who can commit one can talk to your agent. Review notes files like code.

## Intents

`intents.md` holds one intent per line:

```md
- AUTH-1: Users can sign in with email
```

Tag the code and the tests that implement it:

```ts
// @intent AUTH-1
export async function signIn(email: string) { … }
```

`ig check` reports intents that are changed in `intents.md` but not implemented or not tested, and tags
that `intents.md` doesn't declare.

## Config (`ig.yaml`)

```yaml
layers:
  ui: ["@acme/ui"]            # a workspace package name, or a glob
  core: ["packages/core/**"]
rules:
  - forbid: "core -> ui"      # core must not import, call or render ui
complexity: { max: 15, maxIncrease: 5 }
embedded: [".vue"]            # single-file components
autoImports: [".nuxt/imports.d.ts", ".nuxt/components.d.ts"] # framework auto-imports (generated d.ts; run `nuxt prepare`)
patterns:                     # ast-grep patterns that become graph nodes
  - kind: route
    match: "app.$METHOD($PATH, $$$)"
    where: { METHOD: [get, post] }
    id: "{METHOD} {PATH}"
checkers:                     # any command that prints findings on stdout (a non-zero exit with no findings is an error)
  - name: stylelint
    run: "npx stylelint --formatter json --allow-empty-input {files} 2>&1" # {files}: the changed files (see below)
    format: stylelint-json    # sarif (default) | checkstyle | eslint-json | stylelint-json | lines
    configs: ["**/.stylelintrc*"] # the tool's own config files, watched for tampering
    extensions: [".css", ".scss"] # which changed files {files} keeps (optional)
blockOn: error
blueprint:                    # optional: page → front → API → use cases → adapters → systems
  lanes:
    - { name: User, match: ["apps/web/src/pages/**/*.vue"], exclude: ["**/components/**"] }
    - { name: Front, layers: [ui] }
    - { name: API, match: ["services/api/src/routes/**"] }
    - { name: Core, layers: [core] }
  http: { calls: [fetch, useFetch, $fetch], routes: route, prefix: /api }
  systems:
    PostgreSQL: [pg, knex]
    Stripe: [stripe]
```

`{files}` in `run` becomes the changed files that checker cares about: files with added or changed nodes, plus files `git diff --name-only <base>` lists and untracked files, single-quoted, space-separated, relative to the root. A checker keeps the files matching its `extensions` (e.g. `[".ts", ".vue"]`); without `extensions`, graph files plus `.ts .tsx .js .jsx .mjs .cjs .vue .css .scss .less`. No such file: the checker is skipped. With `--all`, or when the command would pass 100 KB (with a notice), `{files}` is `.`. `ig view` and `ig serve` don't run checkers (see the commands above). On a terminal, `ig` prints progress (graph, base graph, each checker and its time) on stderr.

The viewer (`ig view`) has four modes: **Changed**, **Architecture** (drill down by layer and directory),
**Focus** (one node's neighbourhood) and **Blueprint** (pick a page, see everything behind it in swimlanes).
A file tree sits alongside, and each function shows its code (before/after diff when it changed) and a
call chain down to systems (copyable as Mermaid or PlantUML). Everything works offline from one HTML file.

All keys, with defaults, are listed in the [design](docs/specs/design.md#igyaml).
Known findings can be accepted by adding their fingerprints to `.ig/baseline.json`.

## Integrations

**Claude Code:** `ig init` adds the Stop hook to `.claude/settings.json`:

```json
{ "hooks": { "Stop": [{ "hooks": [{ "type": "command", "command": "ig hook claude", "timeout": 300 }] }] } }
```

**MCP:** `ig init claude` adds `.claude/skills/ig/.mcp.json`; without the plugin, `claude mcp add ig -- ig mcp`

**GitLab CI:**

```yaml
ig:
  rules: [{ if: $CI_MERGE_REQUEST_IID }]
  script: ig report --gitlab
  artifacts: { when: always, paths: [.ig/view.html] }
```

## Development

```sh
cargo test --workspace             # unit tests plus golden fixture tests
UPDATE_GOLDEN=1 cargo test         # rewrite crates/ig/tests/golden/ after an intended change
cd viewer && bun run test          # viewer model tests (Vitest); bun run typecheck for tsc
ig view --json > viewer/dev-data.json && cd viewer && bun run dev   # the viewer live in Vite
```

Layout: a Cargo workspace in `crates/`. `ig-core` is pure domain plus the ports; `ig-app` holds the use cases;
`ig-lang-ts`, `ig-fs`, `ig-git` and `ig-checkers` are adapters; `ig` is the binary that wires them.
The dependency rule (core and adapters depend only on `ig-core`) and a 600-line file limit are enforced by
`crates/ig/tests/architecture.rs`. `LanguageFrontend` is the seam for new languages, but it is not drop-in yet: source-file detection (`ig_core::workspace::is_source_ext`) and package discovery (`ig-fs`: npm/pnpm workspaces, package.json, tsconfig) are still TS/npm-shaped, and `prepare_checkout`'s `link_auto_imports` flag and the node hash/intent helpers in `ig-lang-ts` must move first.
See [Architecture](docs/specs/design.md#architecture).
