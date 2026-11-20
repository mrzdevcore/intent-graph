# `ig.yaml`

`ig.yaml` sits at the root of your repo and tells ig **how to read the code**. It is optional: with no
`ig.yaml` (or an empty one) ig uses the defaults below. Commit it.

It is not the architecture. The boxes and arrows you intend live in `ig/*.kdl`; `ig.yaml` only
changes what ig reads, how it names some things, and which dependencies you forbid.

| You want to… | Key |
|---|---|
| forbid one part of the code from depending on another | [`layers`](#layers) + [`rules`](#rules) |
| skip generated or vendored files | [`ignore`](#ignore) |
| say which files are tests | [`tests`](#tests-and-testcalls), [`testCalls`](#tests-and-testcalls) |
| read `.vue` / `.svelte` files, or Nuxt-style auto-imports | [`embedded`](#embedded), [`autoImports`](#autoimports) |
| turn framework routes into nodes and link HTTP calls to them | [`patterns`](#patterns), [`blueprint`](#blueprint) |
| name the outside systems your code talks to | [`blueprint.systems`](#blueprint) |
| pick the AI agent for `ig analyse` | [`agent`](#agent) |
| draw running servers on the map | [`connectors`](#connectors) |

## What `ig init` writes

`ig init` creates `ig.yaml` once and never overwrites it. In a repo with several packages it looks
like this:

```yaml
# ig (intent-graph) config. See docs for every key.
layers:
  api: ["@acme/api"]
  web: ["@acme/web"]
  shared: ["@acme/shared"]
# Forbidden layer dependencies, e.g.:
#   - forbid: "core -> ui"
rules: []
embedded: []
# ignore: ["**/generated/**"]
# patterns:
#   - kind: route
#     match: "app.$METHOD($PATH, $$$)"
#     where: { METHOD: [get, post, put, delete] }
#     id: "{METHOD} {PATH}"
```

- **`layers`** gets one entry per package ig found (an npm/pnpm workspace package, a `Cargo.toml`, a
  `go.mod`, a `pyproject.toml`, `setup.py` or `requirements.txt` next to Python code). Packages
  whose names end the same (Go modules `…/shop/genproto` and `…/billing/genproto`) share one layer.
  A repo with a single package gets one layer per folder under `src/` instead.
- **`rules`** is empty: nothing is forbidden until you say so.
- **`embedded`** lists the file extensions your code imports that ig would not read otherwise
  (`.vue`, `.svelte`).

The layers on their own do nothing visible. They are there so you can write `rules` against them
straight away. Delete, merge or rename them freely.

## Keys

### `layers`

Names parts of the code, so `rules` can talk about them.

```yaml
layers:
  ui: ["@acme/ui", "apps/web/**"]   # a package name, or a glob
  core: ["packages/core/**"]
  core-internal: ["packages/core/internal/**"]
```

- Each entry is a list. An item is either a **package name** (it stands for that package's folder)
  or a **glob** relative to the repo root.
- A file belongs to one layer at most: the **most specific** match wins (the longest text before the
  first `*`, `?`, `[` or `{`). Above, `packages/core/internal/x.ts` is in `core-internal`, not
  `core`. On a tie, the layer declared first wins.
- Files no layer matches are simply not checked by `rules`.

Layers do not create boxes on the map, and the map's **Layers** menu is something else (overlays
such as marks written by other tools).

Default: none.

### `rules`

Dependencies you forbid between layers.

```yaml
rules:
  - forbid: "core -> ui"      # core must not import, call or render ui
  - forbid: "shared -> api"
```

- The form is always `forbid: "a -> b"`, with two names from `layers`.
- An import, a call or a component render from a file in `a` to a file in `b` breaks the rule.
  Calls through an interface (`dispatch`) do not count.
- Each break shows on the map as a **rules** mark on the box and the arrow, with the file and line
  and a suggested fix.

`ig check` does not fail on broken rules; it checks the code against `ig/`.

Default: none.

### `ignore`

Globs ig never reads.

```yaml
ignore: ["**/generated/**", "vendor/**", "**/*.pb.go"]
```

Files ig already skips (`.gitignore`d files, `node_modules`, build output) need no entry. Dotfiles
match the globs too.

Default: none.

### `tests` and `testCalls`

Which files are tests, and which calls declare a test in TypeScript/JavaScript.

```yaml
tests: ["**/*.test.*", "**/*.spec.*", "test/**"]
testCalls: [describe, it, test, bench]
```

Test files are drawn as tests and do not count as uses of outside systems. Setting `tests`
**replaces** the default list, so copy what you still need from it.

Defaults:

```yaml
tests: ["**/*.test.*", "**/*.spec.*", "**/__tests__/**", "**/test_*.py", "**/*_test.py",
        "**/conftest.py", "**/tests.rs", "**/tests/**/*.rs", "**/*_test.go"]
testCalls: [describe, it, test]
```

### `embedded`

Extensions of single-file components whose `<script>` ig should read as TypeScript.

```yaml
embedded: [.vue, .svelte]    # "vue" works too: the dot is added
```

Default: none (`ig init` fills it in when your code imports such files).

### `autoImports`

Generated declaration files that list what a framework imports for you, so ig can resolve calls to
functions and components you never import by hand.

```yaml
autoImports: [.nuxt/imports.d.ts, .nuxt/components.d.ts]
```

Nuxt's `.nuxt/*.d.ts`, unplugin-auto-import's `auto-imports.d.ts` and unplugin-vue-components'
`components.d.ts` work. Generate them first (`nuxt prepare`); a missing file is a notice.

Default: none.

### `patterns`

Turn a code shape into a node of its own: an HTTP route, a job, a command handler. TypeScript and
JavaScript only.

```yaml
patterns:
  - kind: route                       # the node's kind
    match: "app.$METHOD($PATH, $$$)"  # an ast-grep pattern
    where: { METHOD: [get, post] }    # optional: allowed texts for a variable
    id: "{METHOD} {PATH}"             # the node's name; {VAR} is replaced
```

- `match` is an [ast-grep](https://ast-grep.github.io/guide/pattern-syntax.html) pattern: `$NAME`
  matches one node, `$$$` any number of them.
- In `id`, a `where` variable is uppercased (`GET`) and quotes are stripped from the others, so
  `app.get("/users", h)` becomes `GET /users`.
- The comment lines right above a match become the node's intent.
- A pattern that does not parse is reported once and skipped.

Default: none.

### `blueprint`

Links ig cannot see from imports alone.

```yaml
blueprint:
  http:
    calls: [fetch, get, post]   # functions whose first argument is a URL
    routes: route               # the patterns kind those URLs match (default: route)
    prefix: /api                # added in front of route paths by a proxy or server
  systems:
    PostgreSQL: [pg, knex]      # packages that stand for an outside system
    Stripe: [stripe]
```

- **`http`**: a call such as `fetch("/api/users")` or `axios.get(...)` (matched by the last name,
  `get`) is linked to the route node with that path, so the frontend gets an arrow to the endpoint
  it calls. It needs routes from `patterns`. TypeScript and JavaScript only.
- **`systems`**: code that imports one of these packages gets an arrow to a box named after the
  system. ig already knows many client libraries (`pg`, `ioredis`, `stripe`, `pgx`, …); use this for
  the ones it does not know, or to rename one.

Default: none.

### `agent`

The AI agent `ig analyse` uses to name and describe boxes.

```yaml
agent: claude     # claude | gemini | codex | ollama
```

`--agent` on the command line wins over it. With neither, `ig analyse` uses the first of `claude`,
`gemini`, `codex` on your `PATH`, after asking in a terminal; in a script or CI it runs without an
agent. Setting `agent:` lets it run in CI too.

Default: none.

### `connectors`

Running systems to read and draw next to the code, read-only. See
[Connectors](reference.md#connectors-what-is-running).

```yaml
connectors:
  - { kind: incus, remote: homelab }               # the instances of `incus list homelab:`
  - { kind: traefik, file: traefik-dynamic.yml }   # routes, from a file in the repo
```

Default: none.

## Errors and removed keys

- An unknown key, or a value of the wrong type, stops every command (exit 2) with the key's path:
  `ig: ig.yaml: rules[0]: expected "a -> b"`.
- Keys from older versions (`complexity`, `complexityMax`, `checkers`, `severity`, `blockOn`,
  `editor`, `hookMaxRounds`, `entryPoints`, `review`, `blueprint.lanes`) are ignored with a notice.
- Changing `embedded`, `autoImports`, `ignore`, `tests`, `testCalls`, `patterns` or `blueprint`
  invalidates ig's cache, so the next run reads the code again.
