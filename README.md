# intent-graph (`ig`)

Code graph and change checker for TypeScript/JavaScript monorepos, written in Rust.

`ig` parses the repo with oxc into a graph of modules, classes, functions, methods, tests and your own
pattern nodes, then checks each change against it: architecture layers, complexity, `@intent` coverage,
your linters (SARIF) and structural regressions. The result shows up in the terminal, in an HTML viewer,
over MCP, in a Claude Code Stop hook and in a GitLab MR note.

Design: [docs/specs/design.md](docs/specs/design.md)

## Install

```sh
cargo install --path .   # installs the `ig` binary
```

`ig` needs `git` on `PATH` and nothing else.

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
| `ig view [--watch] [--no-open]` | Writes `.ig/view.html`. `--watch` serves it on localhost and reloads on every save. |
| `ig report --gitlab` | Checks `HEAD` and creates or updates one MR note (needs `IG_GITLAB_TOKEN` or `GITLAB_TOKEN`). |
| `ig mcp` | MCP server on stdio with `check`, `get_findings`, `get_intents` and `graph_query`. |
| `ig hook claude` | Claude Code Stop hook: blocks the agent on blocking findings, up to `hookMaxRounds`. |
| `ig init [claude\|codex\|all]` | Scaffolding, including the `ig-setup` skill for the chosen agents (default all). Never overwrites anything. |
| `ig setup [--json]` | Facts for writing `ig.yaml`: packages, directories, page candidates, unlayered modules, unmapped external packages, HTTP calls, arch violations. |
| `ig setup claude\|codex` | Starts that coding agent on the `ig-setup` skill (`/ig:setup` in Claude Code, `$ig-setup` in Codex), which populates and validates `ig.yaml`. |

Every command accepts `--root <dir>`. Exit codes: 0 ok, 1 blocking findings, 2 usage or error.

**Base selection:** `--base`, else `CI_MERGE_REQUEST_DIFF_BASE_SHA`, else the merge-base of `HEAD` with
`origin/HEAD`, `main` or `master`, else `HEAD`. Base graphs are cached in `.ig/cache/`.

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
patterns:                     # ast-grep patterns that become graph nodes
  - kind: route
    match: "app.$METHOD($PATH, $$$)"
    where: { METHOD: [get, post] }
    id: "{METHOD} {PATH}"
checkers:                     # any command that prints findings on stdout (a non-zero exit with no findings is an error)
  - name: stylelint
    run: "npx stylelint --formatter json 'src/**/*.scss' 2>&1"
    format: stylelint-json    # sarif (default) | checkstyle | eslint-json | stylelint-json | lines
    configs: [".stylelintrc*"] # the tool's own config files, watched for tampering
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

The viewer (`ig view`) has four modes: **Changed**, **Architecture** (drill down by layer and directory),
**Focus** (one node's neighbourhood) and **Blueprint** (pick a page, see everything behind it in swimlanes).
A file tree sits alongside, and each function shows its code (before/after diff when it changed) and a
sequence diagram (copyable as Mermaid or PlantUML). Everything works offline from one HTML file.

All keys, with defaults, are listed in the [design](docs/specs/design.md#igyaml).
Known findings can be accepted by adding their fingerprints to `.ig/baseline.json`.

## Integrations

**Claude Code:** `ig init` adds the Stop hook to `.claude/settings.json`:

```json
{ "hooks": { "Stop": [{ "hooks": [{ "type": "command", "command": "ig hook claude", "timeout": 300 }] }] } }
```

**MCP:** `claude mcp add ig -- ig mcp`

**GitLab CI:**

```yaml
ig:
  rules: [{ if: $CI_MERGE_REQUEST_IID }]
  script: ig report --gitlab
  artifacts: { when: always, paths: [.ig/view.html] }
```

## Development

```sh
cargo test                      # unit tests plus golden fixture tests
UPDATE_GOLDEN=1 cargo test      # rewrite tests/golden/ after an intended change
```
