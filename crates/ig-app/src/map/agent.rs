//! Stage 2 of the map: an AI agent reads ig's facts (stage 1) into annotations, and the compiler
//! (stage 3: `annotate`, `validate`, then `upgrade` here) checks what it wrote. Running the agent
//! is I/O, so the caller passes it in (`Ask::run`); the rest is here.

use super::file::{self, Confidence, Field, MapBox, MapFile, Source};
use super::{BoxKind, FactBox};
use ig_core::config::Config;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

pub const NO_AGENT: &str = "No AI agent found (claude, gemini or codex): showing ig's own reading.";
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

    /// The command line, program first. The prompt goes on stdin (it can outgrow argv).
    pub fn argv(self) -> &'static [&'static str] {
        match self {
            Agent::Claude => &["claude", "-p", "--output-format", "text"],
            // gemini appends `-p` to what it reads on stdin
            Agent::Gemini => &["gemini", "-p", "Answer as the input above asks."],
            Agent::Codex => &["codex", "exec", "-"],
        }
    }

    /// `flag`, else the config's `agent`, else the first of `ALL` that `found` (on PATH).
    pub fn pick(cfg: &Config, flag: Option<Agent>, found: &dyn Fn(&str) -> bool) -> Option<Agent> {
        flag.or_else(|| cfg.agent.as_deref().and_then(Agent::parse))
            .or_else(|| Agent::ALL.into_iter().find(|a| found(a.name())))
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
    /// May this agent read the architecture?
    pub confirm: &'a mut dyn FnMut(Agent) -> bool,
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

/// `ig view`'s reading: when some box needs it (or `--read`), picks the agent, asks to confirm
/// and reads. Returns a notice: no agent, or why the reading was not used.
pub fn offer(
    a: &mut Ask,
    m: &mut MapFile,
    ev: &Evidence,
    cfg: &Config,
    progress: &dyn Fn(&str),
) -> Option<String> {
    if !a.all && !m.boxes.iter().any(|b| b.needs_reading && !b.stale) {
        return None;
    }
    let Some(agent) = Agent::pick(cfg, a.flag, a.found) else {
        return Some(NO_AGENT.into());
    };
    if !(a.confirm)(agent) {
        return None;
    }
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
    progress(&format!("Asking {name} to read {n} boxes…"));
    let mut ask = base.clone();
    let mut errs = vec![];
    for _ in 0..2 {
        let reply = run(agent, &ask)?;
        progress(&format!("{name} answered, checking…"));
        match apply(m, &reply, ev) {
            Ok(k) => {
                progress(&format!("Read {k} boxes."));
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
    (template.replace("{{boxes}}", &text), boxes.len())
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

/// An agent's field becomes `extracted` when ig's facts prove it: a tech or logo naming a
/// package the box's files import, a kind ig's folders gave too.
pub fn upgrade(m: &mut MapFile, ev: &Evidence) {
    fn prove<T>(f: &mut Field<T>, proved: impl Fn(&T) -> bool) {
        if f.source == Source::Agent && proved(&f.value) {
            f.confidence = Some(Confidence::Extracted);
        }
    }
    for b in m.boxes.iter_mut() {
        let pkgs: Vec<String> = ev.packages(b).into_iter().filter_map(stem).collect();
        let names = |s: &String| {
            let s = alnum(s);
            pkgs.iter().any(|p| s.contains(p.as_str()))
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
