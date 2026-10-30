# `ig` (intent-graph) in Rust: design

Date: 2026-10-24. Status: implemented (v0.1.0).

## Intent

`ig` turns a TypeScript/JavaScript monorepo into a code graph and draws it as a system map you can walk (see [System map](#system-map)). The graph carries:

- architecture layers and the `ig.yaml` rules between them;
- functions, calls, imports and your own pattern nodes;
- one-line intents linked to code with `@intent` tags.

The map is shown by `ig view` and answers to a coding agent over MCP.

`ig` has no built-in knowledge of any framework. Routes and single-file components come from the project's `ig.yaml`.

**Speed is the primary design goal.** `ig view` builds the graph on every run, so its wall-clock time sets the pace. The target is a warm run on a large monorepo in under 2 seconds.

## Decided

These were decided with the user:

- **Language: Rust**, chosen for speed.
- **Parser: oxc** (`oxc_parser`, `oxc_ast`, `oxc_semantic`, `oxc_resolver`).
- **No type checker.** Call edges are resolved from scopes, imports and re-export chains. Typed-variable method calls are knowingly imprecise in exchange for speed (see Call resolution).

Everything else is **the author's call**, marked *(call)*. The builder can revisit those.

## Goals

- Commands: `init`, `view`, `mcp`, `plugin export`, `--version`. All of them accept `--root <dir>`.
- A single static binary with no runtime dependencies besides `git`.
- Deterministic output: the same source always gives the same node IDs, hashes, edges, fingerprints and sort order.

## Non-goals

- Type checking, or shelling out to `tsc`/`tsgo`.
- Languages beyond TS/JS and configured embedded (single-file-component) files.
- Built-in framework or linter knowledge.

## Anti-goals (a failure even if it works)

- **Unstable fingerprints or node IDs.** Unchanged code would show as changed, and the committed map would churn.
- **`resolved` edges that are really guesses.** They produce false rule marks on the map. When in doubt, emit `guessed` or nothing.
- **Slow on large repos.** The map is rebuilt on every run.

## Data model

All JSON uses camelCase keys, and fields that are absent are omitted.

- **Node:** `id`, `kind` (`module` | `component` | `class` | `function` | `method` | `test` | `system` | a pattern kind such as `route`), `name`, `file` (repo-relative posix path), `range {startLine, endLine}` (1-based, inclusive), `parent?`, `package?`, `exported?`, `hash`, `metrics {loc, complexity}`, `intents[]`.
- **Edge:** `from`, `to`, `kind` (`imports` | `calls` | `renders` | `extends` | `implements` | `dispatch` | `http` | `uses`), `confidence` (`resolved` | `guessed`), `file`, `line`, `namespace?`, `reexport?`. Edges are deduplicated on from/kind/to, and `namespace`/`reexport` are OR-merged across duplicates.
- **Graph:** `schemaVersion: 1`, `toolVersion`, `commit` (sha or null), `nodes` (sorted by id), `edges` (sorted by from, kind, to), `parseErrors [{file, message}]`.
On-disk files:

| File | Content |
|---|---|
| `ig.yaml` | Config (below). Committed. |
| `ig/*.kdl` | The spec: the intended architecture (boxes, their `at` globs, arrows). Committed; written once by ig, then edited by people, never rewritten. |
| `.ig/layers/*.json` | Marks written by other tools: `{layer, marks: [{target, state, badge?, value?, note?}]}`. |
| `.ig/graph.json` | The last head graph. |
| `.ig/cache/<sha>-<key>.json` | Cached graphs. `key` = the first 12 hex chars of a hash over toolVersion + `embedded`, `autoImports`, `ignore`, `tests`, `testCalls` and `patterns`. Written to a temp file, then renamed. |
| `.ig/view.html` | The self-contained viewer page. |

`.gitignore` gets `.ig/*`; `ig/` sits outside `.ig/` and is never ignored.

## `ig.yaml`

| Key | Default | Meaning |
|---|---|---|
| `layers` | `{}` | `name: [entries]`. An entry is a glob, or a workspace package name, which expands to `<pkg dir>/**`. The most specific entry wins (the longest literal prefix before the first `*?[{`); a tie goes to the first declared. |
| `rules` | `[]` | `- forbid: "a -> b"`. Layer `a` must not import, call or render layer `b`. |
| `embedded` | `[]` | Extensions of single-file components (for example `.vue`, `.svelte`). |
| `autoImports` | `[]` | Paths or globs (relative to the root) of generated declaration files listing a framework's auto-imports: Nuxt's `.nuxt/imports.d.ts` and `.nuxt/components.d.ts`, unplugin-auto-import's `auto-imports.d.ts`, unplugin-vue-components' `components.d.ts`. See Call resolution. A missing file is a parse-error notice. |
| `ignore` | `[]` | Globs that are never analysed (dotfiles match). |
| `tests` | `["**/*.test.*", "**/*.spec.*", "**/__tests__/**"]` | |
| `testCalls` | `["describe", "it", "test"]` | |
| `patterns` | `[]` | `{kind, match, where?: {VAR: [allowed]}, id}`. These are ast-grep patterns; `id` interpolates `{VAR}`. |
| `blueprint` | none | `http: {calls, routes: route, prefix?}`, `systems: {Name: [packages]}`: HTTP links and outside systems for the map's facts. `lanes` is no longer read. |
| `agent` | none | `claude`, `gemini` or `codex`: the CLI for the AI reading. |

Removed keys (`complexity`, `checkers`, `severity`, `blockOn`, `editor`, `hookMaxRounds`, `entryPoints`, `review`) are ignored with a notice. Validation errors are fatal and look like `ig.yaml: rules[0]: expected "a -> b"`. List keys must be lists.

## Architecture

The repo is a Cargo workspace in a hexagonal layout: a pure core, use cases over ports, and adapters that implement the ports.

| Crate | Role |
|---|---|
| `ig-core` | Pure domain: graph model, config, rules (arch, intents), cache keys, and the ports (`Files`, `Vcs`, `LanguageFrontend`). No I/O. |
| `ig-app` | Use cases over the ports: the language-neutral graph build and the system map (facts, reading, layers, layout, view). No I/O. |
| `ig-lang-ts` | `LanguageFrontend` for TypeScript/JavaScript: oxc extraction, resolution, tsconfig, embedded SFC scripts, ast-grep patterns, auto-imports. |
| `ig-fs` | `Files` over `std::fs` (source discovery, `.ig/` files). |
| `ig-git` | `Vcs` over the `git` binary. |
| `ig-wire` | Wires the adapters into `ig-app`'s ports. |
| `ig` (package `intent-graph`) | The binary: CLI parsing (`cli.rs`), the agent runner, `mcp`, `init`, skills. `build.rs` embeds the viewer. |
| `ig-desktop` | A Tauri 2 app around the viewer, with a chat panel. Built locally with `make app`, not distributed. |

**Dependency rule:** `ig-core` depends on no `ig-*` crate; `ig-app` and every adapter depend only on `ig-core`; only the wiring and the binaries depend on everything. `crates/ig/tests/architecture.rs` enforces this with `cargo metadata`, greps `ig-core` and `ig-app` for I/O (`std::fs`, `std::process`, `std::net`, `Command::new`, `File::open`, `println!`, `eprintln!`) and fails on any `.rs` file over 600 lines.

**Adding a language frontend:** the port (`LanguageFrontend`: `handles`, extraction, resolution, `prepare_checkout`, `cache_inputs`) is the seam, and a new `crates/ig-lang-<x>` crate depends only on `ig-core`. It is not yet drop-in: source-file detection (`ig_core::workspace::is_source_ext`) and package discovery (`ig-fs` discover: npm/pnpm workspaces, package.json, tsconfig) are still TS/npm-shaped, `prepare_checkout`'s `link_auto_imports` flag is TS-specific, and the node hash/intent helpers live in `ig-lang-ts`. Generalising those is the first step of a language project; only then does registering the frontend in `main` suffice.

Data flow, in pipeline order (the stages now live in the crates above):

```
cli         CLI parsing, dispatch, exit codes (ig)
config      ig.yaml → Config (ig-core, read by ig-app)
files       source file discovery, package discovery (ig-fs)
embedded    script extraction and markup scanning for SFCs (ig-lang-ts)
extract     per-file nodes and edges (oxc AST, ig-lang-ts)
resolve     module resolution, export tables, symbol → node (ig-lang-ts)
build       two-pass build, pattern nodes, sort/dedup (ig-app, with ig-lang-ts)
map         facts → boxes and arrows, reading, layers, layout (ig-app)
render      the viewer page (ig); the viewer itself is in viewer/
mcp, init   JSON-RPC over stdio, scaffolding (ig)
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
8. Edges whose `to` is not a known node are dropped. Non-test code never resolves into a test file (stubs, mocks): such edges are dropped, except `imports`, which stay so arch rules can flag code importing a test file.

**Auto-imports.** Each `autoImports` file is read line by line (no TypeScript parse) for `export { a, b as c } from '<spec>'`, `const a: typeof import('<spec>').a` / `['a']`, and `'Name': typeof import('<spec>')['default']`. A relative `<spec>` resolves from the declaration file's directory, probing `.ts`, `.tsx`, `.vue`, `.js`, `.jsx`, `.mts`, `.mjs` and `/index.*` against the source files. A bare package, `#alias` or path outside the repo (`../node_modules/…`) marks the name external: a call to it resolves to nothing and is never guessed onto a same-named repo file. Only free identifiers use these entries (a member `obj.useX()`, a parameter or local named `useX` stays guessed); an unresolved template tag (as PascalCase) or word resolves only through them. Nuxt's lazy entries (`LazyX: LazyComponent<typeof import(...)>`) are not parsed yet, so `<LazyX>` stays unresolved. A stale declaration file (written before a composable was added) misses names: a free call to a name it doesn't list, neither external nor declared, resolves to the unique exported node of that name in a directory the file already covers, and one notice per declaration file lists those names ("run nuxt prepare"; test callers are not counted). Base-graph worktrees link missing (gitignored) `autoImports` files from the working tree, and their contents are part of the base-graph cache key.

**Narrow local inference** (no type checker). A variable, parameter or class field has a type when it is declared with a type reference (`repo: Repo`, constructor parameter properties included) or initialised with `new C()`. Then `x.m()`, `this.f.m()` and the method reference `x.m` (as in `handler: c.get.bind(c)`) resolve to method `m` of that class (a `calls` edge), or, when the type is an interface, a type alias or a class lacking `m`, to `m` of each class that implements or extends it (a `dispatch` edge, `resolved` when there is exactly one implementation, `guessed` otherwise). A class with no such method anywhere (`this.db.knex`, a property) gives a `calls` edge to the class itself. A type from `node_modules` gives no edge. Interfaces and type aliases get an internal id for this but are not nodes. `dispatch` edges stand for runtime flow, not a code dependency, so the rules ignore them. Everything else stays guessed as above.

### Pattern nodes

Pattern nodes use `ast-grep-core` and `ast-grep-language` (TypeScript, Tsx, JavaScript). For embedded files the extracted script is used.

- **Language:** `.jsx`/`.tsx` → Tsx, `.js/.mjs/.cjs` → JavaScript, everything else → TypeScript.
- **Match filter:** `where` restricts metavariables to the allowed texts.
- **ID:** `id` interpolates `{VAR}`. A `where`'d variable is uppercased, and other variables have their quotes stripped. A match with a missing variable is skipped.
- **Order and numbering:** matches are sorted by start line, and IDs are numbered with `~n` like other nodes.
- **Placement:** a pattern node's parent is the smallest node that contains it. Nodes inside it are reparented to it, and edges from its parent on its lines are re-attributed to it. An ID that already exists is skipped.
- **Intents:** these come from the comment lines directly above the match.
- **Invalid patterns:** an invalid pattern adds one `parseErrors` entry (`file: ig.yaml`, `patterns[i]: …`) and is skipped from then on.

### Git

`ig` shells out to `git` *(call)*, so the user's git config applies unchanged. Outside a git repo the commit is null. The graph cache is keyed by commit and config.

### MCP

`ig mcp` serves JSON-RPC 2.0 over stdio with two tools: `ig_state` (what the viewer is showing) and `ig_show` (show a path or subgraph, with steps, in the viewer; every id is checked against the graph; writes only `.ig/show.json`).

*(call)* Hand-rolled. Move to `rmcp` if the protocol surface grows.

### Init

`ig init [claude|codex|all]` never overwrites anything. It creates `ig.yaml`, the `explore` and `view` skills for the chosen agents (`/ig:explore`, `$ig-explore`; the texts are embedded from `crates/ig/assets/skills/`), an `AGENTS.md` snippet (appended once, guarded by the `<!-- ig:agents -->` marker) and the `.gitignore` entry (`.ig/*`; an existing `.ig/` or `.ig` line is rewritten).

### CLI

```
ig [--version] | init [claude|codex|all]
   | view [--no-open] [--json] [--read | --no-read] [--agent claude|gemini|codex|ollama] | mcp | plugin export <dir>
   (all accept --root <dir>)
```

Exit codes: 0 ok, 2 usage or error (`ig: <message>` on stderr). Notices go to stderr. *(call)* `lexopt`.

## System map

`ig view` draws the graph as a walkable canvas: boxes (system, containers, components, then code) and arrows (imports and calls bundled), built in three stages (facts, an optional AI reading, layers), compared with the committed spec in `ig/`. The spec is [docs/superpowers/specs/2026-10-27-ig-system-map-design.md](../superpowers/specs/2026-10-27-ig-system-map-design.md); the README describes the commands, the consent rule for the agent, the `.ig/layers/*.json` shape and the canvas controls. Checking changes (`check`, plans, apply, review, the Stop hook, GitLab notes, design boards, the live server) was removed in this iteration; git history has it.

## Speed design

- **Two-pass build, both passes parallel per file** (`rayon`) *(call)*:
  - pass 1: parse, extract nodes, build the export tables;
  - pass 2: edges, against the read-only global index.
- Each file is parsed once, even when several packages import it.
- The graph is cached per commit and config key.
- No incremental head cache until benchmarks show it's needed.

## Crates *(all calls)*

| Purpose | Crates |
|---|---|
| Viewer (npm packages in `viewer/package.json`, pinned) | Svelte, Vite, Vitest, TypeScript to build and test |
| Parsing and resolution | `oxc_parser`, `oxc_ast`, `oxc_semantic`, `oxc_span`, `oxc_allocator`, `oxc_resolver` (pin these versions together) |
| Patterns | `ast-grep-core`, `ast-grep-language` |
| Serialisation | `serde`, `serde_json`, a maintained YAML crate (`serde_yaml_ng`), `json_comments` |
| Matching and hashing | `globset`, `regex`, `sha1` |
| Runtime | `rayon` |
| CLI and errors | `lexopt`, `anyhow` |

## Verification

1. **Fixture repos under `crates/ig/tests/fixtures/`**, with golden `graph.json`. They cover:
   - a monorepo with workspace imports;
   - re-export chains and namespace imports;
   - an SFC with `<script lang="ts">`;
   - tests with nested `describe` and duplicate titles;
   - patterns;
   - a layer violation;
   - a cycle.
2. **Unit tests** for node IDs and hashes, the map's compiler and layout, the layers files, the agent command lines, init's no-overwrite behaviour, and the viewer's model (Vitest).
3. **Real repos:** `ig view --no-read` warm on a 3,760-node monorepo in under 2 seconds; see the walkthrough for the browser checks.

## Left to the builder

- The internal module layout and error types.
- Exact oxc parse-error messages in `parseErrors` (first error per file).
- Whether to pretty-print `graph.json`.
- How the binary is distributed.
