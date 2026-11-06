//! Stage 2 of the map: an AI agent reads ig's facts (stage 1) into annotations, and the compiler
//! (stage 3: `annotate`, `validate`, then `upgrade` here) checks what it wrote. Running the agent
//! is I/O, so the caller passes it in (`Ask::run`); the rest is here.

use super::file::{self, Confidence, Field, MapBox, MapFile, Source};
use super::{BoxKind, FactBox};
use ig_core::config::Config;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

pub const NO_AGENT: &str = "No AI agent found (claude, gemini or codex): showing ig's own reading.";
/// Said before claude runs.
pub const READ_ONLY: &str = "The agent reads files in this repo with read-only tools.";
pub const NO_TERMINAL: &str = "no terminal to ask: pass --agent or set agent: in ig.yaml";
pub const NOTHING_TO_PROPOSE: &str = "ig/ exists; the reading needs an agent: nothing to propose";
const OPEN: &str = "<ig-annotations>";
const CLOSE: &str = "</ig-annotations>";
/// File names shown per box.
const SAMPLE: usize = 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Agent {
    Claude,
    Gemini,
    Codex,
    /// A small model on this machine, through a local Ollama server: no tools, it answers from
    /// the facts alone, a few boxes at a time (`batch`).
    Ollama,
}

impl Agent {
    /// In the order `pick` looks for them.
    pub const ALL: [Agent; 4] = [Agent::Claude, Agent::Gemini, Agent::Codex, Agent::Ollama];

    pub fn name(self) -> &'static str {
        match self {
            Agent::Claude => "claude",
            Agent::Gemini => "gemini",
            Agent::Codex => "codex",
            Agent::Ollama => "ollama",
        }
    }

    pub fn parse(name: &str) -> Option<Agent> {
        Agent::ALL.into_iter().find(|a| a.name() == name)
    }

    /// The command line, program first. The prompt goes on stdin (it can outgrow argv). The repo
    /// may be untrusted: each agent gets read-only tools and skips the repo's own agent config
    /// where its CLI allows.
    pub fn argv(self) -> &'static [&'static str] {
        match self {
            // checked against `claude --help`: --restricted drops the code-running tools and
            // ignores user/project/local settings (so the repo's hooks and allow rules);
            // --safe-mode drops CLAUDE.md, skills, hooks and MCP servers; --tools is the whole set.
            // --disallowed-tools keeps it from secrets (tried with fake ones: Read and Grep both
            // come back without them); `ig::agent::is_secret` is the same list for the agents
            // that have no such flag, but for `.example` files, harmless to keep from Claude
            Agent::Claude => &[
                "claude",
                "-p",
                "--output-format",
                "text",
                "--restricted",
                "--safe-mode",
                "--strict-mcp-config",
                "--disallowed-tools",
                "Read(**/*.tfstate*)",
                "Read(**/*.tfvars)",
                "Read(**/*.tfvars.json)",
                "Read(**/.env*)",
                "Read(**/*.env)",
                "Read(**/*.pem)",
                "Read(**/*.key)",
                "Read(**/id_rsa*)",
                "Read(**/id_ed25519*)",
                "Read(**/id_ecdsa*)",
                "--tools",
                "Read,Grep,Glob",
            ],
            // ponytail: from gemini-cli's docs, not installed here to check: `default` approval
            // leaves out the tools that need approval (shell, edits) when non-interactive,
            // `-e none` loads no extensions; GEMINI.md is still read (no flag skips it).
            // gemini appends `-p` to what it reads on stdin.
            Agent::Gemini => &[
                "gemini",
                "--approval-mode",
                "default",
                "--extensions",
                "none",
                "-p",
                "Answer as the input above asks.",
            ],
            // ponytail: from codex's docs, not installed here to check: a read-only sandbox, no
            // approvals asked, AGENTS.md not loaded (project_doc_max_bytes=0); `-` reads stdin
            Agent::Codex => &[
                "codex",
                "exec",
                "--sandbox",
                "read-only",
                "-c",
                "approval_policy=\"never\"",
                "-c",
                "project_doc_max_bytes=0",
                "-",
            ],
            // not run as a command: ig posts the prompt to the server (`ig::agent::run`)
            Agent::Ollama => &["ollama"],
        }
    }

    /// The most boxes asked about at once: a small model loses the thread in a long prompt, and
    /// smaller asks let `ig analyse` show how far it got.
    // ponytail: 20 keeps most containers in one ask (a regrouping needs all its boxes); tune
    // if bigger containers get cut too often
    pub fn batch(self) -> usize {
        match self {
            Agent::Ollama => 12,
            _ => 20,
        }
    }

    /// `flag`, else the config's `agent`, else claude; None when that one is not `found` (on
    /// PATH). Gemini, codex and ollama run only when named (ruling R30).
    pub fn pick(cfg: &Config, flag: Option<Agent>, found: &dyn Fn(&str) -> bool) -> Option<Agent> {
        let a = flag
            .or_else(|| cfg.agent.as_deref().and_then(Agent::parse))
            .unwrap_or(Agent::Claude);
        found(a.name()).then_some(a)
    }

    /// Said before it runs: only claude's read-only mode is checked by ig.
    pub fn note(self) -> String {
        match self {
            Agent::Claude => READ_ONLY.into(),
            Agent::Ollama => {
                "Using a local model through ollama: it reads only ig's facts, no files.".into()
            }
            a => format!(
                "Using {}: its read-only mode is not verified by ig.",
                a.name()
            ),
        }
    }
}

