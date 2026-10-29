# ig: a linter for system design, and a map to build on

Status: design, not implemented. Date: 2026-10-29. Replaces the add-only draft of this file and
`2026-10-25-ig-design-workspace-design.md`. Mockups: the "Design mode" row of the design canvas
(https://claude.ai/artifact/Wny722GMWPs6bRBu7KZy9k); they predate the multi-repo and refactoring
decisions below, so the decisions win where they differ.

## Intent

ig helps people build better, scalable software in the AI era by keeping a system's design visible
and true, and by making the design drive the work instead of trailing behind the code.

In the user's words, ig is **a linter for system design**: like ESLint or Semgrep, but for the shape
of a system (services sharing data, cycles, layers, call chains) instead of lines of code. Teams set
its rules in config; it runs locally, in CI, and live while an agent builds.

It has two sides on one map:

- **Understand:** one map of the whole system, even across many repos, from the bird's-eye view
  down to the code.
- **Build:** change the system on the map (from an empty project, or on top of an existing one,
  refactoring included); Claude proposes and builds; ig checks the code against the map and the
  rules.

The user's rule for this work: **few features, built very well**.

## Scope of this work (decided with the user)

**Now: editing**, in one repo (a system of one): start from an empty repo or an existing one, edit
the map in every way (add, delete, move, rename, split, merge), Claude proposes and builds, ig checks
the code against the map. Built in this order, each finished before the next:

1. **@ mentions:** boxes referenced in the chat with Claude (small; it is how the user points Claude
   at what to edit; kept in scope as my call).
2. **Changes:** draw the system as it should become; Claude proposes and builds; ig checks.

**Later, not in this work:** many repos as one system, and the rules as a configurable linter. Their
direction is kept below so nothing built now blocks them.

## 1. @ mentions: boxes in the chat

- Typing `@` in the desktop app's chat opens a list of boxes narrowing as you type, using the map
  search's matching (title, then kind or tech, then file path). A pick becomes a chip.
- Selecting boxes on the map and a button or shortcut adds them as chips to the message being
  written. (My call: pointing is often faster than typing.)
- Claude receives, per chip: the box's id, where it sits (its parents), its files, its arrows in and
  out, and whether it is part of the open change.
- Claude points back: boxes it names show as the same chips; hovering lights the box on the map,
  clicking flies there. (My call.)

## 2. Changes: the map as it should become

### The model

- A **change** is the map as it should become: boxes and arrows added, deleted, moved (to another
  parent or another repo), renamed, split, merged; plus "will change" notes on real boxes. No
  add-only limit (decided with the user).
- **Every planned box says where its code lives** (decided with the user): a folder or file glob,
  such as `src/billing/**`. Claude proposes it with the box, the user can edit it, and it shows small
  on the box. ig judges with its own deterministic analysis, never Claude's word or an AI reading:
  a planned box is built when code exists there and ig's analysis puts it in that box; a deleted box
  is gone when its place holds no code; a moved part is moved when its code sits at the new place.
  Planned arrows are built when ig's analysis finds them between those places.
- Every moved or renamed item keeps a link to where it came from, so ig can tell "moved to Billing"
  from "deleted, and something new created". (My call.)
- Deleted items stay visible, struck through, until their code is gone.
- An empty project is a change on an empty map: the user opens an empty repo (a folder, `git init`
  done or done by ig) in the desktop app and designs the whole system; Claude builds it in that
  repo. (Later, with many repos, starting empty grows into creating new sibling repos.)
- A change lives in the repo's `.ig/`, committed text with stable order and readable diffs, written
  only through ig and validated strictly like `.ig/map.json`.
- One open change at a time. (My call.)

### Who does what

- **The user** draws and edits in the desktop app: add, delete, move, rename, draw arrows, undo and
  redo, keyboard first. The `ig view` page stays read-only and shows an open change dashed.
- **Claude** edits the change only through ig's tools, never the file directly. Its edits are gold
  until the user keeps them (one by one or all), and always undoable.
- **Refactors are proposed by Claude** (decided with the user: moving parts by hand one by one is
  tedious). The user says what they want, on the map or in the chat ("split Shop API into Orders
  and Billing"); Claude proposes on the map which parts and arrows go where, which arrows appear
  between the new pieces, and flags what is unclear; the user adjusts only what is wrong and keeps
  it.
- **Rules show while drawing:** a planned arrow or placement that breaks one of the rules ig checks
  today (layers and patterns from `ig.yaml`) is marked at once.

### Build and check

- One action hands the kept change to Claude in the app's chat, running in the repo.
- After each Claude turn (and on file changes while building) ig re-reads the code and compares it
  with the change: what is built, what is not yet, what should be gone and is not, arrows the code
  has that the change did not plan, and breaks of the rules ig checks today (layers and patterns,
  calls in loops, complexity). The same status feeds the map, a
  bar ("3 of 5 built · 1 arrow not planned"), and Claude through ig's tools.
- **Done is decided by the check, not by Claude.** Claude may not finish while the code does not
  match the change or a layer rule is broken; when it cannot fix something it asks the user. When all
  matches, the change becomes the map and is closed: titles, descriptions and declared arrows from
  the change become the user's own entries in `.ig/map.json` (as hand corrections are today, so
  later analyses keep them); "moved from" links are dropped, git keeps that history. (My call.) How "may not finish" is enforced (Claude's
  instructions alone, or a hook that blocks it) is the builder's call; it must hold in practice.

## Later: the system, many repos as one map

Not built in this work; recorded so nothing above blocks it. In this work the system is one repo,
and the change file lives in that repo's `.ig/`.


### Layout (decided with the user)

```
my-shop/                 ← parent folder, not a repo
├── system/              ← the system repo: config, system map, changes
│   └── ig-system.yaml   ← lists the repos
├── orders-api/          ← a repo
├── payments/            ← a repo
└── web/                 ← a repo
```

- One config file for the whole system, in its own `system/` repo, listing the sibling repos. Not
  a file in each repo: one place says what the system is, and the repos stay untouched.
- `ig-system.yaml` holds the system's name and, per repo, a name and a path relative to `system/`
  (`../orders-api`), plus optionally its git URL (so a teammate missing it can be told what to
  clone; cloning for them is later).
- Each repo keeps its own `ig.yaml` for its own analysis, and its own map as today.
- `system/` holds what belongs to the whole system: the system-level map (repos, the arrows
  between them, shared infrastructure), the system rules, and changes.
- A single repo is a system of one: plain `ig view` in a repo keeps working exactly as today.

### What the map shows

- The top level is the system: one box per repo (service, app, library), plus infrastructure.
- **Infrastructure gets real boxes:** databases, caches and queues each become one box shared by the
  services that use them (today a database is a box only when code imports its client).
- **Arrows between repos, from evidence first**, for example: a frontend's HTTP call matched to the
  API route that serves it; a service's `DATABASE_URL` / `REDIS_URL` matched to the database or
  cache a Compose, Kubernetes or Terraform file declares; a client generated from another repo's
  OpenAPI spec. Which evidence sources ship first is the builder's call, ranked by how common they
  are in TS/JS systems.
- **Arrows ig can't see** (through a gateway, a queue with a dynamic topic) are declared by Claude
  or by the user, and marked declared, not proven, as boxes already are.
- Walking into a repo's box shows that repo's map, as today.

### Constraint

ig only analyses TypeScript and JavaScript today. A repo in another language still appears (from the
config, with declared arrows and infrastructure from deployment files) but has no inside until a
frontend for its language exists. Language frontends are not part of this work.

## Later: the rules (the linter)

Not built in this work; recorded so nothing above blocks it.


- **ig ships a rule set.** Each rule has an id, a one-line why it matters, settings with defaults,
  and a level: blocking or warning. Starting set (the builder may add rules that fit the same
  model, never remove these):
  - between services: each service owns its data (two services writing the same database tables);
    no cycles between services; a frontend never reaches a database or cache directly; synchronous
    call chains no longer than N services;
  - inside a service: layers point inward (exists today); no cycles between parts; no reaching into
    another part's insides, only its public entry point;
  - in the code: calls inside loops (exists today); complexity per function (exists today).
- **Each team decides, in config** (decided with the user): which rules are on, their settings,
  blocking or warning. System rules in `system/ig-system.yaml`, repo rules in each repo's `ig.yaml`
  (which already has `layers`, `rules` and severities: build on them).
- **Custom rules** in a simple form checkable from the map: "nothing in `web` talks to `payments`
  directly", "`billing` may only use the `payments` API".
- **With no config**, the default set is on as warnings: useful at once, blocking nothing. (My call.)
- **Where rules show:** on the map (the box or arrow, the rule, why, where to turn it off); in a CLI
  command that prints findings and exits non-zero on a blocking rule, for CI; and to Claude through
  ig's tools while it builds.

## Non-goals

- Many repos as one system, and the rules as a configurable linter (later, see above).
- Screens, mockups, Figma, images.
- Several open changes at once; branches of changes; history beyond git.
- Editing code in the app (code is read in the app; Claude or the user's editor writes it).
- Multi-user, real time, hosting; cloning repos for the user.
- Free shapes, floating text, hand-picked colours on the map.
- Code generation without Claude (templates, scaffolders).
- New language frontends.

## Anti-goals

- A linter people turn off: findings that are noisy, vague, or not actionable. Every finding names
  the rule, the place, why it matters and how to change or silence it.
- Drawing that feels like forms: a dialog per box, no keyboard, no undo.
- Claude changing the map behind the user's back.
- A change that silently drifts from the code.
- A design side that slows down or clutters the map for reading.

## Left to the builder

- File formats (`ig-system.yaml`, the change file, rule config, custom rule syntax), tool schemas,
  the CLI command's name and output.
- Which cross-repo evidence sources ship first.
- Exact interactions and shortcuts for drawing and @ mentions, designed on the canvas before code
  (the project's design-before-code rule).
- How the layout places new and moved boxes and keeps them stable.
