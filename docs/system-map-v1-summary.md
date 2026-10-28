# ig — the minimal version (system map, v1)

**What ig is now.** A system design tool, not an editor. Run `ig view` on a TypeScript or JavaScript
repo and you get a walkable map of the system in your browser: from the whole system down to the
code, shaped like its real architecture, with problems marked in red and amber.

Design: `docs/specs/2026-10-27-ig-system-map-design.md` · Plan:
`docs/plans/2026-10-27-system-map.md` · Try it:
`docs/plans/2026-10-27-system-map-walkthrough.md`.

## How a map is made: three stages

1. **Static analyser (ig, exact, fast).**
   - **Containers** come from packages.
   - **Outside services** come from the packages the code imports (Postgres, Redis, Stripe, Auth0,
     Sentry, about 25 known ones), or from `ig.yaml` declarations, which win.
   - **Components** come from folders. ig guesses the pattern from folder names: onion, layered,
     hexagonal or none. Loose files get a "root" component.
   - **Arrows** are lifted to the right level, each with a kind: depends, calls, network, data,
     implements, or bundle.
   - **Calls inside loops** are flagged.
   - **Build output and gitignored files are skipped.** Inside git ig trusts git; outside git it
     skips the standard build folders.
2. **AI reading (optional).** Claude by default; Gemini or Codex only when named.
   - The agent gets ig's facts and returns what each box is: level, kind, pattern, title,
     description, badges, and how sure it is (`extracted`, `inferred` or `ambiguous`).
   - It runs only on `--read` or a yes at the terminal prompt.
   - Claude runs with read-only tools and without the repo's own agent settings and hooks.
   - The timeout kills the agent's whole process tree.
   - Repo text is fenced as data in the prompt.
   - ig validates every answer, re-asks once with the errors, and upgrades a claim to `extracted`
     when the code proves it.
3. **Compiler.** It combines both into `.ig/map.json`, one committed file the team shares.
   - It keeps annotations, pinned corrections and the agent's grouping across code changes.
   - New boxes are marked "needs reading".
   - It validates strictly and never saves a file it would reject later.
   - The output is byte-identical on every run, so diffs stay clean.

## Layers on the map

- **Rules:** wrong-way dependencies, in red (for example, the domain importing infrastructure in an
  onion).
- **Risks:** a query inside a loop, or three or more network hops, in amber.
- **Health:** read from `.ig/layers/*.json` files, the door for real production data later.

## The canvas (one offline HTML file)

- **Boxes:** one component with four looks (system, container, component and code levels), real
  logos, and health colors where "healthy looks like nothing".
- **Arrows:** right-angle routes that never cross a box or overlap, with label pills.
- **Pattern shapes:** onion rings, layered bands and a hexagonal core. Insides are laid out wide,
  and people, containers and outside services stay apart at the top level.
- **Navigation:** a smooth glide into any box, wheel or pinch zoom, drag to pan, a breadcrumb and
  Fit.
- **Details card:** health, what needs attention, connections, what the box is made of, and where
  each annotation came from.
- **Controls:** layer toggles and keyboard support (Esc, focus).
- **Safety:** text from the map file is always rendered as text, never as HTML.
- **Speed:** only the levels in view are drawn. shop-front (3,760 code items) builds in
  0.25 s.

## Using it

```sh
make build
cd <a copy of your repo>
./target/release/ig view            # asks once before any AI reading
ig view --no-read | --read | --agent gemini | --no-open | --json
```

- **Correct a box:** edit `.ig/map.json` and add the field name to that box's `"pinned"` list.
- **Simulate health:** create `.ig/layers/health.json`:
  `{ "layer": "health", "marks": [ { "target": "api", "state": "warn", "value": "p95 1.4 s" } ] }`
- **Share the map:** commit `.ig/map.json`. Everything else in `.ig/` stays out of git.

## Removed (kept in git history)

Plans, apply, review, check, focus, serve, design boards, the Cytoscape graph and the external
checkers are gone. Old setups keep working:
- `ig hook` exits quietly;
- `ig check` exits with a clear message;
- `ig init` removes the stale Claude Code hook.

## How it was built

- **Tasks:** 15 tasks, each implemented by a fresh agent and reviewed independently.
- **Final review:** one review of the whole branch, followed by one fix wave.
- **Tests:** 187 Rust tests and 70 viewer tests.
- **Trials:** run on scratch copies only. One real Claude reading of intake took 11 s.

## Still open

- **Monorepos with more than 12 packages:** grouped packages lose their folder level and are
  renamed at the threshold. The fix is designed, not built.
- **Gemini and Codex:** their read-only modes are not yet verified against the real CLIs.
- **Visual and timing checks** need a browser: the visual check, first paint and zoom smoothness
  (see the walkthrough).

## Next

- Ask Claude and see the answer drawn on the map.
- Live production health via OpenTelemetry.
- The map in the desktop app.
- More languages.
- A landscape view for a folder of several projects.
