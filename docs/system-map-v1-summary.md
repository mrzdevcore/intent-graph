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
     hexagonal or none. Loose files get a "root" component. Deeper down, folders whose names
     show a pattern (`domain/`, `application/`, `use-cases/`, `infrastructure/`…) are laid out
     in its shape too.
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
3. **Compiler.** The first run combines both into `ig/*.kdl`, the committed spec the team shares;
   ig never rewrites it, and later runs compare it with the code (built, to build, drift).
   - A repo with `.ig/map.json` keeps its annotations, pinned corrections and the agent's grouping.
   - Code no spec box claims is drawn as drift.
   - It validates strictly and never writes a spec it would reject later.
   - The output is byte-identical on every run, so diffs stay clean.

## Layers on the map

- **Rules:** wrong-way dependencies, in red (for example, the domain importing infrastructure in an
  onion).
- **Risks:** a query inside a loop, or three or more network hops, in amber.
- **Health:** read from `.ig/layers/*.json` files, the door for real production data later.

## The canvas (one offline HTML file)

- **Boxes:** one component with four looks (system, container, component and code levels), real
  logos, and health colors where "healthy looks like nothing".
- **As deep as the code goes:** below the components, the code level nests folders, files, classes,
  functions and methods. A box holds at most 12; past that its items are grouped by subfolder or
  file, else by name prefix, else alphabetically, and every group opens. bigshop goes 11
  levels deep. Zoomed into, a function or method shows its code with line numbers; the details
  card shows it too, with its file, line and complexity.
- **Ansible trees:** in a repository with no code ig reads but with Ansible roles
  (`**/roles/*/tasks`), `ig view` draws the roles instead (`map/ansible.rs`). Containers are where
  roles deploy (`meta/targets.yml`; small targets put together, helpers apart), each role a
  component (API, front, job, helper by its name), or past 12 roles per kind with each role a
  folder inside. A role's tasks and files are its code level, with their YAML. Arrows: a task
  including a role or rendering a template, a template's `*_API_HOST`/`*_URL` settings naming
  another role, and the outside systems its settings name (`AUTH0_`, `ELASTIC_APM_`,
  `DATABASE_`…). Read as text (Jinja is not YAML). The infrastructure repo: 75 roles, 701 boxes,
  0.7 s.
- **UI components:** a Vue single-file component (with `embedded: [".vue"]` in `ig.yaml`) or a
  React function component is one box with its framework's logo. It opens onto its parts:
  Inputs (props, emits, slots, models), State (refs, computed, data, watchers, `useState`…), Uses
  (composables, stores, hooks), Template or Renders (the components it draws, with the template's
  code), then its own functions. Read from the syntax by `ig-lang-ts` (`anatomy`): `<script
  setup>`, the Options API and React hooks; a props type is resolved only in the same file.
- **Arrows:** curves from the side of a box facing the other end to the opposite side of that
  box, drawn by the viewer from where the boxes are, so they follow a box moved by hand. The boxes
  of one grid trade places while that leaves fewer arrows crossing or cutting through boxes
  (bigshop: about half the crossings). The arrows of one box side leave from separate
  points, ordered by where their other end sits. A curve may pass over a box between its ends
  (the right-angle routes `ig` computes are no longer drawn). They thicken with the edges they carry. At rest they are faint (a rule's
  or risk's stays in full); the box or arrow under the pointer, or the selected box, lights its
  own with their labels, above the boxes, and dims the others further.
- **Zooming in:** lines, heads, labels, region names and an open box's header keep a steady size
  on screen. A level far behind the box you are in stops drawing its arrows and regions, and an
  open box keeps its title in a header above its inside. Only boxes on or near the screen are
  drawn, from the deepest open box that covers the whole view, so zooming deep stays fast and
  free of huge-scale jitter.
- **Pattern shapes:** onion rings, layered bands and a hexagonal core. Insides are laid out wide,
  and people, containers and outside services stay apart at the top level.
- **Navigation:** a smooth glide into any box, wheel or pinch zoom, drag to pan, a breadcrumb and
  Fit. Dragging a closed box moves it inside its parent; moves are kept per map in the browser
  (localStorage) and Reset layout undoes them. A drag redraws only that box and its parent's
  arrows; the map is laid out again once, on release.
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

- **Correct a box:** edit its node in `ig/*.kdl`.
- **Simulate health:** create `.ig/layers/health.json`:
  `{ "layer": "health", "marks": [ { "target": "api", "state": "warn", "value": "p95 1.4 s" } ] }`
- **Share the map:** commit `ig/`. Everything in `.ig/` stays out of git.

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

- Live production health via OpenTelemetry.
- The map in the desktop app.
- More languages.
