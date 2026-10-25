# ig as an agent workflow: setup, explore, plan, apply, view

Status: design, not implemented. Date: 2026-10-25.

## Intent

Today `ig` is a CLI that a coding agent is pushed through (the Stop hook) and a viewer a person
opens by hand. The user wants `ig` to live inside the prompting system: a developer works in
Claude Code (or Codex) through a handful of `/ig:*` commands, and `ig` makes sure that

1. **before any code is written**, the developer sees the final result: the architecture after the
   feature lands, and the volume of the change (new code and impact on existing code);
2. **while the code is written**, the agent cannot finish while any team constraint is broken. The
   constraints are enforced by the team's existing tools plus a second, independent reviewer
   agent, and the agent asks the developer when it cannot comply;
3. **after**, the developer reviews the change as architecture plus real before/after code, next
   to the plan they approved.

Why: the user does not want to read long specs (openspec, superpowers plans). They want to read
architecture and real code. And they want robustness enforced, not suggested: "each time the code
breaks some rules the AI gets feedback and doesn't finish its task until it's fixed".

## Goals

- One install (`ig init`) gives a project the `/ig:*` commands in Claude Code and `$ig-*` skills in
  Codex.
- `/ig:plan` produces a reviewable *picture* of the end state before any code is written.
- `/ig:apply` cannot end with a broken constraint. It stops and asks the user when it is stuck.
- Constraints are written once, upfront, by the team, in tools they already use.
- The plan cannot silently drift from the code: `ig` compares them mechanically.

## Non-goals

- `ig` does not become a linter. It runs no style, lint or security rules of its own beyond what it
  has today (layers, complexity, structure, intents). Everything else comes from external tools.
- Not a replacement for openspec or superpowers. It can sit next to them. It does not read their
  documents.
- No server, no cloud, no new runtime dependency for the user. The viewer stays a single offline
  HTML file.
- No IDE extension.

## Anti-goals (a failure even if it technically works)

- A plan the user has to read as long prose. The plan is looked at as a diagram plus numbers. Its
  text is short.
- An agent that loosens a rule, edits the baseline, or silently changes the plan to get a green
  check. Anything that weakens a constraint needs the user's explicit yes.
- An agent that loops forever on a finding it can't fix.
- Rules duplicated between `ig.yaml` and the tool that really owns them.

## The commands

| Command (Claude / Codex) | Purpose |
|---|---|
| `ig init [claude\|codex\|all]` | Installs the commands, the Stop hook (Claude), ig.yaml and the baseline. Exists today; extended. |
| `/ig:setup` / `$ig-setup`, and `ig setup claude\|codex` from the terminal | Writes or improves ig.yaml **and wires the team's existing tools as checkers**. This is today's `ig-explore` skill and `ig explore` command, **renamed**. |
| `/ig:explore <question>` / `$ig-explore` | Answers questions about the existing system using the graph ("how does sign-in work, what would touch it?"). Read-only. |
| `/ig:plan <feature>` / `$ig-plan` | Writes a plan file, shows the planned end state and its volume, and checks the plan against the rules. |
| `/ig:apply [plan]` / `$ig-apply` | Implements the plan step by step under the constraint loop, then runs the reviewer agent. |
| `/ig:view` / `$ig-view` | Opens the viewer on whatever is current: plan, apply progress, brief, or the plain graph. |

