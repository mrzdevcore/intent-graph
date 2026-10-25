---
name: ig-explore
description: Explore this repository and write or improve ig.yaml (intent-graph) so ig check, ig review and ig view (Architecture and Blueprint) describe the real system - layers and forbidden dependencies, embedded components, route patterns, checkers, blueprint lanes, HTTP client calls and external systems. Use when asked to set up, populate, check or improve ig.yaml, or when ig explore reports gaps.
---

# Populate ig.yaml from the code

`ig` builds a code graph of this TypeScript/JavaScript repo. It knows no framework: everything
project-specific lives in `ig.yaml`. Your job is to make that file describe this repo accurately,
using facts from `ig explore` and from reading the code, then prove it with `ig` itself.

## Ground rules

- Work from evidence. Every layer, rule, pattern and system you add must point at real paths or
  real imports you have seen. Never invent package names or paths.
- Edit `ig.yaml` only. Never edit `.ig/baseline.json`, `intents.md` or source code.
- Keep what is already there unless it is wrong; say why when you change it.
- Prefer few, meaningful layers over one per directory.
- `ig` must accept the file: after every edit run `ig check --all --json > /dev/null`. An error
  looks like `ig.yaml: rules[0]: expected "a -> b"`. Fix it before going on.

## 1. Gather facts

```sh
ig explore            # packages, directory shape, page/view candidates, unmapped packages,
                      # URL-calling functions, unmatched HTTP calls, files without a layer
cat ig.yaml
```

Then read what the facts point at: workspace `package.json` files, `tsconfig.json` paths, a few
files per candidate layer, the router or pages directory, the HTTP client, and the server's route
declarations if the repo has a backend. Check `CLAUDE.md`, `AGENTS.md` and docs for an intended
architecture (hexagonal, feature folders, micro-fronts) and follow it.

## 2. Layers and rules

```yaml
layers:                       # name: [entries]; entry = glob, or a workspace package name
  domain: ["services/api/src/*/domain/**"]
  adapters: ["services/api/src/*/infrastructure/**"]
  web: ["@acme/web"]
rules:
  - forbid: "domain -> adapters"   # domain must not import, call or render adapters
```

- The most specific entry wins (longest literal prefix before the first `*?[{`); ties go to the
  first declared. Use that to carve a sub-layer out of a broader one.
- `ig explore` lists files without a layer. Leave out only tooling and config.
- Rules state the dependency direction the architecture intends. Interface-based dispatch
  (`dispatch` edges) never triggers them, so dependency inversion is fine.
- After adding rules, run `ig check --all` and read the `ig/arch` findings. Each one is either a real
  violation (keep the rule and report it) or a sign the layer is drawn wrong (fix the layer).
  Never loosen a rule only to make findings disappear; report them to the user instead.

## 3. Files and tests

```yaml
embedded: [".vue"]                     # single-file components with <script>
ignore: ["**/generated/**"]            # generated code, fixtures, vendored files
tests: ["**/*.test.*", "**/*.spec.*", "**/__tests__/**"]
testCalls: ["describe", "it", "test"]
entryPoints: ["apps/*/src/main.ts"]    # used from outside the repo (review skips their exports)
```

## 4. Patterns (route nodes and other framework concepts)

ast-grep patterns turn framework declarations into graph nodes. Routes matter most, because the
blueprint links HTTP calls to them. Find how the backend declares routes, then write one pattern
per style:

```yaml
patterns:
  - kind: route
    match: "{ method: $METHOD, path: $PATH, $$$REST }"     # Hapi route objects
    id: "{METHOD} {PATH}"
  - kind: route
    match: "app.$METHOD($PATH, $$$ARGS)"                    # Express
    where: { METHOD: [get, post, put, patch, delete] }     # where'd variables are upper-cased in ids
    id: "{METHOD} {PATH}"
```

Route ids must read `METHOD /path` for HTTP linking. `${CONST}` in a path is resolved from a
same-file string constant. Check the result with `ig explore` (route count) or by searching
`.ig/graph.json` for `"kind":"route"`. An invalid pattern shows up as an `ig/parse-error` on
`ig.yaml`.

## 5. Blueprint

The Blueprint view in `ig view` shows a user journey from a page down to external systems.

```yaml
blueprint:
  lanes:                                   # top to bottom; a file goes to the first lane it matches
    - name: User                           # lane 0 = journeys: routed pages or views, not components
      match: ["apps/web/src/pages/**/*.vue"]
      exclude: ["**/components/**"]
    - name: Front
      layers: [web]
    - name: API
      layers: [presentation]
    - name: Use cases
      layers: [use-cases, domain]
    - name: Adapters
      layers: [adapters]
  http:
    calls: [fetch, useFetch, $fetch]       # functions whose first argument is a URL
    routes: route                          # pattern kind of route nodes
    prefix: /api                           # prefix the client adds that route paths lack
  systems:                                 # npm packages that stand for an external system
    PostgreSQL: [pg, knex]
    Stripe: [stripe]
```

- Lane 0 is the list of journeys. Point it at routed pages or views only; exclude
  components, specs and layouts.
- `ig explore` lists the functions called with a URL first argument; put the real HTTP client
  calls in `http.calls`. It then lists HTTP calls that matched no route: fix `prefix`, `calls`
  or the route pattern until the remaining ones are calls to services outside the repo.
- Group the external packages that `ig explore` lists into systems by what they talk to: a
  database, an internal BFF or API (its SDK and interfaces packages), a SaaS (CMS, search,
  analytics, auth, payments). Skip pure utilities, UI kits, type-only helpers and dev tools.
- A repo without a backend (a front or micro-front monorepo) has no routes: model its backends as
  systems through their SDK packages, and its SDK/client packages as a lane above Systems.

## 6. Checkers (optional)

If the repo has linters that can print SARIF, wire them:

```yaml
checkers:
  - name: eslint
    run: "npx eslint -f @microsoft/eslint-formatter-sarif ."
```

Only add a checker whose formatter is already installed or trivially available; otherwise
suggest it to the user.

## 7. Verify and report

```sh
ig check --all          # must run; read arch and parse findings
ig explore              # files without a layer, unmatched HTTP calls, unmapped packages
ig view --no-open       # writes .ig/view.html (Architecture and Blueprint modes)
```

Finish with a short report to the user:
- what you changed in `ig.yaml`, section by section, and the evidence for each choice;
- the arch violations the rules surface (real ones, by rule);
- what remains unmapped and why (e.g. HTTP calls to services outside the repo);
- anything you were unsure of, as a question.

Do not regenerate `.ig/baseline.json`; if the user wants existing findings accepted, tell them to
run `rm .ig/baseline.json && ig init`.