/// Runs an agent with a prompt about some boxes (their ids): its answer, or why it failed.
pub type Run<'r> = dyn FnMut(Agent, &str, &[String]) -> Result<String, String> + 'r;

/// What `ig analyse` needs from the outside to ask an agent.
pub struct Ask<'a> {
    /// `--agent`.
    pub flag: Option<Agent>,
    /// The prompt (`read.md`); `{{boxes}}` is replaced by the boxes to read.
    pub template: &'a str,
    /// The program is on PATH.
    pub found: &'a dyn Fn(&str) -> bool,
    /// May this agent read the architecture? None when nobody can be asked (no terminal).
    pub confirm: Option<&'a mut dyn FnMut(Agent) -> bool>,
    /// Runs the agent with a prompt about the boxes `ids`; its answer, or why it failed.
    pub run: &'a mut Run<'a>,
    /// How far the reading got: boxes asked about so far (read or left out), of all to read, and
    /// what is happening now ("claude is reading…", "12 boxes read").
    pub meter: &'a dyn Fn(usize, usize, &str),
}

/// The facts the compiler checks an agent's fields against.
#[derive(Debug, Default)]
pub struct Evidence {
    /// Module → the outside packages it imports.
    by_file: BTreeMap<String, BTreeSet<String>>,
    /// Blueprint system node → its packages that some module imports.
    systems: BTreeMap<String, BTreeSet<String>>,
    /// Box → ig's kind for it, when ig's facts (folders, files) gave one.
    kinds: BTreeMap<String, BoxKind>,
    /// The map is read from the spec: its component ids need not start with their container's.
    spec: bool,
}

impl Evidence {
    pub fn new(facts: &[FactBox], imports: &BTreeSet<(String, String)>, cfg: &Config) -> Self {
        let mut by_file: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        for (f, p) in imports {
            by_file.entry(f.clone()).or_default().insert(p.clone());
        }
        let used: BTreeSet<&String> = imports.iter().map(|(_, p)| p).collect();
        let systems = cfg
            .blueprint
            .systems
            .iter()
            .map(|(name, pkgs)| {
                let used = pkgs.iter().filter(|p| used.contains(p)).cloned();
                (format!("system:{name}"), used.collect())
            })
            .collect();
        let kinds = facts
            .iter()
            .filter(|f| f.guess.kind != BoxKind::Group)
            .map(|f| (f.id.clone(), f.guess.kind))
            .collect();
        Evidence {
            by_file,
            systems,
            kinds,
            spec: false,
        }
    }

