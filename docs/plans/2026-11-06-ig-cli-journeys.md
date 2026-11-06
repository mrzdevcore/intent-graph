# The ig command line around its journeys: implementation plan

**Goal:** A POSIX command line where `analyse` is the one command that writes `ig/` from the code, `view` only views, and `explore "…"` answers a question on the map.

**Architecture:** `clap` (derive) replaces the `lexopt` parser in `crates/ig/src/cli.rs`. The map engine (`crates/ig-app/src/map/view.rs`) splits into a read-only build (`run`, `run_live`) and a writing one (`analyse`). `explore` is a new runner in the `ig` crate that starts the Claude CLI with only ig's read-only MCP tools.

**Tech Stack:** Rust 2024 workspace, `clap` 4.6 (`derive`, `wrap_help`), `clap_complete` 4.6, existing `ig-app`, `ig-wire`, `serde_json`.

**Spec:** `docs/specs/2026-11-06-ig-cli-journeys-design.md` (read it first; this plan argues from it).

## Global Constraints

- Branch `cli-posix`. `clap` and `clap_complete` are already in `crates/ig/Cargo.toml`; `crates/ig/tests/cli.rs` is a draft from before the design (Task 1 rewrites it).
- Exit status: 0 ok, 1 `check` or `fmt --check` found something, 2 usage error or failure.
- stdout carries results only (page path, plan, JSON, explore answer, completions); notices, warnings, progress and errors go to stderr.
- No `.rs` file over 600 lines (`crates/ig/tests/architecture.rs` enforces it).
- Code comments: only a one-line non-obvious "why"; no doc comments restating code, also in tests.
- Commits: no `Co-Authored-By` or "Generated with" lines. Never push.
- Desktop app, connectors and landscape are frozen: change them only where this plan says.
- The `ig/` files under the repo root are the user's uncommitted work: never stage them (`git add` named paths only).

## Review Focus

1. `ig analyse` run twice, the second time with no agent: exit 0, `ig/` byte-identical, notice on stderr. (Test in Task 2.)
2. `ig analyse` on a broken `ig/`: nothing written, the file and line on stderr, exit 2. (Task 2.)
3. `ig view` in a folder with no code and no `ig/`: a page is written, no `ig/` appears. (Task 2.)
4. `ig accept -- 'web->api'` and `ig -C "dir with space" plan`: `--` and paths with spaces parse. (Task 1.)
5. `ig explore` with no `claude` on `PATH`: exit 2, a message naming `claude`, nothing under `.ig/` but what was there. (Task 4.)

---

### Task 1: POSIX parser, today's behaviour kept

Replace the parser; every command keeps today's effect except: `ig <dir>` is removed, `plan`/`check` gain `--json`, `-q` exists, `completions` exists.

**Files:**
- Modify: `crates/ig/src/cli.rs` (rewrite), `crates/ig/src/main.rs` (dispatch on the enum)
- Test: `crates/ig/tests/cli.rs` (rewrite), `crates/ig/tests/app.rs:56` (drop `vec!["."]`, expect 2 lines), `crates/ig/tests/agent.rs` (`read_and_no_read_together_is_a_usage_error`: still exit 2, update the expected text)

**Interfaces:**
- Produces in `cli.rs`:
  - `pub struct Cli { pub root: Option<PathBuf>, pub quiet: bool, pub command: Option<Cmd> }` with `#[arg(short = 'C', long = "root", global = true, value_name = "DIR")] root`, `#[arg(short, long, global = true)] quiet`, `#[command(name = "ig", version = model::TOOL_VERSION)]` so `-V`/`--version` print `ig <TOOL_VERSION>`.
  - `pub enum Cmd { Init { agents: Option<InitAgents> }, View(ViewArgs), Plan { json: bool }, Check { json: bool }, Accept { all: bool, ids: Vec<String> }, Fmt { check: bool }, App { dir: Option<PathBuf> }, Mcp, Plugin(PluginCmd), Completions { shell: clap_complete::Shell }, #[command(hide = true)] Hook }`
  - `pub enum AgentName { Claude, Gemini, Codex, Ollama }` (`ValueEnum`), `fn as_str(self) -> &'static str`.
  - `pub struct ViewArgs { pub no_open: bool, pub json: bool, pub read: bool, pub no_read: bool, pub agent: Option<AgentName> }` with `read` `conflicts_with = "no_read"` (moved in Task 2).
  - `pub fn parse() -> Cli` (wraps `Cli::parse()`); `Accept` uses `required_unless_present = "all"`, `conflicts_with = "all"` on `ids`.
  - `mcp`, `plugin`, `completions` under `help_heading = "Plumbing"` (clap `next_help_heading` / `subcommand_help_heading`).
