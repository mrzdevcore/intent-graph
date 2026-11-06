# The ig command line, built around its journeys

Date: 2026-11-06. Status: proposed.

## Intent

ig exists to make a project easy to hold in one's head, for a person and for a team. The map is the
shared picture; `ig/` is the intended architecture, committed and owned by the team.

The command line grew one feature at a time and no longer says this. One help text serves every
command, flags are accepted by commands that ignore them, and commands that sound read-only write
`ig/` (`ig plan`, `ig view`). This project rebuilds the command line around three journeys, follows
POSIX utility conventions, and makes the rule simple: **only the commands that say they write `ig/`
write it.**

### The journeys

1. **Understand an existing repo:** `ig init` → `ig analyse` → `ig view` → `ig explore "how does X work"`.
2. **Start from an intent (empty repo):** design `ig/` → `ig plan` → `ig apply`.
3. **Review a change:** `ig diff <base>` locally, and a comment with a Mermaid diagram on GitLab MRs
   and GitHub PRs.

This project builds journey 1 and the command line all three stand on. Journey 3 (`ig diff`, MR
comments) and journey 2 (the design step, `ig apply`) are later projects, in that order.

## Goals

- POSIX and GNU argument conventions, per-command help, errors on misuse, shell completions.
- Each command's effect on disk is what its name and help say.
- Journey 1 works end to end on a real repo with the four commands above.

## Non-goals

- `ig diff`, MR/PR comments, `ig apply`, the design step: later projects.
- New features in the desktop app, the connectors or the folder-of-projects landscape (frozen:
  they keep working, nothing new is built for them).
- Changes to the map engine beyond the non-writing path below.

## Anti-goals

- A command that writes `ig/` without saying so in its help.
- A flag silently ignored.
- Results mixed with messages: anything a script reads goes to stdout, everything else to stderr.
- Breaking `ig mcp` or `ig plugin export`, which the desktop app and the plugin run.

## The commands

```
ig — architecture map of your repo, and a check that the code matches it

Usage: ig [OPTIONS] <COMMAND>

Commands:
  init       Set up this repo: ig.yaml, agent skills, AGENTS.md snippet
  analyse    Read the code and write ig/ (or propose updates to it) for you to review
  view       Build the map and open it in the browser
  explore    Ask a question; the answer is drawn on the map
  plan       Show what is left to build, to move, and the drift
  check      Same as plan, for CI: exit 1 when anything is left
  accept     Take drift into ig/
  fmt        Re-indent ig/*.kdl
  app        Open the desktop app on a folder
  help       Help for a command

Plumbing:
  mcp            MCP server on stdio (for coding agents)
  plugin export  Write the Claude plugin into a folder
  completions    Shell completions (bash, zsh, fish)

Options:
  -C <DIR>         Run as if started in DIR (alias: --root)
  -q, --quiet      Only results; no notices or warnings
  -h, --help       Help
  -V, --version    Version

Exit status: 0 ok, 1 check or fmt --check found something, 2 usage error or failure.
```

(The exact help wording is the builder's; the commands, options and the plumbing section are decided.)

### What each command reads and writes

| Command | Options | Writes |
|---|---|---|
| `ig init [claude\|codex\|all]` | | `ig.yaml`, skills, `AGENTS.md` snippet, `.gitignore` entries. **Not `ig/`.** Ends with "next: `ig analyse`". |
| `ig analyse` (alias `analyze`) | `--agent <claude\|gemini\|codex\|ollama>`, `--no-agent` | `ig/`. No `ig/` yet: the whole intended architecture from the code, boxes named and described by the agent (unless `--no-agent`). With `ig/`: the reading's proposals for the drift and the boxes without a description (today's `view --read` on an existing `ig/`), journaled, reviewed with `git diff ig/`. Ends with "next: `ig view`, then commit `ig/`". |
| `ig view` | `--no-open`, `--json` | `.ig/view.html` only. No `ig/`: the map of the code, and the notice "no ig/ yet: run `ig analyse`". Never runs an agent. |
| `ig explore <QUESTION>` | `--agent <claude>`, `--no-open` | `.ig/show.json` (through `ig_show`) and `.ig/view.html`. Prints the agent's short answer to stdout, then opens the map on it. |
| `ig plan` | `--json` | Nothing. No `ig/`: "no ig/ yet: run `ig analyse`", exit 2. |
| `ig check` | `--json` | Nothing (as today). Same message and exit 2 without `ig/`. |
| `ig accept <ID>...` / `--all` | | `ig/` (as today). Error on neither or both. |
| `ig fmt` | `--check` | `ig/` unless `--check` (as today). |
| `ig app [DIR]` | | Nothing; opens the app (as today). |
| `ig mcp`, `ig plugin export <DIR>` | | As today. |
| `ig completions <SHELL>` | | Nothing; prints the script. |

`--json` on `plan` and `check` prints `{build, move, drift, notRead}`, the object `ig_plan`
already returns over MCP.

### `ig explore`

