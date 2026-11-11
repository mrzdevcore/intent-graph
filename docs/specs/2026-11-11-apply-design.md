# ig apply: build what the plan says

Date: 2026-11-11. Asked by the user: "we need to implement the plan, and add the apply skill and the
command too".

## Intent

`ig plan` lists what the map wants and the code does not have yet ("to build": boxes and arrows in
`ig/` with no code behind them, and "move": code that sits in the wrong box). Today nothing turns that
list into code: a person reads it and asks their agent by hand. Arrows already carry a short `note`
and a longer `context` written for "the agent implementing them". `apply` is that agent's job,
like `terraform apply` after `terraform plan`.

## Goals

- **The skill `apply`** (`/ig:apply` in Claude, `$ig-apply` in Codex), installed beside explore, view
  and draw. It tells the coding agent to:
  1. call `ig_plan` (or run `ig plan --json`) and take the "to build" and "move" items, or only the
     ids the user named;
  2. for each box: read its kind, title, description, tech, `at` globs, `context` and its arrows;
     write the code under its `at` (create the folder and the project files a new block needs, in
     the stack its `tech` names or the repo already uses);
  3. for each arrow: make `from` really depend on `to` in the way its kind says (`calls`: a client
     call, `reads`/`writes`: a query or a repository, `uses`: an import, …), following its `label`,
     `note` and `context`;
  4. for each move: move the code into the box's `at`, keeping behaviour;
  5. build and run the repo's tests the way the repo does, then call `ig_plan` again: an item is
     done when it leaves "to build"; repeat until done or stuck, and say what is left and why;
  6. never edit `ig/` (that is the draw skill, and only when asked); drift it creates is listed for
     the user to `ig accept` or fix, never accepted by the agent.
- **The command `ig apply [IDS…]`**: runs the agent with the apply skill's instructions on the plan
  (all of it, or the named ids), like `ig explore` runs one for a question.
  - It prints the items it will build first and asks to go on (`--yes` skips the question; with no
    terminal and no `--yes` it stops), because the agent writes code.
  - Claude only for now (as `ig explore`); `--agent` as in explore.
  - Tools: Read, Grep, Glob, Edit, Write, the ig MCP tools `ig_plan` and `ig_state`; Bash only with
    `--allow-bash` (to build and run tests); the secret rules of the reading runner stay; `ig/` and
    `ig.yaml` are denied to Edit and Write.
  - After the agent: print the agent's summary, then the new plan (what is still to build, the
    drift the change brought), exit 0 when every chosen item was built, 1 otherwise, 2 on error.
  - Nothing to build: say so and do not start the agent.
- **AGENTS.md snippet** and `ig init` mention `/ig:apply` (`$ig-apply`).
- README, docs/reference.md and the man page list the command and the skill.

## Non-goals

- Editing `ig/`, accepting drift, committing, pushing, opening a PR.
- Agents other than Claude for the command (the skill itself works in Codex).

## Decisions (my calls, to check)

- The command asks before it starts, because it writes code (unlike analyse and explore, which only
  read).
- Bash is off by default; `--allow-bash` lets the agent build and test.
- The map is the contract: the agent never edits `ig/` to make the plan pass.

## Done when

- `ig init` / `ig plugin` install the apply skill for Claude and Codex; the AGENTS.md snippet names it.
- `ig apply` on a repo with nothing to build says so and runs no agent; with items, lists them and asks;
  `ig apply --yes api->db` runs the agent on that one arrow.
- Tests: skill installed for both agents; argv carries the write tools, the ig/ deny rules, the secret
  rules and no Bash unless `--allow-bash`; nothing-to-build path; named ids filter the plan (unknown
  id → error listing the plan's ids); exit codes.
