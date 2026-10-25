# `ig` (intent-graph) in Rust: design

Date: 2026-10-24. Status: implemented (v0.1.0).

## Intent

`ig` turns a TypeScript/JavaScript monorepo into a code graph and checks AI-written changes against it. It reports:

- architecture-layer violations;
- complexity;
- findings from the project's own linters (SARIF);
- one-line intents linked to code with `@intent` tags.

The change is shown as a graph diff in the terminal, in an HTML viewer, over MCP, in a Claude Code Stop hook, and in a GitLab MR note.

`ig` has no built-in knowledge of any framework or linter. Routes, single-file components and checkers all come from the project's `ig.yaml`.

**Speed is the primary design goal.** `ig check` runs in the Stop hook after every agent turn, in `view --watch` on every save, and in CI, so its wall-clock time sets the pace of the whole loop. The target is a full check on a large monorepo in seconds.

## Decided

These were decided with the user:

- **Language: Rust**, chosen for speed.
- **Parser: oxc** (`oxc_parser`, `oxc_ast`, `oxc_semantic`, `oxc_resolver`).
- **No type checker.** Call edges are resolved from scopes, imports and re-export chains. Typed-variable method calls are knowingly imprecise in exchange for speed (see Call resolution).

Everything else is **the author's call**, marked *(call)*. The builder can revisit those.

## Goals

- Commands: `init`, `setup`, `check`, `diff`, `review`, `view` (static and `--watch`), `report --gitlab`, `mcp`, `hook claude`, `--version`. All of them accept `--root <dir>`.
- A single static binary with no runtime dependencies besides `git`.
- Deterministic output: the same source always gives the same node IDs, hashes, edges, fingerprints and sort order.

## Non-goals

- Type checking, or shelling out to `tsc`/`tsgo`.
- Languages beyond TS/JS and configured embedded (single-file-component) files.
- Built-in framework or linter knowledge.

## Anti-goals (a failure even if it works)

- **Unstable fingerprints or node IDs.** Old findings would resurface after a baseline, and unchanged code would show as changed.
- **`resolved` edges that are really guesses.** They produce false arch findings that block the agent loop. When in doubt, emit `guessed` or nothing.
- **Slow on large repos.** That defeats the reason this exists.

## Data model

All JSON uses camelCase keys, and fields that are absent are omitted.

- **Node:** `id`, `kind` (`module` | `component` | `class` | `function` | `method` | `test` | `system` | a pattern kind such as `route`), `name`, `file` (repo-relative posix path), `range {startLine, endLine}` (1-based, inclusive), `parent?`, `package?`, `exported?`, `hash`, `metrics {loc, complexity}`, `intents[]`.
- **Edge:** `from`, `to`, `kind` (`imports` | `calls` | `renders` | `extends` | `implements` | `dispatch` | `http` | `uses`), `confidence` (`resolved` | `guessed`), `file`, `line`, `namespace?`, `reexport?`. Edges are deduplicated on from/kind/to, and `namespace`/`reexport` are OR-merged across duplicates.
- **Graph:** `schemaVersion: 1`, `toolVersion`, `commit` (sha or null), `nodes` (sorted by id), `edges` (sorted by from, kind, to), `parseErrors [{file, message}]`.
- **Finding:** `ruleId`, `severity` (`error` | `warning` | `note`), `message`, `tool`, `file`, `line`, `nodeId?`, `edge? {from, to}`, `snippet?`, `fix?`, `fingerprint`.
- **Fingerprint:** sha1 of `[ruleId, nodeId ?? file, edge.from, edge.to, message].join("|")`, where absent values are empty strings. The line number is deliberately excluded, so fingerprints survive edits above the finding.

On-disk files:

| File | Content |
|---|---|
| `ig.yaml` | Config (below). Committed. |
| `intents.md` | One intent per line: `^\s*(?:[-*]\s+)?([A-Z]+-\d+):\s*(.+)$`. Committed. |
| `.ig/baseline.json` | Sorted, deduplicated fingerprints, pretty-printed. Committed shared team state. |
| `.ig/graph.json` | The last head graph. |
| `.ig/findings.json` | The last check result, without the base graph. |
| `.ig/review.json` | The last full review. |
| `.ig/cache/<sha>-<key>.json` | Cached base graphs. `key` = the first 12 hex chars of a hash over toolVersion + `embedded`, `autoImports`, `ignore`, `tests`, `testCalls` and `patterns`. Written to a temp file, then renamed. |
| `.ig/view.html` | The self-contained viewer. |
| `.ig/hook/<session>` | Stop-hook round counter. |

`.gitignore` gets `.ig/*` and `!.ig/baseline.json`.

## `ig.yaml`