The agent answers in at most 10 lines and calls `ig_show` to draw the answer. *(my call)* It is
started by its own runner, not today's reading runner (whose `--safe-mode` turns MCP off): the
Claude CLI with a strict MCP config naming only `ig mcp`, and the tools `Read`, `Grep`, `Glob`,
`mcp__ig__ig_state`, `mcp__ig__ig_show` and the one shell command `ig view --json`, from which it
takes box ids and arrows as the explore skill does. Never `ig_edit` and no other shell; the repo's
hooks and settings are not loaded. The prompt is the explore skill's instructions plus the
question. ig then renders `.ig/view.html`, which already
opens on an answer less than an hour old, and opens it unless `--no-open`. Without a question it is
a usage error (exit 2); it does not start a chat.

Claude only for now *(my call)*: it is the one agent ig already runs with restricted
tools, and the answer needs the `ig_show` MCP tool. Another `--agent` is refused with
"explore needs claude for now".

### Removed or changed

- `ig <dir>` is removed: `ig .` prints "use `ig app .`" and exits 2. It made the first word
  ambiguous (a folder named `plan`) and blocked suggestions for mistyped commands.
- `ig view --read` and `--agent` are removed: they print "the reading moved to `ig analyse`" and
  exit 2, for one release.
- `ig view --no-read` is kept hidden for one release *(my call)*: skills copied into repos by
  earlier `ig init` runs pass it. It does what `view` now always does, and prints a one-line notice
  on stderr ("--no-read is the default now; run `ig init` to update the skills"), so it is not
  silently ignored.
- `ig plan` no longer writes `ig/`.
- `ig hook` keeps its "was removed" message, hidden from help.
- The terminal prompt "Let an AI agent read the architecture? [Y/n]" moves from `view` to
  `analyse`: plain `ig analyse` in a terminal asks before running the agent; `--agent` or `ig.yaml`'s
  `agent:` skips the question; without a terminal and without either, it runs `--no-agent` with a
  notice. *(my call)*
- *(my call)* The first `ig analyse` (no `ig/`) has the agent read every box, as `view --read`
  does today: the person asked for an analysis. A "no" at the prompt writes the code-only `ig/`, as
  `--no-agent` does. The "declined" memory of today's first-run offer is dropped: the question is
  asked only when `analyse` runs.
- *(my call)* `ig analyse` on an existing `ig/` with no agent to run (`--no-agent`, a "no", no
  terminal and no agent configured, or none installed) writes nothing, says so in a notice
  ("ig/ exists; the reading needs an agent: nothing to propose"), and exits 0, so scripts can run
  it twice.

### Folder of projects

`ig analyse` on a folder of projects reads each project without an `ig/` (today's
`view --read` there). `ig view` on it draws the landscape, as today, without reading.

### MCP

`ig_plan` and `ig_show` never write `ig/`. With no `ig/`, `ig_plan` says "run `ig analyse`";
`ig_show` still draws *(my call)*, and accepts the box ids of the map the non-writing path builds
(the same ids `ig view --json` prints), so `ig explore` works before `ig analyse`. `ig_edit` is
unchanged. The `NO_SPEC` message changes accordingly.

## Conventions

- Short options group (`-qC dir`); long options take `--opt value` or `--opt=value`; `--` ends
  options, so `ig accept -- 'web->api'` works; options may come before or after the command.
- An option belongs to its command: given to another, it is a usage error.
- A usage error is one line saying what is wrong, the closest command or option when there is one,
  and the command's usage line; exit 2.
- stdout: results only (the page path, the plan, JSON, the explore answer, completions).
  stderr: notices, warnings, progress, errors.
- Nothing prompts without a terminal. `NO_COLOR` is honoured. `-q` drops notices and warnings,
  never errors or results.

## How it is built

- `clap` (derive) replaces the hand-written `lexopt` parser in `crates/ig/src/cli.rs`;
  `clap_complete` writes the completions. Both are already added on branch `cli-posix`, with
  `crates/ig/tests/cli.rs` describing the conventions above (to be updated for `analyse` and
  `explore`).
- `main.rs` matches on a typed command instead of strings.
- The map engine (`crates/ig-app/src/map/view.rs`, `run_live`) writes `ig/` in its "no `ig/` yet"
  branch. It gets a way to build the view without writing (the map of the code, as when `ig/` is
  broken). `view`, `plan`, `check`, MCP and the desktop app use it; only `analyse` writes.
- Tests that set up `ig/` with `ig plan` switch to `ig analyse --no-agent`.

### What else changes with it

- README: the quick start becomes journey 1; the command table follows the help above.
- Skills (`crates/ig/assets/skills/explore.md`, `view.md`): `ig view --json` instead of
  `ig view --json --no-read`; no `ig/` means "run `ig analyse`", which the skill may suggest but
  never runs itself.
- Viewer (`viewer/src/map/Map.svelte`): "Run `ig view --read`" becomes "Run `ig analyse`".
- Desktop app (`crates/ig-desktop/src/commands.rs`): it builds the view in-process; it moves to the
  non-writing path and its messages name `ig analyse`. No new feature.

## Left to the builder

- Help wording, the order of commands in help, the exact error texts beyond those quoted here.
- How `analyse` reports progress (today's bar is fine).
- The calls marked *(my call)* above were made by the designer, not the user; the user checks them
  when approving this document.