    /// For a map read from the spec (`file::annotate_spec` checks the reading).
    pub fn for_spec(self) -> Self {
        Evidence { spec: true, ..self }
    }

    /// The outside packages `b`'s files import (a blueprint system: its packages in use).
    fn packages(&self, b: &MapBox) -> BTreeSet<&str> {
        b.nodes
            .iter()
            .filter_map(|n| self.by_file.get(n).or_else(|| self.systems.get(n)))
            .flatten()
            .map(String::as_str)
            .collect()
    }
}

/// "1 box", "2 boxes".
fn boxes(n: usize) -> String {
    if n == 1 {
        "1 box".into()
    } else {
        format!("{n} boxes")
    }
}

/// The agent to read with: `--agent` or ig.yaml's `agent:` without a question, else a yes to
/// `confirm`; None when nobody said yes or the agent is not on PATH (`refusal` says why).
pub fn consent(a: &mut Ask, cfg: &Config) -> Option<Agent> {
    let agent = Agent::pick(cfg, a.flag, a.found)?;
    if a.flag.is_some() || cfg.agent.is_some() {
        return Some(agent);
    }
    let confirm = a.confirm.as_mut()?;
    confirm(agent).then_some(agent)
}

/// Why `consent` gave no agent; None after a no.
pub fn refusal(a: &Ask, cfg: &Config) -> Option<&'static str> {
    if Agent::pick(cfg, a.flag, a.found).is_none() {
        Some(NO_AGENT)
    } else if a.flag.is_none() && cfg.agent.is_none() && a.confirm.is_none() {
        Some(NO_TERMINAL)
    } else {
        None
    }
}

/// The first `ig analyse`'s reading of every box, on `consent`. Returns a notice: why nothing
/// was read (after a no, the hint), or why the reading was not used.
pub fn offer(
    a: &mut Ask,
    m: &mut MapFile,
    ev: &Evidence,
    cfg: &Config,
    progress: &dyn Fn(&str),
) -> Option<String> {
    let Some(agent) = consent(a, cfg) else {
        if let Some(why) = refusal(a, cfg) {
            return Some(why.into());
        }
        let n = (m.boxes.iter())
            .filter(|b| b.needs_reading && !b.stale)
            .count();
        let are = if n == 1 { "is" } else { "are" };
        return (n > 0).then(|| {
            format!(
                "{} {are} ig's guess; run `ig analyse` to let an AI agent read them.",
                boxes(n)
            )
        });
    };
    read(agent, a.template, m, ev, true, a.run, progress, a.meter).err()
}

/// Asks `agent` to read the boxes that need it (`all`: every box), `agent.batch()` at a time, and
/// applies its annotations; on any problem asks once more with the problems appended. Returns the
/// count of boxes read, or why nothing was (the map is then untouched). A batch that fails twice
/// is left out and said in `progress`. `meter` gets the boxes asked about so far, of the total,
/// and what is happening now: the steps of each ask go there, not to `progress`.
#[allow(clippy::too_many_arguments)]
pub fn read(
    agent: Agent,
    template: &str,
    m: &mut MapFile,
    ev: &Evidence,
    all: bool,
    run: &mut Run<'_>,
    progress: &dyn Fn(&str),
    meter: &dyn Fn(usize, usize, &str),
) -> Result<usize, String> {
    let ids: Vec<String> = (m.boxes.iter())
        .filter(|b| !b.stale && (all || b.needs_reading))
        .map(|b| b.id.clone())
        .collect();
    if ids.is_empty() {
        return Ok(0);
    }
    let name = agent.name();
    progress(&format!("Asking {name} to read {}…", boxes(ids.len())));
    progress(&agent.note());
    let batches = batches(&ids, agent.batch());
    let (mut read, mut failed, mut asked) = (0, vec![], 0);
    let total = ids.len();
    for batch in &batches {
        let step = |s: &str| meter(asked, total, s);
        // the agent could not be run (missing, down, timed out): the next batch would fail alike
        let done = read_batch(agent, template, m, ev, batch, run, &step)?;
        asked += batch.len();
        if let Ok(k) = done {
            read += k;
        }
        meter(asked, total, &format!("{} read", boxes(read)));
        match done {
            Ok(_) => {}
            Err(e) if batches.len() == 1 => return Err(e),
            Err(e) => {
                progress(&e);
                failed.push(e);
            }
        }
    }
    if read == 0 && !failed.is_empty() {
        return Err(failed.join("; "));
    }
    Ok(read)
}