- `main.rs`: `removed()` goes; `Cmd::Hook` prints today's text, exit 0. No command → print the usage line to stderr, exit 2. `-q` drops every `eprintln!("ig: {n}")` / `warning:` line (one helper `fn say(quiet: bool, line: &str)`).

- [ ] **Step 1: Rewrite `crates/ig/tests/cli.rs`.** Keep its helpers (`repo`, `ig_in`, `ig`). Keep these tests from the draft unchanged: `each_command_has_its_own_help`, `a_flag_of_another_command_is_an_error`, `a_mistyped_command_gets_a_suggestion`, `an_unknown_agent_is_refused_before_anything_runs`, `read_and_no_read_conflict`, `no_command_prints_usage_and_fails`, `version_short_and_long`, `completions_for_each_shell`, `removed_hook_still_answers`, `a_bare_directory_points_to_ig_app`. Change:
  - `top_help_lists_commands_and_hides_nothing_needed` → also asserts `out.stdout.contains("Plumbing")` and `!out.stdout.contains("hook")`.
  - `dash_c_and_root_both_pick_the_folder`: set up with `ig -C <dir> plan` (Task 2 switches it to `analyse --no-agent`), then `ig check --root <dir>` exits 0; plus `ig -C "<tmp>/dir with space" plan` exits 0.
  - `plan_and_check_print_json`: set up with `plan`; asserts `v["build"]`, `v["move"]`, `v["drift"]`, `v["notRead"]` are arrays.
  - `quiet_silences_notices_not_results`: set up with `plan`, add `lib/c.ts`, `ig -q check` → code 1, stderr empty, stdout contains `drift`.
  - New `double_dash_ends_options`: after `plan`, `ig accept -- 'web->api'` exits 2 with stderr naming `web->api` (unknown id), not a usage error (`!stderr.contains("unexpected argument")`).
  - Drop `plan_without_ig_writes_nothing_and_points_to_init` (Task 2 owns it).
  - `a_bare_directory_points_to_ig_app`: stderr contains `use \`ig app .\``.
- [ ] **Step 2: Run** `cargo test -p intent-graph --test cli` — expect most to FAIL (help text, suggestions, `--json`, completions).
- [ ] **Step 3: Rewrite `cli.rs` and the dispatch in `main.rs`** per Interfaces. `ig <dir>`: no external subcommands; `parse()` uses `Cli::try_parse()`, and on an `ErrorKind::InvalidSubcommand` whose word (`err.get(ContextKind::InvalidSubcommand)`) is an existing directory prints `ig: use \`ig app <word>\`` and exits 2; every other error goes through `err.exit()` (clap exits 2 for usage errors, 0 for help and version). `plan`/`check --json` print the object `mcp::plan_text` builds; move that builder to a shared `fn plan_json(&origin::Plan) -> Value` in `main.rs` or a new `crates/ig/src/plan.rs`, used by both.
- [ ] **Step 4: Run** `cargo test -p intent-graph` — all PASS (incl. `app`, `agent`, `plan_check`, `edit_cli`).
- [ ] **Step 5: Commit** `git add crates/ig/Cargo.toml Cargo.lock crates/ig/src crates/ig/tests && git commit -m "The command line follows POSIX: help per command, misuse is an error, completions, -C, -q, --json for plan and check"`

### Task 2: `analyse` writes `ig/`; `view`, `plan`, `init` never do

**Files:**
- Modify: `crates/ig-app/src/map/view.rs` (split), `crates/ig-app/src/map/agent.rs` (`Ask`, `offer`, texts), `crates/ig-app/src/map/view/propose.rs` (test helper), `crates/ig/src/cli.rs`, `crates/ig/src/main.rs`
- Create: `crates/ig/src/analyse.rs` (the `analyse` command, incl. folder of projects; moves `read_project` out of `main.rs`)
- Test: `crates/ig-app/src/map/view/tests.rs`, `crates/ig-app/src/map/agent/tests.rs`, `crates/ig-app/src/map/ansible/tests.rs`, `crates/ig/src/tests/view.rs`, `crates/ig/tests/{cli,agent,plan_check,edit_cli,connectors,landscape,app,unreadable}.rs`