The CLI facts command that `ig explore` is today (`ig explore [--json]`, a report for writing
ig.yaml) is renamed to `ig setup --facts [--json]` (builder's call on the exact flag), and `ig
setup claude|codex` launches the agent on the setup skill, as `ig explore claude|codex` does today.

### How `/ig:` works in Claude Code (verified 2026-10-25, Claude Code 2.1.310)

Claude Code namespaces skills by plugin name. A directory under the project's `.claude/skills/`
that contains `.claude-plugin/plugin.json` loads as a "skills-directory plugin" named after the
manifest. So `ig init claude` writes:

```
.claude/skills/ig/
  .claude-plugin/plugin.json        {"name": "ig", "skills": ["./skills"], ...}
  skills/setup/SKILL.md             -> /ig:setup
  skills/explore/SKILL.md           -> /ig:explore
  skills/plan/SKILL.md              -> /ig:plan
  skills/apply/SKILL.md             -> /ig:apply
  skills/view/SKILL.md              -> /ig:view
  agents/reviewer.md                the reviewer subagent (see Apply)
```

Caveat, observed: project-scope skills-directory plugins load only once the user accepted the
workspace trust dialog. `ig init` prints that. The builder must confirm the exact manifest shape
(`claude plugin init ig --with skills` scaffolds one; it used `"skills": ["./"]` with a root
SKILL.md) and that `agents/` in such a plugin is picked up. If not, the reviewer goes in
`.claude/agents/ig-reviewer.md` instead.

Codex has no namespaces: skills go in `.agents/skills/ig-<name>/SKILL.md` and are invoked as
`$ig-plan` and so on, as `ig-explore` is today.

All skill files are embedded in the binary (as `assets/skill-explore.md` is today), written by
`ig init` and never overwritten. The builder can choose to add `ig init --update-skills` to
refresh them.

## Constraints: where team rules live

Two kinds of rules, decided with the user:

### 1. Rules a tool can check: owned by existing tools

The user: "rely heavily on existing tools: stylelint, biome, oxc, oxlint, semgrep, … and can be
extended with other tools". So:

- The rule lives in the tool's own config (`.stylelintrc`, `biome.json`, `.oxlintrc.json`,
  `.semgrep/`, `tsconfig.json`), which the team already reviews in git. Examples from the user:
  - SCSS: mobile first, and breakpoint blocks come after the base styles instead of being nested
    in each class (stylelint);
  - JS/TS: arrow functions, or other code-style rules (oxlint, biome, eslint).
- `ig.yaml` only lists the tools, in the existing `checkers:` key, extended with `format`:

  ```yaml
  checkers:
    - name: stylelint
      run: "npx stylelint '**/*.scss' --formatter json"
      format: stylelint-json
    - name: oxlint
      run: "npx oxlint --format json"
      format: oxlint-json        # or whatever oxlint prints; builder verifies
    - name: semgrep
      run: "semgrep --config .semgrep --sarif --quiet"
      # format defaults to sarif
    - name: tsc
      run: "npx tsc --noEmit -p ."
      format: lines              # file(line,col): message  /  file:line:col: message
  ```

- **Extensible by output format, not by plugin code.** Supported formats: `sarif` (default, as
  today), `eslint-json` (also emitted by stylelint? builder verifies; otherwise `stylelint-json`),
  `checkstyle`, and `lines` (a regex for `path:line:col: message` and tsc's `path(line,col)`). Any
  new tool that prints one of these works without changing `ig`. The builder decides the exact set
  after checking what stylelint, biome, oxlint, semgrep and tsc each print. The goal is that all
  five work with no wrapper package.
- What `ig` adds on top of the tools (exists today for SARIF checkers; keep it for all formats):
  only findings on changed lines block, old findings go in the baseline, all tools feed one
  feedback list, and severity and `blockOn` work as for built-in rules.
- `/ig:setup` detects which tools the repo already has (config files, devDependencies) and wires
  them. It installs nothing without asking. When a team states a rule in plain words, the setup
  skill may translate it into the right tool's config and prove that it fires on a deliberately bad
  snippet (written to a temp file, then deleted).

### 2. Rules only a reader can judge: a reviewer agent

Examples: "components stay dumb, logic lives in composables", "API errors are typed, never bare
strings". These live in a plain Markdown file owned by the team, `rules.md` at the repo root
(builder may pick the name; one file, one rule per heading or bullet, each with a stable id like
`R-COMPOSABLES`). `ig init` creates it with a commented example, like `intents.md`.

Plan and apply always load `rules.md` as guidance. Enforcement is by a second agent (see Apply).

## `/ig:explore <question>`

Read-only. The skill tells the agent to answer from the graph rather than by grepping alone. It
uses `ig` to find the modules, functions, routes, lanes and edges involved (the MCP server or a
new CLI query, builder's choice), then reads the code it points at. The answer is short prose plus
a small Mermaid diagram (flowchart or sequence) in chat. It also records the node ids it talked
about as the current *focus* (e.g. `.ig/focus.json`), so `/ig:view` opens on them.

If `ig` lacks a query the skill needs (e.g. "callers of X up to depth N", "path from page P to
system S"), the builder adds it as a CLI subcommand with `--json` output. Keep it to what the
skill actually uses.

## `/ig:plan <feature>`

### What the user sees

1. The **final architecture**, in the viewer, in the same style as a real change (Changed /
   Architecture / Blueprint modes): planned new modules and functions marked *new*, existing ones
   the plan modifies marked *changed*, planned links drawn with their lane and layer colours. For a
   small feature the chat summary is enough; for a big one the diagram is the point.
2. The **volume**, computed by `ig` from the real graph, not estimated by the agent. Both kinds:
   - new: files, functions, routes, external systems;
   - impact on existing code: functions changed, and the blast radius, i.e. the existing code that
     depends on what changes (transitive callers and importers, grouped by lane and layer, with
     counts);
   - layers and lanes crossed; new cross-layer or cross-lane links; new external systems.
3. **Rule problems before code**: planned links that break a layer rule (e.g. web → pseudonym),
   a planned new dependency cycle, a new external system. Reported as findings on the plan.

### The plan file

A small Markdown file in git, e.g. `plans/<slug>.md` (builder's call on the directory):

````markdown
# Patient export

Intent: clinicians export a filtered patient list as CSV. Two or three lines at most.

## Steps
1. shared: export row schema
2. api: GET /api/patients/export, service + query
3. web: export button on the patients view

```ig-plan
nodes:
  - id: shared/src/export.ts#exportRow        # file#name, the graph's id form
    kind: function
    status: new
  - id: api/src/patients/service.ts#listPatients
    status: changed
  - id: "route:GET /api/patients/export"
    kind: route
    status: new
  - id: web/src/views/PatientsView.vue
    status: changed
edges:
  - from: web/src/views/PatientsView.vue
    to: "route:GET /api/patients/export"
    kind: http
  - from: api/src/patients/routes.ts
    to: api/src/patients/service.ts#exportPatients
    kind: calls
systems: []                                    # new external systems, by blueprint.systems name
```
````

- The prose is for humans and stays short. The `ig-plan` block is the contract `ig` reads.
- Node ids use the graph's real id format (the builder documents it in the skill), so `ig` can tell
  "existing node, changed" from "new node". A `changed` id that doesn't exist in the graph is an
  error, reported back to the agent.
- `ig plan check <file>` (new CLI) merges the plan onto the current graph, computes the volume and
  blast radius, runs the arch and cycle checks on the merged graph, and prints the result (`--json`
  for the skill). `ig view --plan <file>` renders the merged graph.
- The skill: read the feature request, use `/ig:explore`-style queries to find what exists, write
  the plan file, run `ig plan check` until it is free of errors, then show the user the chat
  summary (volume, findings) and open the viewer. The user approves, edits, or asks for changes.
  Nothing is implemented in `/ig:plan`.
- Which plan is current: `.ig/current-plan` holds its path (builder's call). `/ig:apply` with no
  argument uses it.

## `/ig:apply [plan]`

Implements the current plan under a constraint loop.

### The loop

- Work step by step, following the plan's Steps. After each step, run `ig check --agent` (the
  built-in checks plus all checkers plus plan conformance). Fix everything blocking before starting
  the next step, so a problem from step 1 isn't built on by steps 2–5.
- **Plan conformance** is a new check (`ig/plan`), active when a current plan exists. It compares
  the real change (base vs head graph, as `ig check` computes today) with the plan:
  - *unplanned*: a new node, a cross-layer or cross-lane edge, or an external system that isn't in
    the plan. Blocking.
  - *missing*: planned and not built. Not blocking mid-apply, blocking at the end. The builder
    decides how the end is detected: the skill passes `--final`, or the Stop hook treats every stop
    as the end.
  - New private helpers inside a planned file aren't "unplanned". Only nodes that are exported, or
    that cross a file, count. The builder tunes this so the plan doesn't need every helper.
- **The Stop hook** (exists) keeps blocking the end of the turn while anything blocks. It now covers
  checkers in all formats and `ig/plan`.
- **The reviewer**: when the tools are clean and the agent believes it is done, a separate
  reviewer agent runs (below). Its blocking findings join the same loop.

### When the agent is stuck: ask the user

The user's decision: when the agent can't fix a finding, or believes a rule is wrong, it **asks
the user**. It doesn't loop, and it never weakens a constraint by itself.

- Trigger: the same finding (fingerprint) survives N fix attempts (default 3; the existing
  `hookMaxRounds` or a new key, builder's call), or the agent judges the rule inapplicable.
- The agent shows the finding (rule, file:line, why it can't comply, in one or two lines) and
  offers concrete options. In Claude it uses its question UI; in Codex it asks in chat:
  1. a different approach (and what changes in the plan);
  2. update the plan to allow it (the plan file is edited and `ig plan check` re-run, so the user
     sees the new diagram);
  3. accept this one finding as an exception (recorded, e.g. added to the baseline with a reason,
     so it shows up in review instead of being hidden);
  4. change the rule (in ig.yaml, the tool's config, or rules.md).
- Options 2–4 happen only after the user picks them. The skills say so explicitly, and plan
  conformance cannot be satisfied by the agent editing the plan unasked: ig records the plan's hash
  at approval time and reports a changed plan in the findings (builder's mechanism).

### The reviewer agent

The user's decision: judgment rules go to a second agent.

- **Fresh context**: it does not see the implementing agent's conversation. Its input is only:
  `rules.md`, the approved plan, and the change as `ig brief --md` renders it (architecture plus
  per-function before/after code). It may read files to check context.
- **Output**: findings in the same shape as tool findings (rule id from rules.md, file, line,
  message, severity), as JSON. `ig` ingests them as checker `ig-review` (e.g. via a file
  `.ig/review.json` that `ig check` reads when it is newer than the change, builder's call), so
  they block like any other finding and the agent gets them in the same feedback list.
- **When**: once per completion attempt, after the tools are clean, on the diff only. It isn't run
  on every hook round.
- **Disagreement**: if the implementing agent thinks a reviewer finding is wrong, that's the "ask
  the user" path. If the user rules it a false alarm, it is recorded (baseline with reason), so
  the same finding doesn't block again.
- **Claude**: a subagent shipped in the plugin (`agents/reviewer.md`), which the apply skill invokes.
- **Codex**: `ig` (or the skill) runs `codex exec` with the reviewer instructions and the inputs, and
  reads the JSON it prints. To be verified by the builder. If it's not workable, the Codex apply
  skill falls back to doing a separate review pass itself, and the docs state that it's weaker.

### Apply state, base, and lifecycle (author's calls, not discussed with the user)

- **Apply in progress** is its own state, separate from "a plan exists". `/ig:apply` runs
  `ig apply start <plan>`, which writes `.ig/apply.json` holding the plan path, the plan's hash and
  the **base commit** (`HEAD` at start). `ig apply finish` clears it once the final check is clean.
- The Stop hook enforces `ig/plan` and the reviewer only while `.ig/apply.json` exists. Plan,
  explore and ordinary chat turns are never blocked by plan findings. The built-in checks and the
  checkers keep blocking as they do today.
- **Base for plan conformance:** the recorded base commit, not the merge-base. Earlier work on the
  branch isn't "unplanned", and working on `main` is fine. Apply does **not** commit. Committing is
  the user's step, as it is today.
- **The round cap while applying:** when `hookMaxRounds` is reached during apply, the hook doesn't
  silently let the turn end. Its message tells the agent to stop and ask the user, with the options
  from "When the agent is stuck". Outside apply, the cap works as it does today.
- **End of a plan:** `ig apply finish` sets `status: done` in the plan file, and `/ig:view` shows
  "apply finished" for the plan most recently finished, until the next `/ig:plan` or apply. A new
  `/ig:plan` replaces `.ig/current-plan`. Done plans stay in git as the record of what was decided.

### Migration from today's `ig-explore` (author's call)

The only installs are the author's own repos. `ig init` replaces an existing `ig-explore` skill
only if it is byte-identical to a previously shipped version, which means the user never edited
it. Otherwise it prints a warning. `ig explore` with no agent argument keeps printing the facts
for one release, with a note pointing to `ig setup --facts`.

## `/ig:view`

No arguments. It shows whatever is current:

| State | View |
|---|---|
| current plan, nothing built yet | the plan's final state (`ig view --plan`): new/changed nodes, the volume and blast radius as a panel |
| current plan, apply in progress | the change so far overlaid on the plan: built, still missing, unplanned |
| apply finished | the brief next to the plan, i.e. the review screen |
| no plan | the normal viewer (Architecture, Blueprint, tree); focused on `.ig/focus.json` if an explore answer just set it |

It uses `--watch` so it refreshes while the agent writes. The view is still the single offline HTML
file. The viewer needs these additions: a status `planned` (not yet built) for nodes and edges, a
"plan" overlay, and a volume panel. The builder designs them in the style of the existing Changed
mode.

## Trust model (decided with the user)

- **The agent is assumed to cooperate but make mistakes; it isn't assumed to be hostile.** Skill
  instructions plus `ig check` are the enforcement. No locking, no signed reviewer output.
- **Cheap tamper detection on top:** `ig apply start` records hashes of the plan, `ig.yaml`,
  `rules.md`, the checker tools' config files and `.ig/baseline.json`. While apply is in progress,
  `ig check` reports a blocking `ig/tamper` finding for any of them that changed. The agent then has
  to ask the user, who either accepts the change (`ig apply accept`, which re-records the hashes) or
  rejects it. The finding has to be visible, but it doesn't have to be unforgeable.
- **Codex:** no blocking end-of-turn hook is known. There, "cannot finish while broken" rests on the
  `$ig-apply` skill running `ig check --agent` until it is clean. This is accepted, and the README
  states that Codex enforcement is weaker than Claude's.

## Viewer as a frontend project (added 2026-10-25, decided with the user)

- The viewer moves out of `assets/` (plain JS plus vendored libraries) into `viewer/`: a TypeScript project built with Lit web components and Vite, using bun and Node. Libraries such as Cytoscape, fcose, Mermaid, simple-icons and lucide are npm packages.
- Building `ig` takes two steps: first the viewer (`bun run build`, which produces one self-contained offline HTML file), then the backend (`cargo build`, which embeds that file through `build.rs`). `make build` runs both. Without a built viewer, cargo still builds and embeds a placeholder page.
- System icons come from packages: `simple-icons` brand logos for known systems, and `lucide` icons by category as the fallback. They replace the hand-drawn icons first planned for Task 9.
- The Architecture view gets a layered row layout like Blueprint, with external services in the bottom row, and a toggle to hide links.

## Constraints

- Rust, existing crates. No new runtime dependency for users. External tools are run only if the
  repo already uses them.
- Viewer: one offline HTML file with no network access at runtime. Its libraries come from npm and are bundled at build time.
- Never overwrite user files (`ig init` rule today). Skills are rewritten only on an explicit
  update command.
- The user's git rules: nothing pushes. Skills that commit (if any) never add AI attribution.
- Existing `ig check`, `ig brief`, the Stop hook and the baseline keep working unchanged for users
  who never use a plan.

## Left to the builder

- Exact CLI flag names and the plan directory.
- The checker output formats to support. Requirement: stylelint, biome, oxlint, semgrep and tsc all
  work out of the box.
- How "end of apply" is signalled for the *missing* check.
- How reviewer findings are handed to `ig check`.
- The graph queries `/ig:explore` needs.
- The viewer's plan and volume UI.
- Order of delivery. A suggestion that keeps each step useful on its own: (1) the rename to setup
  and the plugin layout; (2) checker formats; (3) `ig plan check` and `ig view --plan`; (4) apply
  with `ig/plan` conformance and ask-the-user; (5) the reviewer; (6) the `/ig:view` state logic and
  explore.
