# ig architecture refactor: a hexagonal Cargo workspace

Status: design, not implemented. Date: 2026-10-25.

## Intent

ig grew feature by feature into one flat crate:
- 25 modules, several of them 1,000–1,500 lines (`extract.rs`, `review.rs`, `graph.rs`);
- **no traits at all**;
- filesystem, git and process I/O spread through the logic (`apply.rs` touches them 36 times, `view.rs` 27).

The user wants ig built to the practices it promotes: SOLID, and a clean or onion style architecture, idiomatic in Rust. Two upcoming needs make this the right moment:

- **More languages.** The user's priorities are TypeScript (now), then **Python, Go and Rust**. Parsing and resolution are TS/JS-only and woven into the graph builder.
- **More front ends.** CLI, MCP, `ig serve`, and soon a Tauri desktop app that needs ig as a library.

## Goals

- A Cargo workspace whose crates follow **ports and adapters** (hexagonal):
  - a pure domain core;
  - use cases that orchestrate ports;
  - adapters for languages, git, the filesystem and external tools;
  - thin driving adapters (CLI, MCP, serve, desktop).
- **No behaviour change.** Every existing command produces the same output. The existing unit tests, the golden tests and the viewer tests pass unchanged. Golden files are the proof.
- **TypeScript becomes one language adapter** behind a `LanguageFrontend` port. Python, Go and Rust can be added later as further adapters, without touching the core.
- The core is testable without temporary repos: use cases run against in-memory fakes of the ports.

## Non-goals

- New features, new languages or performance work in this refactor. Python, Go and Rust come in later projects. The port must be shaped so they fit (see below), but no adapter for them is written here.
- An interface for everything. Traits exist only at real boundaries.

## Anti-goals

- A big-bang rewrite that leaves the tool broken for days. Every step compiles, passes all tests, and is committed.
- Abstraction ceremony: one-implementation traits inside the core, factories, dependency-injection frameworks.

## Shape

| Crate | Depends on | Holds |
|---|---|---|
| `ig-core` | serde only | **Domain:** graph model (`Node`, `Edge`, `Graph`), diff, layers and arch rules, complexity, review metrics, plans and notes parsing and merge, call chain (Rust side), findings and fingerprints, config model and validation. **Ports** (traits): `LanguageFrontend`, `Vcs`, `Files`, `CheckerRunner`. No I/O. |
| `ig-app` | `ig-core` | **Use cases:** `build_graph`, `check`, `review`, `plan_check`, `apply_start/finish/abort/except`, `brief`, `view_data`, serve state. They take port implementations as arguments, generic or `&dyn`. Errors are typed (`thiserror`), not stringly. |
| `ig-lang-ts` | `ig-core`, oxc, oxc_resolver, ast-grep | The TS/JS frontend: extraction, tsconfig flattening, resolution, embedded `.vue`, autoImports, patterns. |
| `ig-git` | `ig-core` | `Vcs` adapter: git CLI, worktrees, base-graph cache. |
| `ig-fs` | `ig-core` | `Files` adapter: workspace discovery, globs, atomic writes. |
| `ig-checkers` | `ig-core` | `CheckerRunner` adapter: process spawning, output formats. |
| `ig` (bin) | all | CLI argument parsing and rendering only. Also hosts the MCP and serve driving adapters as modules (or crates `ig-mcp` and `ig-serve` if they stay large). |
| `ig-desktop` (later) | `ig-app` + adapters | The Tauri app from the Design workspace plan. |

Dependency rule: arrows only point inward. `ig-core` depends on nothing from ig. Adapters depend on `ig-core`, never on each other or on `ig-app`. Only the binaries wire everything together.

## The `LanguageFrontend` port (shaped for Python, Go and Rust)

- **Per file, parallelisable:** `extract(file, source) -> FileFacts` (nodes, exports, raw references, HTTP calls, packages).
- **Resolution with whole-workspace context:** `resolve(facts: &[FileFacts], ws) -> Vec<Edge>`.
- **Capabilities** declare what the frontend can resolve (`types`, `http`, `embedded`…), so rules and the viewer degrade gracefully.
- **Language-neutral facts:** module, function, class, method, route and system kinds; call, import, render, extend, implement, http and uses references. Language-specific concepts stay inside adapters.
- **Two future routes for new languages, both fitting this port:**
  - a **tree-sitter** adapter, driven by per-language queries: broad and quick;
  - a **SCIP** adapter that reads indexes from official indexers (`scip-python`, `scip-go`, `rust-analyzer`'s SCIP output): compiler-accurate.
  
  Which to use per language is decided in the later language projects.
- Several frontends can coexist in one repo (a TS front with a Go API), chosen per file by extension and config.

## SOLID, concretely

- **Single responsibility:** split the files over about 600 lines by responsibility. For example, `extract.rs` becomes visitor, callee resolution and HTTP detection; `review.rs` becomes a metrics module per family.
- **Open/closed:** a new language, VCS or checker format is a new adapter; the core doesn't change.
- **Liskov:** every port has a fake used in tests, and the fake and the real adapter pass the same contract tests.
- **Interface segregation:** small ports. `Files` doesn't carry git; `Vcs` doesn't carry checkers.
- **Dependency inversion:** use cases depend on port traits, never on `std::fs`, `Command` or oxc directly.

## Constraints

- Rust 2024. New crates are limited to `thiserror`, plus any workspace plumbing that's needed.
- Same CLI, same flags, same output formats, same `.ig/` files, same golden outputs.
- `make build`, `make test` and `make install` keep working. The viewer (`viewer/`) is untouched, apart from the path `build.rs` embeds from.
- The migration happens in small, green, committed steps, on a branch, with a whole-branch review at the end.

## Left to the builder

- The exact module split inside each crate, generic versus `dyn` per port, and the error type layout.
- Whether MCP and serve become their own crates.
- The order of extraction. Suggested: `ig-core` model and diff first, then the ports with fakes, then the adapters one by one, then the use cases, then the binary.