| Key | Default | Meaning |
|---|---|---|
| `layers` | `{}` | `name: [entries]`. An entry is a glob, or a workspace package name, which expands to `<pkg dir>/**`. The most specific entry wins (the longest literal prefix before the first `*?[{`); a tie goes to the first declared. |
| `rules` | `[]` | `- forbid: "a -> b"`. Layer `a` must not import, call or render layer `b`. |
| `complexity` | `{max: 15, maxIncrease: 5}` | |
| `embedded` | `[]` | Extensions of single-file components (for example `.vue`, `.svelte`). |
| `autoImports` | `[]` | Paths or globs (relative to the root) of generated declaration files listing a framework's auto-imports: Nuxt's `.nuxt/imports.d.ts` and `.nuxt/components.d.ts`, unplugin-auto-import's `auto-imports.d.ts`, unplugin-vue-components' `components.d.ts`. See Call resolution. A missing file is a parse-error notice. |
| `ignore` | `[]` | Globs that are never analysed (dotfiles match). |
| `tests` | `["**/*.test.*", "**/*.spec.*", "**/__tests__/**"]` | |
| `testCalls` | `["describe", "it", "test"]` | |
| `patterns` | `[]` | `{kind, match, where?: {VAR: [allowed]}, id}`. These are ast-grep patterns; `id` interpolates `{VAR}`. |
| `checkers` | `[]` | `{name, run, format?, configs?}`. `run` is a shell command that prints findings on stdout. `format` is one of `sarif` (default), `checkstyle`, `eslint-json`, `stylelint-json`, `lines`. `configs` is a list of globs naming the tool's own config files. |
| `severity` | `{}` | Per-rule severity overrides. |
| `blockOn` | `error` | The lowest severity that blocks. |
| `editor` | `zed {file}:{line}` | Template for viewer links. URL schemes become links; `javascript:`/`data:`/`vbscript:` never do. |
| `hookMaxRounds` | `3` | |
| `entryPoints` | `[]` | Globs of files used from outside the repo. |
| `review` | see Review | Thresholds. |
| `blueprint` | none | `lanes: [{name, match?, exclude?, layers?}]` (top to bottom, first match wins), `http: {calls, routes: route, prefix?}`, `systems: {Name: [packages]}`. See Blueprint. |

Validation errors are fatal and look like `ig.yaml: rules[0]: expected "a -> b"`. List keys must be lists, and severities must be one of `error`, `warning` or `note`.

## Architecture

There is one crate *(call)*. Modules are listed in pipeline order:

```
main        CLI parsing, dispatch, exit codes
config      ig.yaml → Config
workspace   package discovery, workspace import resolution
files       source file discovery
embedded    script extraction and markup scanning for SFCs
nodes       per-file node extraction (oxc AST)
resolve     module resolution, export tables, symbol → node
edges       per-file edge extraction
graph       two-pass build, pattern nodes, sort/dedup
patterns    ast-grep matching
git         base/head resolution, worktrees, base-graph cache
diff        graph diff
intents     intents.md parsing and changed intents
checks/     arch, complexity, intents, sarif, baseline
check       orchestration, scoping, writing .ig/ files
review/     module graph, cycles, hubs, stability, cohesion, coupling, unused, oversized, ranking, regressions
report/     mermaid, gitlab note
view/       html render, watch server
mcp         JSON-RPC over stdio
hook        Claude Code Stop hook
init        scaffolding
```

### Workspace and files

- **Packages** come from `pnpm-workspace.yaml` `packages`, or from `package.json` `workspaces` (an array or `{packages}`). Negated globs (`!x`) are supported. The directory walk skips `node_modules` and dot dirs, and its depth is bounded by the deepest pattern unless a pattern contains `**`. A directory must contain `package.json` to count. If no workspace config exists, or nothing matches, the only package is `.`.
- **Per package:** `name` (from package.json, otherwise the dir basename), `dir`, `tsconfig` (if present) and `sourceEntry`. `sourceEntry` is `exports["."]` (the first string found in nested conditions), else `exports` as a string, else `main`. That path is mapped from `outDir` back to `rootDir` as `.ts`/`.tsx` when the tsconfig sets both, falling back to `src/index.ts`. tsconfig is JSONC.
- **Workspace imports:** `pkg` or `pkg/sub` → `src/<sub>.ts[x]` when it exists, else `sourceEntry`.
- **Source files** per package: `.[jt]sx?` plus the `embedded` extensions. The walk skips `node_modules`, `dist`, dot dirs, nested package dirs and `ignore` globs. Files are sorted.

### Embedded files

These are regex-based *(call)*:

