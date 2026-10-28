//! Stage 2 of the map: an AI agent reads ig's facts (stage 1) into annotations, and the compiler
//! (stage 3: `annotate`, `validate`, then `upgrade` here) checks what it wrote. Running the agent
//! is I/O, so the caller passes it in (`Ask::run`); the rest is here.

use super::file::{self, Confidence, Field, MapBox, MapFile, Source};
use super::{BoxKind, FactBox};
use ig_core::config::Config;
use ig_core::ports::Files;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

pub const NO_AGENT: &str = "No AI agent found (claude, gemini or codex): showing ig's own reading.";
/// Said before claude runs.
pub const READ_ONLY: &str = "The agent reads files in this repo with read-only tools.";
/// The unread boxes a person said no to (a JSON array of ids), relative to the ig root.
pub const DECLINED: &str = ".ig/declined.json";
const OPEN: &str = "<ig-annotations>";
const CLOSE: &str = "</ig-annotations>";
/// File names shown per box.
const SAMPLE: usize = 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Agent {
    Claude,
    Gemini,
    Codex,
}

impl Agent {
    /// In the order `pick` looks for them.
    pub const ALL: [Agent; 3] = [Agent::Claude, Agent::Gemini, Agent::Codex];

    pub fn name(self) -> &'static str {
        match self {
            Agent::Claude => "claude",
            Agent::Gemini => "gemini",
            Agent::Codex => "codex",
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
            // --safe-mode drops CLAUDE.md, skills, hooks and MCP servers; --tools is the whole set
            Agent::Claude => &[
                "claude",
                "-p",
                "--output-format",
                "text",
                "--restricted",
                "--safe-mode",
                "--strict-mcp-config",
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
        }
    }

    /// `flag`, else the config's `agent`, else claude; None when that one is not `found` (on
    /// PATH). Gemini and codex run only when named (ruling R30).
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
            a => format!(
                "Using {}: its read-only mode is not verified by ig.",
                a.name()
            ),
        }
    }
}

/// What `ig view` needs from the outside to ask an agent.
pub struct Ask<'a> {
    /// `--read`: every box, even the annotated ones, and no question.
    pub all: bool,
    /// `--agent`.
    pub flag: Option<Agent>,
    /// The prompt (`read.md`); `{{boxes}}` is replaced by the boxes to read.
    pub template: &'a str,
    /// The program is on PATH.
    pub found: &'a dyn Fn(&str) -> bool,
    /// May this agent read the architecture? None when nobody can be asked (no terminal):
    /// then only `all` reads.
    pub confirm: Option<&'a mut dyn FnMut(Agent) -> bool>,
    /// Runs the agent with a prompt; its answer, or why it failed.
    pub run: &'a mut dyn FnMut(Agent, &str) -> Result<String, String>,
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
        }
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

/// `ig view`'s reading, only on consent: `--read`, or a yes to `confirm` when some box needs
/// reading. A no is kept in `DECLINED`: the same unread boxes are not asked about again. Returns
/// a notice: the hint when nobody can be asked (no agent to ask about, or a no kept), no agent
/// for `--read`, or why the reading was not used.
pub fn offer(
    a: &mut Ask,
    m: &mut MapFile,
    ev: &Evidence,
    cfg: &Config,
    files: &dyn Files,
    progress: &dyn Fn(&str),
) -> Option<String> {
    let unread: Vec<&str> = m
        .boxes
        .iter()
        .filter(|b| b.needs_reading && !b.stale)
        .map(|b| b.id.as_str())
        .collect();
    let n = unread.len();
    let hint = || {
        let are = if n == 1 { "is" } else { "are" };
        Some(format!(
            "{} {are} ig's guess; run `ig view --read` to let an AI agent read them.",
            boxes(n)
        ))
    };
    if !a.all && n == 0 {
        return None;
    }
    let agent = Agent::pick(cfg, a.flag, a.found);
    if !a.all {
        let (Some(ag), Some(confirm)) = (agent, a.confirm.as_mut()) else {
            return hint();
        };
        let declined = files
            .read_bytes(DECLINED)
            .and_then(|b| serde_json::from_slice::<Vec<String>>(&b).ok());
        if declined.is_some_and(|d| d == unread) {
            return hint();
        }
        if !confirm(ag) {
            let text = serde_json::to_string(&unread).unwrap_or_default();
            // ponytail: a failed write only means the question comes back next time
            let _ = files.write_atomic(DECLINED, text.as_bytes());
            return None;
        }
    }
    let Some(agent) = agent else {
        return Some(NO_AGENT.into());
    };
    read(agent, a.template, m, ev, a.all, a.run, progress).err()
}

/// Asks `agent` to read the boxes that need it (`all`: every box) and applies its annotations;
/// on any problem asks once more with the problems appended. Returns the count of boxes read,
/// or why nothing was (the map is then untouched).
pub fn read(
    agent: Agent,
    template: &str,
    m: &mut MapFile,
    ev: &Evidence,
    all: bool,
    run: &mut dyn FnMut(Agent, &str) -> Result<String, String>,
    progress: &dyn Fn(&str),
) -> Result<usize, String> {
    let (base, n) = prompt(template, m, ev, all);
    if n == 0 {
        return Ok(0);
    }
    let name = agent.name();
    progress(&format!("Asking {name} to read {}…", boxes(n)));
    progress(&agent.note());
    let mut ask = base.clone();
    let mut errs = vec![];
    for _ in 0..2 {
        let reply = run(agent, &ask)?;
        progress(&format!("{name} answered, checking…"));
        match apply(m, &reply, ev) {
            Ok(k) => {
                progress(&format!("Read {}.", boxes(k)));
                return Ok(k);
            }
            Err(e) => errs = e,
        }
        let list: Vec<String> = errs.iter().map(|e| format!("- {e}")).collect();
        ask = format!(
            "{base}\n## Your previous answer was rejected\n\n{}\n\nAnswer again, fixing every \
             problem above.\n",
            list.join("\n")
        );
    }
    Err(format!(
        "{name}'s reading was not used: {}",
        errs.join("; ")
    ))
}

/// The prompt, and the number of boxes in it.
fn prompt(template: &str, m: &MapFile, ev: &Evidence, all: bool) -> (String, usize) {
    let boxes: Vec<Value> = m
        .boxes
        .iter()
        .filter(|b| !b.stale && (all || b.needs_reading))
        .map(|b| fact_json(b, m, ev))
        .collect();
    let text = serde_json::to_string_pretty(&boxes).unwrap_or_default();
    // a fence longer than any run of backticks in the repo's names, so none can close it
    let longest = text.split(|c| c != '`').map(str::len).max().unwrap_or(0);
    let fence = "`".repeat(longest.max(2) + 1);
    let block = format!("{fence}json\n{text}\n{fence}");
    (template.replace("{{boxes}}", &block), boxes.len())
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
    file::annotate(m, json)?;
    upgrade(m, ev);
    let notes: Vec<Value> = serde_json::from_str(json).unwrap_or_default();
    let ids: BTreeSet<&str> = notes.iter().filter_map(|a| a["id"].as_str()).collect();
    Ok(ids.len())
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