/// `ids` cut into runs of at most `size`, a container's boxes kept together where they fit (a
/// regrouping with `madeOf` needs all of them).
fn batches(ids: &[String], size: usize) -> Vec<Vec<String>> {
    let mut by_root: BTreeMap<&str, Vec<String>> = BTreeMap::new();
    for id in ids {
        let root = id.split('/').next().unwrap_or(id);
        by_root.entry(root).or_default().push(id.clone());
    }
    let mut out: Vec<Vec<String>> = vec![];
    for group in by_root.into_values() {
        for part in group.chunks(size.max(1)) {
            match out.last_mut() {
                Some(last) if last.len() + part.len() <= size => last.extend_from_slice(part),
                _ => out.push(part.to_vec()),
            }
        }
    }
    out
}

/// Outer error: the agent could not be run; inner: its answers were rejected twice. `step` says
/// what is happening now.
fn read_batch(
    agent: Agent,
    template: &str,
    m: &mut MapFile,
    ev: &Evidence,
    ids: &[String],
    run: &mut Run<'_>,
    step: &dyn Fn(&str),
) -> Result<Result<usize, String>, String> {
    let name = agent.name();
    let base = prompt(template, m, ev, ids);
    let mut ask = base.clone();
    let mut errs = vec![];
    for again in ["", " again"] {
        step(&format!("{name} is reading{again}…"));
        let reply = run(agent, &ask, ids)?;
        step(&format!("checking {name}'s answer…"));
        match apply(m, &reply, ev) {
            Ok(k) => return Ok(Ok(k)),
            Err(e) => errs = e,
        }
        let list: Vec<String> = errs.iter().map(|e| format!("- {e}")).collect();
        ask = format!(
            "{base}\n## Your previous answer was rejected\n\n{}\n\nAnswer again, fixing every \
             problem above.\n",
            list.join("\n")
        );
    }
    Ok(Err(format!(
        "{name}'s reading was not used: {}",
        errs.join("; ")
    )))
}

/// The prompt for the boxes `ids`.
fn prompt(template: &str, m: &MapFile, ev: &Evidence, ids: &[String]) -> String {
    let boxes: Vec<Value> = (m.boxes.iter())
        .filter(|b| ids.contains(&b.id))
        .map(|b| fact_json(b, m, ev))
        .collect();
    let text = serde_json::to_string_pretty(&boxes).unwrap_or_default();
    // a fence longer than any run of backticks in the repo's names, so none can close it
    let longest = text.split(|c| c != '`').map(str::len).max().unwrap_or(0);
    let fence = "`".repeat(longest.max(2) + 1);
    let block = format!("{fence}json\n{text}\n{fence}");
    template.replace("{{boxes}}", &block)
}

fn fact_json(b: &MapBox, m: &MapFile, ev: &Evidence) -> Value {
    let count = |mine: fn(&super::MapArrow) -> &str, other: fn(&super::MapArrow) -> &str| {
        m.arrows
            .iter()
            .filter(|a| mine(a) == b.id)
            .map(|a| (other(a).to_string(), a.count))
            .collect::<BTreeMap<String, u32>>()
    };
    let sample: Vec<&String> = b
        .nodes
        .iter()
        .filter(|n| !n.starts_with("system:"))
        .collect();
    let mut v = json!({
        "id": b.id,
        "level": b.level,
        "parent": b.parent,
        "kind": b.kind.value,
        "title": b.title.value,
        "pattern": b.pattern.as_ref().map(|p| &p.value),
        "slot": b.slot.as_ref().map(|s| &s.value),
        "madeOf": b.made_of,
        "files": b.files,
        "sample": &sample[..sample.len().min(SAMPLE)],
        "importsFrom": count(|a| &a.from, |a| &a.to),
        "importedBy": count(|a| &a.to, |a| &a.from),
        "packages": ev.packages(b),
    });
    if let Value::Object(o) = &mut v {
        o.retain(|_, x| !x.is_null());
    }
    v
}