- **Script extraction:** every character outside `<script…>…</script>` bodies is replaced with a space, keeping newlines, so offsets and lines are preserved. The language is `ts` if any script tag has `lang="ts"` or `lang="tsx"`. `\nexport {};` is appended, so every component parses as a module.
- **Markup scanning:** script, style and comment regions are masked first.
  - Opening tags `<([A-Za-z][\w.-]*)(?=[\s/>])` that are PascalCase or contain `-` become `renders` edges.
  - Identifier words `[A-Za-z_$][\w$]*` (first use only) become `calls` edges from the component, when they name an import binding or a top-level declaration.

### Nodes

Node extraction is one recursive walk per file. It is parallel across files.

- **Module node:** id = the file path. The kind is `component` for embedded files, otherwise `module`. Its name is the basename. `endLine` = the line count, ignoring one trailing newline, with a minimum of 1. The hash covers the raw file (the original embedded file, not the extracted script).
- **What becomes a node:**
  - class and function declarations: overload signatures are skipped, and an unnamed default export is named `default`;
  - methods with a body, in classes or in object literals initialising `const o = {…}`. A property assigned a function in such an object is also a method;
  - `const f = <arrow|function|class expression>`. Type wrappers `( )`, `as`, `satisfies`, `!` and `<T>` are transparent;
  - `const f = wrap(<fn expr>, …)`, but only outside function bodies;
  - `export default <fn|class expr>`, named `default`;
  - in test files, calls to a name in `testCalls` (or `x.only` and similar) whose first argument is a string literal. These have kind `test`, named by the string.