**Interfaces:**
- `ig_app::map::view`:
  - `pub fn run(env: &Env) -> Result<View>` — never writes. No `ig/`: the map of the code (today's `today(old)` result), `broken: false`, notice `"no ig/ yet: run \`ig analyse\`"`.
  - `pub fn run_live(env: &Env, connectors: &[&dyn Connector]) -> Result<View>` — same, plus connectors (the `ask` parameter goes).
  - `pub fn analyse(env: &Env, ask: Option<&mut Ask>) -> Result<View>` — the only writer. No `ig/`: `offer` (if `ask`), then `write_spec` (today's `Ok(None)` arm). With `ig/`: with consent `propose`; otherwise notice `NOTHING_TO_PROPOSE`. Broken `ig/`: writes nothing, `broken: true`.
  - `run_with` is removed. One private `fn build(env, mode: Mode, connectors) -> Result<View>` with `enum Mode<'a, 'b> { Look, Analyse(Option<&'b mut Ask<'a>>) }` holds the shared body.
- `ig_app::map::agent`:
  - `Ask` loses `all`.
  - `pub const NOTHING_TO_PROPOSE: &str = "ig/ exists; the reading needs an agent: nothing to propose";`
  - `pub fn consent(a: &mut Ask, cfg: &Config) -> Option<Agent>`: no question when `a.flag.is_some() || cfg.agent.is_some()`; else `a.confirm` (none → `None` with notice `"no terminal to ask: pass --agent or set agent: in ig.yaml"` returned by the caller); `Agent::pick` must find it on `PATH`.
  - `offer` reads every box (`read(.., all: true, ..)`) after `consent`; `DECLINED` and its file go; the hint says `run \`ig analyse\``.
- `ig` crate:
  - `Cmd::Analyse(AnalyseArgs)` with `#[command(visible_alias = "analyze")]`, `pub struct AnalyseArgs { pub agent: Option<AgentName>, #[arg(conflicts_with = "agent")] pub no_agent: bool }`.
  - `ViewArgs` becomes `{ no_open, json, #[arg(long, hide = true)] no_read, #[arg(long, hide = true)] read, #[arg(long, hide = true)] agent: Option<String> }`; `--read`/`--agent` → stderr `the reading moved to \`ig analyse\``, exit 2; `--no-read` → stderr `--no-read is the default now; run \`ig init\` to update the skills`, then views.
  - `analyse::run(root: &Path, args: &AnalyseArgs, quiet: bool) -> Result<u8>`; folder of projects: `analyse` each `ig_wire::unread_projects(root)`, then nothing else. When it wrote, it ends with the stderr line `next: ig view, then commit ig/`.
  - `init` stops writing `ig/` and ends with stderr `next: ig analyse`.
  - `plan` without `ig/`: stderr `ig: no ig/ yet: run \`ig analyse\``, exit 2 (as `check`, `fmt`, `accept`, whose texts change the same way).

- [ ] **Step 1: ig-app tests.** In `view/tests.rs` rename `the_first_run_writes_ig_and_later_runs_never_rewrite_it` → `analyse_writes_ig_once_and_run_never_writes`: `run` on a repo without `ig/` → `files.read("ig/system.kdl").is_none()` and a notice containing `run \`ig analyse\``; `analyse(env, None)` → `ig/system.kdl` exists; a second `analyse(env, None)` → files unchanged and notices contain `NOTHING_TO_PROPOSE`. Other tests in that file and `ansible/tests.rs` that relied on `run` writing call `analyse(.., None)` first. In `agent/tests.rs`: `offer_reads_only_on_consent` keeps its intent with `consent`; the `DECLINED` assertion (line ~198) goes; new `a_flag_or_config_agent_skips_the_question` (confirm closure panics if called).
- [ ] **Step 2: Run** `cargo test -p ig-app` — FAIL (no `analyse`, `consent`).
- [ ] **Step 3: Implement** the ig-app Interfaces.
- [ ] **Step 4: Run** `cargo test -p ig-app` — PASS.
- [ ] **Step 5: CLI tests.** In `tests/cli.rs`: add back `plan_without_ig_writes_nothing_and_points_to_analyse` (exit 2, stderr contains `ig analyse`, no `ig/`); switch set-ups from `plan` to `analyse --no-agent`; add `view_never_writes_ig` (empty dir, `view --no-open` exit 0, no `ig/`; Review Focus 3), `analyse_twice_without_agent_changes_nothing` (Review Focus 1), `analyse_on_broken_ig_writes_nothing` (write `ig/system.kdl` = `system "x" {`, exit 2, file unchanged, stderr names `system.kdl`; Review Focus 2), `view_read_points_to_analyse`, `view_no_read_still_views_with_a_notice`, `init_does_not_write_ig`, `analyze_is_analyse`. In `tests/app.rs` `init_on_an_empty_repo_writes_an_empty_system` → `init` then `analyse --no-agent`. In `tests/agent.rs` every `["view", "--read", ..]` → `["analyse", ..]`; `no_tty_never_reads` → no terminal and no `--agent`: `ig/` written code-only, stderr has the no-terminal notice. Elsewhere replace setup `plan` with `analyse --no-agent`.
- [ ] **Step 6: Run** `cargo test -p intent-graph` — FAIL on the new tests.
- [ ] **Step 7: Implement** the `ig` crate Interfaces (`analyse.rs`, `cli.rs`, `main.rs`, `init` call site).
- [ ] **Step 8: Run** `cargo test --workspace` — PASS (the desktop crate compiles: `view::run` keeps its signature).
- [ ] **Step 9: Commit** `git add crates/ig-app/src crates/ig/src crates/ig/tests && git commit -m "ig analyse is the one command that writes ig/; view, plan and init never do"`

### Task 3: MCP, skills, viewer and desktop name `ig analyse`

**Files:**
- Modify: `crates/ig/src/mcp.rs` (`NO_SPEC`, `ig_show` targets), `crates/ig/src/mcp/schema.rs:73,92`, `crates/ig/assets/skills/explore.md`, `crates/ig/assets/skills/view.md`, `viewer/src/map/Map.svelte:590`, `crates/ig-desktop/src/commands.rs` (no code change needed beyond texts if any), `crates/ig-app/src/map/agent.rs` / `view.rs` / `view/propose.rs` doc lines that name `ig view --read`
- Test: `crates/ig/src/mcp/tests.rs`, `crates/ig/tests/landscape.rs` (mcp part)

**Interfaces:**
- `NO_SPEC = "no ig/ yet: run \`ig analyse\` to write it"`.
- `ig_show` without `ig/`: `Targets::new(&head_graph(root)?, Some(&view::run(&w.env())?))` (no `has_spec` branch).
- Skills: `ig view --json` everywhere instead of `ig view --json --no-read`; `explore.md` drops "writes the spec `ig/` on its own the first time" and says: no `ig/` → tell the user to run `ig analyse`, never run it yourself. `view.md` step 1 runs `ig view`.
- Viewer copy: `Run <code>ig analyse</code> to let an AI agent read them.`

- [ ] **Step 1: Write failing tests** in `mcp/tests.rs`: `show_draws_box_ids_without_ig` (repo without `ig/`, `ig_show` with a box id from `view::run`'s `boxes[0].id` → Ok, and no `ig/` written); `plan_without_ig_points_to_analyse` (text contains `ig analyse`). Add to `crates/ig/src/tests/` or `skills.rs` tests: `no_skill_passes_no_read` (no skill body contains `--no-read`).
- [ ] **Step 2: Run** `cargo test -p intent-graph mcp skills` — FAIL.
- [ ] **Step 3: Implement** per Interfaces; edit the viewer string and the stale doc lines.
- [ ] **Step 4: Run** `cargo test --workspace` and `cd viewer && pnpm run test && pnpm run typecheck` — PASS.
- [ ] **Step 5: Commit** `git add crates/ig/src crates/ig/assets crates/ig-app/src viewer/src crates/ig/tests && git commit -m "MCP, skills and the viewer say ig analyse; ig_show draws before ig/ exists"`

### Task 4: `ig explore "<question>"`

**Files:**
- Create: `crates/ig/src/explore.rs`
- Modify: `crates/ig/src/cli.rs` (`Cmd::Explore`), `crates/ig/src/main.rs`, `crates/ig/src/agent.rs` (`run_argv` → `pub(crate)`; secret-read deny list reused, not copied)
- Test: `crates/ig/tests/explore.rs` (new, `#![cfg(unix)]`, fake `claude` on a temp `PATH` like `tests/agent.rs`)

**Interfaces:**
- `Cmd::Explore(ExploreArgs)`, `pub struct ExploreArgs { pub question: String, #[arg(long)] pub agent: Option<AgentName>, #[arg(long)] pub no_open: bool }` — `question` required (clap usage error, exit 2, when absent).
- `explore::run(root: &Path, args: &ExploreArgs) -> Result<u8>`:
  1. `--agent` other than `claude` → stderr `explore needs claude for now`, exit 2. `claude` not on `PATH` → `claude was not found on PATH`, exit 2 (Review Focus 5).
  2. Write `.ig/view.json` = `serde_json::to_string_pretty(&view::run_live(&env, &[])?)` (the box ids the agent greps; no shell tool is granted — this replaces the spec's `Bash(ig view --json)`, same data, no shell).
  3. Argv: `claude -p --output-format text --restricted --strict-mcp-config --mcp-config <json> --tools Read,Grep,Glob,mcp__ig__ig_state,mcp__ig__ig_show --disallowed-tools <the secret Read rules of Agent::Claude.argv()>`, where `<json>` = `{"mcpServers":{"ig":{"command":"<current_exe>","args":["mcp","-C","<root>"]}}}`. Never `ig_edit`, `ig_plan`, `Bash`, `--safe-mode` (it turns MCP off).
  4. Prompt (stdin) = `include_str!("../assets/explore-prompt.md")` with `{{question}}` replaced; the prompt says: ids are in `.ig/view.json` (`boxes[].id`, `arrows[]`), call `ig_show`, answer in at most 10 lines.
  5. stdout: the agent's answer. Then `render::write` the view and `render::open` unless `--no-open`; print nothing else on stdout.
- Create `crates/ig/assets/explore-prompt.md` (the prompt above, short).

- [ ] **Step 1: Write failing tests** in `tests/explore.rs`: `explore_answers_and_draws` (fake `claude` writes its argv to `bin/argv` and prints `The answer.`; `ig explore "how" --no-open` → exit 0, stdout starts with `The answer.`, `argv` contains `--strict-mcp-config` and `mcp__ig__ig_show` and not `ig_edit`, `Bash`, `--safe-mode`; `.ig/view.json` parses; no `ig/` written); `explore_needs_a_question` (exit 2); `explore_refuses_other_agents` (`--agent codex` → exit 2, stderr `explore needs claude for now`); `explore_without_claude_on_path` (empty `PATH` dir → exit 2, stderr contains `claude`).
- [ ] **Step 2: Run** `cargo test -p intent-graph --test explore` — FAIL.
- [ ] **Step 3: Implement** per Interfaces.
- [ ] **Step 4: Run** `cargo test --workspace` — PASS. Then by hand on this repo: `cargo run -p intent-graph -- explore "how does ig view build the map" --no-open` — an answer on stdout and a fresh `.ig/show.json`.
- [ ] **Step 5: Commit** `git add crates/ig/src crates/ig/assets crates/ig/tests && git commit -m "ig explore answers a question and draws it on the map"`

### Task 5: README and help copy

**Files:**
- Modify: `README.md` (Quick start = journey 1: `ig init`, `ig analyse`, `ig view`, `ig explore "…"`; the `ig view` section loses the reading, which moves under a new `ig analyse` section with "Reading and consent" updated: first run reads every box, "no" writes code-only, no agent on an existing `ig/` is a no-op; Commands table matches `ig --help`; `ig .` → `ig app .`), `docs/specs/design.md` command list if it names `plan` writing `ig/`.

- [ ] **Step 1:** Run `cargo run -q -p intent-graph -- --help` and each `<cmd> --help`; paste the top-level help into the README Commands section, check every flag in the README exists in the help and vice versa.
- [ ] **Step 2:** `grep -rn "view --read\|--no-read\|ig plan.*write\|ig \.\b" README.md docs/specs/design.md crates/ig/assets` — only the hidden `--no-read` compatibility note may remain.
- [ ] **Step 3:** `make test` — PASS.
- [ ] **Step 4: Commit** `git add README.md docs/specs/design.md && git commit -m "README: the three commands of journey 1 and the new command table"`