/// The block between the last `<ig-annotations>` and the `</ig-annotations>` after it.
fn extract(reply: &str) -> Option<&str> {
    let end = reply.rfind(CLOSE)?;
    let start = reply[..end].rfind(OPEN)? + OPEN.len();
    Some(reply[start..end].trim())
}

/// The reply's annotations applied and upgraded; the count of boxes annotated.
fn apply(m: &mut MapFile, reply: &str, ev: &Evidence) -> Result<usize, Vec<String>> {
    let json =
        extract(reply).ok_or_else(|| vec![format!("the answer has no {OPEN}…{CLOSE} block")])?;
    let json = &as_list(json);
    if ev.spec {
        file::annotate_spec(m, json)?;
    } else {
        file::annotate(m, json)?;
    }
    upgrade(m, ev);
    let notes: Vec<Value> = serde_json::from_str(json).unwrap_or_default();
    let ids: BTreeSet<&str> = notes.iter().filter_map(|a| a["id"].as_str()).collect();
    Ok(ids.len())
}

/// A small model often wraps the array: `{"annotations": [...]}` is its array, one annotation
/// `{"id": ...}` a list of one. Only the first JSON value counts: a model may write more after
/// it (gpt-oss through ollama did). Anything else is left for `annotate` to reject.
fn as_list(json: &str) -> String {
    match serde_json::Deserializer::from_str(json)
        .into_iter::<Value>()
        .next()
    {
        Some(Ok(Value::Object(o))) if o.contains_key("id") => {
            Value::Array(vec![Value::Object(o)]).to_string()
        }
        Some(Ok(Value::Object(o))) => match o.into_values().find(Value::is_array) {
            Some(a) => a.to_string(),
            None => json.to_string(),
        },
        Some(Ok(v)) => v.to_string(),
        _ => json.to_string(),
    }
}

/// An agent's field becomes `extracted` when ig's facts prove it: a tech or logo with a word that
/// is the name of a package the box's files import, a kind ig's folders gave too.
pub fn upgrade(m: &mut MapFile, ev: &Evidence) {
    fn prove<T>(f: &mut Field<T>, proved: impl Fn(&T) -> bool) {
        if f.source == Source::Agent && proved(&f.value) {
            f.confidence = Some(Confidence::Extracted);
        }
    }
    for b in m.boxes.iter_mut() {
        let pkgs: Vec<String> = ev.packages(b).into_iter().filter_map(stem).collect();
        let names = |s: &String| {
            s.split(|c: char| !c.is_ascii_alphanumeric())
                .any(|w| pkgs.iter().any(|p| p.eq_ignore_ascii_case(w)))
        };
        for f in [b.tech.as_mut(), b.logo.as_mut()].into_iter().flatten() {
            prove(f, names);
        }
        let kind = ev.kinds.get(&b.id).copied();
        prove(&mut b.kind, |k| Some(*k) == kind);
    }
}

/// The name a package goes by: its scope (`@prisma/client` → prisma), else its first word
/// (`socket.io-client` → socket). None under 3 letters.
// ponytail: a name heuristic; a package → product table if it misses real ones
fn stem(pkg: &str) -> Option<String> {
    let s = match pkg.strip_prefix('@') {
        Some(scoped) => scoped.split('/').next().unwrap_or(scoped),
        None => pkg.split(['-', '.', '/']).next().unwrap_or(pkg),
    };
    let s = alnum(s);
    (s.len() >= 3).then_some(s)
}

fn alnum(s: &str) -> String {
    s.chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

#[cfg(test)]
mod tests;