- **Anchor:** a `const` statement with a single declarator anchors on the statement; otherwise the declarator is the anchor. The range, the hash and the leading comments all come from the anchor.
- **IDs:** `<file>#<qualified name>`. The qualified name is the scope (the enclosing node's qualified name, plus any named object literal) joined with `.`. Test nodes join their enclosing test titles with ` > `. A repeated ID within a file gets `~2`, `~3`, … in source order.
- **Hash:** sha1 of the anchor's source text, with no leading trivia and every whitespace run collapsed to one space.
- **Complexity:** 1, plus one per `if`, `?:`, `case`, `for`, `for…in`, `for…of`, `while`, `do`, `catch`, `&&`, `||` and `??`. Each counts toward the innermost enclosing node.
- **Intents:** `@intent\s+([A-Z]+-\d+(?:\s*,\s*[A-Z]+-\d+)*)` in the anchor's leading comments. For test calls, a comment on the first line inside the callback body also counts. Results are deduplicated in order.
- **Exported:** an inline `export` or `export default`, or a top-level name listed in `export { a }` / `export default a`. Never true for methods or tests.
- Lines are computed from oxc byte offsets. Columns are never used.

### Call resolution

Resolution happens after every file's nodes are known.

1. **Module resolution** uses `oxc_resolver`. It is configured from the package tsconfig: `paths` and `baseUrl`, the first reference for a solution-style config, `.ts/.tsx/.js/.jsx` extensions and `index` files. Workspace package specifiers are tried first. A relative import of an embedded extension resolves to that file.
2. **The export table** for each module covers local exports, `export { x as y }`, `export default`, `export * from` and `export { x } from`. Chains are followed, with a visited set to stop loops.
3. **Callee → target:**

   | Callee | Target | Confidence |
   |---|---|---|
   | local top-level `f()`, `new C()` | that declaration's node | resolved |
   | an import binding `f()` | the exporting module's node, via the export table | resolved |
   | `ns.f()` where `ns` is a namespace import | export `f` of that module | resolved |
   | `this.m()` / `super.m()` in a class | method `m` of that class or of its resolved base | resolved |
   | `extends C` / `implements I` | same as identifiers | resolved |
   | a free identifier (bound nowhere in the file: `useX()`, `extends X`) or an unresolved template tag or word, listed in an `autoImports` file | that file's export, via the export table (`default` → the default export) | resolved |
   | anything else (`obj.m()`, parameters, destructured values) | the bare name | guessed |

   A guessed edge is kept only if exactly one non-module, non-test node has that name. Otherwise it is dropped.
4. **Declaration → node:** a declaration maps to the node of that file whose `startLine` is the anchor line, preferring a node with the same name.
5. **The edge's `from`** is the innermost node containing the call line, falling back to the module.
6. **Imports:** a static import or re-export gives an `imports` edge to the target module. `* as ns` sets `namespace`, and `export … from` sets `reexport`. Dynamic `import('x')` gives `imports` with `namespace`.
7. **JSX:** `<Foo>` and `<Foo.Bar>` give a `renders` edge to the import binding of `Foo`.
8. Edges whose `to` is not a known node are dropped.

**Auto-imports.** Each `autoImports` file is read line by line (no TypeScript parse) for `export { a, b as c } from '<spec>'`, `const a: typeof import('<spec>').a` / `['a']`, and `'Name': typeof import('<spec>')['default']`. A relative `<spec>` resolves from the declaration file's directory, probing `.ts`, `.tsx`, `.vue`, `.js`, `.jsx`, `.mts`, `.mjs` and `/index.*` against the source files. A bare package, `#alias` or path outside the repo gives no entry, so the call stays guessed. Only free identifiers use these entries (a member `obj.useX()`, a parameter or local named `useX` stays guessed); an unresolved template tag (as PascalCase) or word resolves only through them. Nuxt's lazy entries (`LazyX: LazyComponent<typeof import(...)>`) are not parsed yet, so `<LazyX>` stays unresolved. Base-graph worktrees link missing (gitignored) `autoImports` files from the working tree.

**Narrow local inference** (no type checker). A variable, parameter or class field has a type when it is declared with a type reference (`repo: Repo`, constructor parameter properties included) or initialised with `new C()`. Then `x.m()`, `this.f.m()` and the method reference `x.m` (as in `handler: c.get.bind(c)`) resolve to method `m` of that class (a `calls` edge), or, when the type is an interface or type alias, to `m` of each class that implements or extends it (a `dispatch` edge, `resolved` when there is exactly one implementation, `guessed` otherwise). Interfaces and type aliases get an internal id for this but are not nodes. `dispatch` edges stand for runtime flow, not a code dependency, so arch rules and review ignore them. Everything else stays guessed as above.

### Pattern nodes

Pattern nodes use `ast-grep-core` and `ast-grep-language` (TypeScript, Tsx, JavaScript). For embedded files the extracted script is used.

- **Language:** `.jsx`/`.tsx` → Tsx, `.js/.mjs/.cjs` → JavaScript, everything else → TypeScript.
- **Match filter:** `where` restricts metavariables to the allowed texts.
- **ID:** `id` interpolates `{VAR}`. A `where`'d variable is uppercased, and other variables have their quotes stripped. A match with a missing variable is skipped.
- **Order and numbering:** matches are sorted by start line, and IDs are numbered with `~n` like other nodes.
- **Placement:** a pattern node's parent is the smallest node that contains it. Nodes inside it are reparented to it, and edges from its parent on its lines are re-attributed to it. An ID that already exists is skipped.
- **Intents:** these come from the comment lines directly above the match.
- **Invalid patterns:** an invalid pattern adds one `parseErrors` entry (`file: ig.yaml`, `patterns[i]: …`) and is skipped from then on.

### Git and the base

- **The base** is picked from `--base`, then `CI_MERGE_REQUEST_DIFF_BASE_SHA` (with a notice if it doesn't verify), then the merge-base of `HEAD` with `origin/HEAD`, `main` or `master`, then `HEAD` itself (with a notice). Outside a git repo, every node counts as added, with a notice.
- **`--head <ref>`:** if the ref is the current HEAD and the tree is clean, the working tree is used. Otherwise the ref is materialised in a temporary worktree.
- **Worktrees** use `git worktree add --detach` in a temp dir, with `node_modules` symlinked from the root and from each package. They are always removed, even on error (Drop guard), followed by `worktree prune`.
- **Implementation:** shell out to `git` *(call)*. The user's git config and worktree behaviour then apply unchanged.

### Check

1. Load the config. **Trust boundary:** for a materialised `--head`, `checkers` always come from the working-tree `ig.yaml`, never from the head's copy.
2. Build the head graph and the base graph (cached), then diff them. A node is `added`, `removed`, `changed` (different hash) or `unchanged`. An edge, keyed `from|kind|to`, is `added`, `removed` or `unchanged`.
3. **Scoped findings** count only if they touch the change, unless `--all`:
   - **Which findings:** arch, complexity, and checker findings other than checker errors.
   - **What counts as touching the change:** a finding with a node counts if that node is added or changed. One with an edge counts if the from→to pair is a new edge. Any other finding counts if its file has an added or changed node.
4. **Always-on findings** are reported regardless of scope:
   - parse errors (`ig/parse-error`, warning);
   - checker errors (`ig/checker-error`, warning);
   - intent findings;
   - structural regressions, when a base exists.
5. **Post-processing, then output:**
   - apply `severity` overrides;
   - drop baselined fingerprints;
   - attach snippets (±2 lines, with line numbers);
   - `blocking` = any finding at or above `blockOn`;
   - write `.ig/graph.json` and `.ig/findings.json`.

**Rules:**

| Rule | Severity | When |
|---|---|---|
| `ig/arch` | error | An imports, calls or renders edge crosses a forbidden layer pair. |
| `ig/complexity` | error | Not modules or components. **Added** node: complexity > `max`. **Changed** node: complexity > `max` and higher than before, or an increase > `maxIncrease`. |
| `ig/intent-unimplemented` | error | An in-scope intent (added or changed in `intents.md`, or all with `--all`) has no non-test node carrying the tag. |
| `ig/intent-untested` | warning | The same scope, but no test node (or node in a test file) carries the tag. |
| `ig/intent-unknown` | error | A tag is not declared in `intents.md`. |

**Checkers** run as `sh -c <run>` in the repo, with a 120 s timeout and a 64 MB output cap. Results are mapped as follows:

- `uri` → a repo-relative path (`file://` URIs and absolute paths are handled);
- `startLine` → line (default 1);
- `level` → severity (default warning);
- `fixes[0].description.text` → fix;
- the innermost node at that line → nodeId.

The other formats map the same way: `eslint-json` (`severity` 2 error, 1 warning; rule `ruleId`), `stylelint-json` (`severity` error/warning; rule `rule`), `checkstyle` (`severity` error/warning/info; rule `source`) and `lines`. `lines` matches `path:line[:col][: ]message` and tsc's `path(line,col): error TS1234: message`. Its rule is the `TS\d+` code if present, and its severity is an explicit `error`/`warning` word, else warning. The tool name is the rule id when the output has none. A non-zero exit is not an error by itself, but a non-zero exit with zero findings is reported as `ig/checker-error` (`exited <code> with no findings; stderr: …`). Exit 0 with empty output means zero findings.

Bad output becomes an `ig/checker-error` finding that includes the first 300 chars of stderr.

**Output formats:**

- **summary:** intent changes (`+`, `~`, `-`), a node count line, then findings grouped by severity.
- **agent:** the blocking findings with snippet and fix, then the closing line `Fix these, add @intent tags where missing, then run ig check again.` If nothing blocks, it prints `ig check: no blocking findings (N warnings)`.
- **json:** the full result without the base graph.

`diff` prints the changed nodes grouped by file (`+`, `~`, `-`).

### Blueprint

With a `blueprint` section, the graph also links the stack end to end:

- **`http` edges:** a call to a name in `http.calls` (`fetch(url, {method})`, `this.fetch`, `useFetch`) whose first argument is a string, template, concatenation or a variable initialised with one is an HTTP call (method from the `method` property, default `GET`). Its path, minus `http.prefix`, is matched against nodes of kind `http.routes`, named `METHOD /path` (a pattern node). Both sides are normalised: scheme, host, query and fragment dropped, `${x}`, `{param}` and `:param` segments become wildcards. `${CONST}` in a route path is replaced by a same-file string constant, and route display names show the result. An exact match is `resolved`; a route parameter matching a literal, or a call with extra leading segments, is `guessed`.
- **System nodes:** a bare import of a package listed under `systems` adds a node `system:<Name>` (kind `system`, no file) and a `uses` edge from the importing module.

`ig view` gets a **Blueprint** mode. The journeys are the modules of the first lane (typically pages). Picking one shows, in swimlanes, the units on paths from that page into the backend: modules, with endpoints and systems as their own boxes. The walk follows every edge kind but `imports`, only goes down the lanes (or within one), never enters other pages, and by default keeps only units that lead to something reached through an `http` edge or a system. Test files are left out.

### Brief

A change is read as architecture plus real code, scoped to functions and ordered by architecture, rather than as a file-by-file diff or a long spec.

- **Function diffs:** for a changed node, its old text (from `git show base:file`, at the base node's range) and new text (from the tree or the head commit) are diffed line by line (LCS; deletions before additions). Hunks keep 3 lines of context. Added and removed nodes show their full text (80 lines max in the brief).
- **Sequences** start from a node and follow `calls`, `dispatch`, `http` and `uses` edges in source-line order to depth 4 (60 messages max), skipping guessed edges and construction of `*Error`/`*Exception` classes. Participants are classes (for members of a class), modules, or systems. They render as Mermaid (`sequenceDiagram`) and export as PlantUML.
- **`ig brief [--base] [--head] [--md] [--out <file>]`** runs a check and prints: added/changed/removed functions grouped by blueprint lane (or by layer); new dependencies on systems; new links between lanes or layers; the code of each changed function (its diff) and each new one, in lane order, skipping a node whose changed parts are shown on their own (a class whose methods changed); sequences from the entry points of the change (changed nodes no other changed node calls; at most 5); blocking findings. `--md` (implied by `--out`) writes Markdown with `diff` and `mermaid` blocks, which GitLab and GitHub render.
- **Viewer:** the side panel shows a node's code (diff with line numbers when it changed; "Open wide" shows it full-window) and its sequence (Mermaid, offline, in a full-window overlay, with "Copy Mermaid" and "Copy PlantUML"). A tree of folders, files and functions sits on the left, synced with the graph both ways, with a filter and an "only what is on screen" switch.

### Review

`ig review [--all] [--json] [--agent]` builds a module graph and reports structural problems, ranked.

**The module graph** has these properties:

- **Modules** are module or component nodes outside test files.
- **Links** are resolved edges of every kind, lifted to file → file, ignoring self-links.
- **Two link sets:** `out`/`in` includes imports, while `useOut`/`useIn` covers usage edges only.
- **`dirOf`:** the first path segment under the source root. The source root is the package dir, or its `src/` when the package has modules there.

**Rules:**

| Rule | Weight | Finding |
|---|---|---|
| `struct/cycle` | 3 | Strongly connected components of size ≥ 2 (iterative Tarjan). Evidence = the shortest loop through the first module. With `layers`, layer-level cycles are also reported, with back edges (see below). |
| `struct/hub` | 3 | Use fan-in and fan-out both ≥ the thresholds. A threshold is the `hubPercentile` value over active modules, at least `hubMin`. |
| `struct/unstable-dependency` | 2 | Dir A, with fan-in ≥ `stableFanIn`, depends on dir B whose instability is ≥ A's + `instabilityGap`. Instability = out / (in + out) over dir-level module edges. B being a source root is skipped. |
| `struct/low-cohesion` | 2 | ≥ `cohesionGroups` groups and ≥ `cohesionMinMembers` members (see Cohesion below). |
| `struct/change-coupling` | 2 | Pairs from `git log --relative --name-only -n historyCommits` that changed together in ≥ `couplingMin` commits, at a ratio of ≥ `couplingRatio` of the rarer one. They must be in different dirs, with no dependency either way. Commits with > 50 files are ignored. |
| `struct/unused-export` | 1 | Exported members of a module that no non-test code uses through calls, renders, extends or implements. A module imported as `* as ns` counts as fully used. Package entries, everything they re-export (transitively), and `entryPoints` are skipped. Evidence lists the symbols used only by tests. |
| `struct/oversized` | 1 | Module loc ≥ `oversizedLoc`, or a member with complexity ≥ `oversizedComplexity`. |

**Cohesion groups:** a module's top-level members are joined by union-find. Members are linked when they call each other directly (a nested node counts as its top-level ancestor) or share an outside, non-test caller. Edges from the module node itself are ignored. A "job" is a group of 2 or more.

**Layer cycles:** the layer graph is built from module edges, and its SCCs are found.

- **Back edges:** for each layer pair linked both ways, the direction with fewer edges holds the back edges. On a tie, the direction whose source layer is declared later holds them. A ring with no two-way pair breaks at its weakest link.
- **Evidence:** directions sorted cheapest first. The first 3 directions are shown, with 3 example edges each.
- **Modules:** the modules on the shown edges.

**Review fingerprint:** sha1 of `ruleId|sorted modules joined by ","`.

**Ranking:**

- score = affected × weight;
- ties are broken by fan-in of the first module, then by fingerprint;
- at most 3 findings per rule in the top `top`; the rest follow in score order.

The default output shows only that top block, plus `N more — ig review --all`. Each item has a title, evidence line, why and fix, written in plain words.

**Structural regressions** run during `check`, as warnings that are always on, against the base:

| Rule | When |
|---|---|
| `struct/new-cycle` | A head SCC that isn't a subset of any base SCC. |
| `struct/new-hub` | A hub under the base's thresholds that wasn't a hub before. |
| `struct/cohesion-drop` | Jobs increased by ≥ 2. For a new module: jobs ≥ `cohesionGroups` and size ≥ `cohesionMinMembers`. |

Their fingerprint = the rule plus the sorted module set. The file is the first sorted module, at line 1.

**Review defaults:** `hubPercentile 0.95`, `hubMin 5`, `instabilityGap 0.3`, `stableFanIn 5`, `cohesionGroups 3`, `cohesionMinMembers 8`, `couplingRatio 0.6`, `couplingMin 5`, `historyCommits 500`, `oversizedLoc 500`, `oversizedComplexity 25`, `top 10`.

### View

**`ig view`** writes a self-contained `.ig/view.html` and opens it with `open`, `xdg-open` or `start` (skipped with `--no-open`). The page embeds:

- **Data JSON:** nodes with their status and their before/after source slices (before from `git show base:file`; after from the tree or the head commit), edges with status and an arch-violation flag, findings pinned to a node, intents with coverage, changed intents, notices and the editor config. `<` is escaped as `<`.
- **The default mode:** "changed" (the changed nodes plus their direct neighbours). It is "architecture" when that set is empty or larger than 300 nodes.
- **Modes:**
  - *Changed*: the changed nodes and their neighbours, boxed by module.
  - *Architecture*: a flat, scoped drill-down. The scope's children (layers, then directories, then modules; single-child directory chains are merged and single-module directories show as the module) are the main nodes, and the siblings at every level above that connect to the scope are dimmed context. Edges are aggregated with counts, and arch violations are red. A breadcrumb leads back up.
  - *Focus*: the depth-1 or depth-2 neighbourhood of one node, reached by clicking a node, a finding or the search box.
- **The page:** `viewer/` is a TypeScript project (strict, Lit web components, Vite, Vitest, bun as package manager and script runner; node 20+ works too) with Cytoscape, cytoscape-fcose and Mermaid as pinned npm packages. `viewer/src/model/` holds the pure, DOM-free logic (indexing, architecture, blueprint, focus, changed, sequence, tree) and `viewer/src/components/` the Lit components (`ig-app`, `ig-tree`, `ig-graph`, `ig-side-panel`, `ig-plan-panel`, `ig-overlay`). `bun run build` makes `viewer/dist/index.html`, one offline file with everything inlined and two markers, `/*IG_DATA*/` and `/*IG_LIVE*/`, which `ig view` fills. `build.rs` copies it into the binary (`include_str!` from `OUT_DIR`); when it is missing, a placeholder page that shows the data as JSON takes its place, with a cargo warning, so Rust-only builds need no bun. `make build` runs both steps.
- **State and focus:** the data has `state` (`applying` when valid apply state exists, else `planned` or `done` from the current plan's `Status`, else `plain`) and, when `.ig/focus.json` exists, `focus` (node ids). The header shows `Planning`, `Applying` or `Done` next to the plan title, and a non-empty `focus` starts the page in Focus mode on those ids. `ig view` and every `--watch` rebuild delete `focus.json` after rendering, so a focus is shown once (an `ig focus` issued while watching shows on the next rebuild). The plan shown is `--plan`, else the plan being applied, else `.ig/current-plan`; `--no-plan` shows none.
- **`ig view --json`** prints the data instead of writing the page (for `bun run dev`, which loads `viewer/dev-data.json`).

**`ig view --watch`** serves `/` and `/events` (server-sent events) on `127.0.0.1` at a random port, and rebuilds on file changes:

- changes are debounced by 300 ms, and `.ig`, `.git` and `node_modules` are ignored;
- the plan and apply state are re-read on every rebuild;
- rebuilds are serialised, and the last good page is kept on error;
- `--head` is rejected.

*(call)* Uses `tiny_http` and `notify`.

### Report (GitLab)

`ig report --gitlab` checks `HEAD`, writes `view.html`, and creates or updates one MR note. The note is found by the marker `<!-- ig-report -->` across pages (via `x-next-page`), and is sent with the `PRIVATE-TOKEN` from `IG_GITLAB_TOKEN` or `GITLAB_TOKEN`.

The note contains, in order:

1. a blocking status line and the notices;
2. an intent-diff block;
3. a Mermaid flowchart of the changed nodes and their neighbours, collapsed to modules above 40 nodes, with added/changed/removed classes and red violating links;
4. a findings table (max 50 rows);
5. an intent-coverage table;
6. an artifact link and a reproduce command.

*(call)* Uses `ureq`.

### MCP

`ig mcp` serves JSON-RPC 2.0 over stdio with these tools:

| Tool | Does |
|---|---|
| `check {all?}` | Agent text plus structured `{blocking, findings}`. |
| `get_findings` | The last `findings.json`, or runs a check. |
| `get_intents` | Intents with implemented/tested coverage. |
| `graph_query {id, direction?: in\|out\|both, depth?}` | The neighbourhood of a node, with depth clamped to 1–3. An unknown id returns up to 5 substring suggestions, shortest first. |

*(call)* Hand-rolled. Move to `rmcp` if the protocol surface grows.

### Stop hook

`ig hook claude` reads the hook JSON on stdin and never fails. Errors become `{"systemMessage": "ig hook error: …"}`.

- **Not blocking:** clears the session counter and prints `{}`.
- **Blocking, under the cap:** increments `.ig/hook/<session>` and prints `{"decision":"block","reason":<agent text>}`.
- **At `hookMaxRounds`:** prints a `systemMessage` handing off to a human.

### Init

`ig init [claude|codex|all]` never overwrites anything. The argument picks the coding agents to set up (default: all). It creates:

- **`ig.yaml`:** with proposed layers (one per workspace package, or one per `src/*` dir), empty rules, the complexity defaults, detected `embedded` extensions (unknown extensions that relative imports point at), and commented examples;
- **`intents.md`;**
- **`.claude/settings.json`** (Claude only): with the Stop hook (`ig hook claude`, timeout 300). If the file already exists, the snippet is printed instead;
- **the `ig-setup` skill** per agent, in the open SKILL.md format: `.claude/skills/ig/skills/setup/SKILL.md` (plugin `.claude/skills/ig/.claude-plugin/plugin.json`) for Claude Code (`/ig:setup`), `.agents/skills/ig-setup/SKILL.md` for Codex (`$ig-setup`). The text is embedded in the binary (`assets/skills/setup.md`);
- **the `AGENTS.md` snippet:** appended once, guarded by the `<!-- ig:agents -->` marker;
- **the `.gitignore` entries:** an existing `.ig/` or `.ig` line is rewritten;
- **the initial baseline:** from `check --all`, excluding checker and parse errors.

### Setup

`ig.yaml` is project knowledge that `ig` cannot infer on its own, so a coding agent writes it, following the `ig-setup` skill and grounded in facts from `ig setup`.

- **`ig setup [--json]`** builds the graph with the current config (or defaults if it is missing or invalid) and prints: config status; workspace packages; module counts per directory (depth 3); directories named `pages`, `views`, `routes` or `screens` (journey candidates); modules without a layer; external packages by importing module count, with the system each is mapped to; callee names whose first argument looks like a URL (`http.calls` candidates); route count, linked and unmatched HTTP calls (with samples); node and edge counts; parse errors; arch violations per rule over the whole repo. Test files are left out of the package and callee counts.
- **`ig setup claude|codex`** installs the skill for that agent if it is missing, then runs the agent's CLI interactively in the root with the skill invoked (`claude /ig:setup`, `codex '$ig-setup'`). The agent edits `ig.yaml` only, validates with `ig check --all` and `ig setup`, and reports what it changed and why.

`ig` ships as a project-local Claude Code plugin (`.claude/skills/ig`), so skills are namespaced `/ig:setup`; Codex skills are `ig-<name>` (`$ig-setup`).

### CLI

```
ig [--version] | init [claude|codex|all] | setup [--json] | setup claude|codex
   | check [--base <ref>] [--head <ref>] [--all] [--json] [--agent]
   | brief [--base <ref>] [--head <ref>] [--md] [--out <file>]
   | review [--all] [--json] [--agent] | diff [--base <ref>] [--head <ref>] [--json]
   | view [--base <ref>] [--head <ref>] [--watch] [--no-open] | report --gitlab | mcp | hook claude
   (all accept --root <dir>)
```

Exit codes: 0 ok, 1 blocking findings, 2 usage or error (`ig: <message>` on stderr). Notices go to stderr. *(call)* `lexopt`.

## Speed design

- **Two-pass build, both passes parallel per file** (`rayon`) *(call)*:
  - pass 1: parse, extract nodes, build the export tables;
  - pass 2: edges, against the read-only global index.
- Each file is parsed once, even when several packages import it.
- The base graph is cached per commit and config key.
- No incremental head cache until benchmarks show it's needed.

## Crates *(all calls)*

| Purpose | Crates |
|---|---|
| Viewer (npm packages in `viewer/package.json`, pinned) | Cytoscape, cytoscape-fcose, Mermaid, Lit; Vite, Vitest, TypeScript to build and test |
| Parsing and resolution | `oxc_parser`, `oxc_ast`, `oxc_semantic`, `oxc_span`, `oxc_allocator`, `oxc_resolver` (pin these versions together) |
| Patterns | `ast-grep-core`, `ast-grep-language` |
| Serialisation | `serde`, `serde_json`, a maintained YAML crate (`serde_yaml_ng`), `json_comments` |
| Matching and hashing | `globset`, `regex`, `sha1` |
| Runtime | `rayon` |
| Networking and serving | `ureq`, `tiny_http`, `notify` |
| CLI and errors | `lexopt`, `anyhow` |

## Verification

1. **Fixture repos under `tests/fixtures/`**, with golden `graph.json` and findings. They cover:
   - a monorepo with workspace imports;
   - re-export chains and namespace imports;
   - an SFC with `<script lang="ts">`;
   - tests with nested `describe` and duplicate titles;
   - patterns;
   - a layer violation;
   - a cycle.
2. **Unit tests** for node IDs and hashes, fingerprints, base resolution, check scoping, every review rule, the Mermaid output and GitLab note, the hook rounds, and init's no-overwrite behaviour.
3. **A benchmark** on one large real monorepo, measuring `ig check` cold (no cache) and warm. The target is set with the user.
4. **Dogfood:** `ig check --agent` on this repo stays clean.

## Left to the builder

- The internal module layout and error types.
- Exact oxc parse-error messages in `parseErrors` (first error per file).
- Whether to pretty-print `graph.json`.
- How the binary is distributed.

## Open questions for the user

- Which large repo is the speed benchmark, and what time is "fast enough" for the Stop hook?
- How much call-edge recall loss from having no type checker is acceptable before it needs narrow inference?
