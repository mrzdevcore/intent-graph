//! What `ig view` draws: the map with its code level, every layer's marks and the notices.

use super::agent::{Ask, Evidence, offer};
use super::code::{as_fact, code_boxes};
use super::file::{self, Field, MapBox, MapFile, refresh};
use super::kinds::{KindView, Kinds};
use super::layers::{Mark, file_layers, risks, rules, sort_marks};
use super::layout::layout;
pub use super::layout::{Frame, Placed, Region, RegionKind};
use super::origin::{compare, not_read};
use super::warn::warnings;
use super::{MapArrow, Pattern, arrows, facts, spec};
use crate::Env;
use globset::GlobSet;
use ig_core::config::Config;
use ig_core::model::{Anatomy, Graph, Part, Range};
use ig_core::ports::{Files, LanguageFrontend};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

mod proposals;
mod propose;
pub use proposals::Proposal;

#[derive(Debug, Clone, Serialize)]
pub struct View {
    pub version: u32,
    pub boxes: Vec<MapBox>,
    pub arrows: Vec<MapArrow>,
    pub marks: Vec<Mark>,
    pub placed: Placed,
    /// The code of each function, class, method or component box, and of a component's
    /// template, by box id.
    pub sources: BTreeMap<String, Snippet>,
    /// The list each part box of a component holds, by box id.
    pub parts: BTreeMap<String, Vec<Part>>,
    pub notices: Vec<String>,
    /// What reads but looks wrong in `ig/` (`warn::warnings`); never a reason to fail.
    pub warnings: Vec<String>,
    /// Every kind a box in `ig/` may be, the team's among them.
    pub kinds: Vec<KindView>,
    /// The journal's entries nobody kept yet.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub proposals: Vec<Proposal>,
    /// The files ig read (the graph's modules), for `unread`.
    #[serde(skip)]
    pub read: BTreeSet<String>,
    /// No spec could be read or written: the view is of the code alone.
    #[serde(skip)]
    pub broken: bool,
}

/// A piece of a file: `text` from line `line` on, `cut` lines left out at its end.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snippet {
    pub file: String,
    pub line: u32,
    pub text: String,
    #[serde(skip_serializing_if = "is_zero")]
    pub cut: u32,
    pub complexity: u32,
}

fn is_zero(n: &u32) -> bool {
    *n == 0
}

/// The most lines a snippet holds.
const SNIPPET: u32 = 400;

/// Each span's lines (box id, file, lines, complexity), read once per file; none for a file
/// that cannot be read.
fn snippets(
    files: &dyn Files,
    spans: &[(String, String, Range, u32)],
) -> BTreeMap<String, Snippet> {
    let mut read: HashMap<&str, Option<Vec<String>>> = HashMap::new();
    let mut out = BTreeMap::new();
    for (id, file, range, complexity) in spans {
        let lines = read.entry(file).or_insert_with(|| {
            files
                .read(file)
                .map(|t| t.lines().map(String::from).collect())
        });
        let Some(lines) = lines else { continue };
        let first = range.start_line.max(1);
        let last = range.end_line.max(first).min(lines.len() as u32);
        if first > last {
            continue;
        }
        let shown = last.min(first + SNIPPET - 1);
        out.insert(
            id.clone(),
            Snippet {
                file: file.clone(),
                line: first,
                text: lines[first as usize - 1..shown as usize].join("\n"),
                cut: last - shown,
                complexity: *complexity,
            },
        );
    }
    out
}

/// The UI components of `g`'s files that a frontend reads (`LanguageFrontend::anatomy`).
pub fn components(
    files: &dyn Files,
    g: &Graph,
    frontends: &[&dyn LanguageFrontend],
    config: &Config,
) -> Vec<Anatomy> {
    let mut out = vec![];
    for n in g.nodes.iter().filter(|n| n.is_module()) {
        let Some(f) = frontends.iter().find(|f| f.handles(&n.file, config)) else {
            continue;
        };
        if let Some(src) = files.read(&n.file) {
            out.extend(f.anatomy(&n.file, &src, config));
        }
    }
    out
}

/// `m` with its code level (`ui`: the components of `components`), the marks of `.ig/layers`
/// and where everything is drawn; the notices are those layers' and the layout's.
pub fn view(files: &dyn Files, m: &MapFile, g: &Graph, ui: &[Anatomy]) -> View {
    let code = code_boxes(m, g, ui);
    let sources = snippets(files, &code.spans);
    let parts = code.parts.into_iter().collect();
    let mut boxes: Vec<MapBox> = m.boxes.iter().cloned().chain(code.boxes).collect();
    let patterns: HashMap<String, Pattern> = code.patterns.into_iter().collect();
    for b in boxes.iter_mut().filter(|b| b.pattern.is_none()) {
        b.pattern = patterns.get(&b.id).cloned().map(Field::ig);
    }
    boxes.sort_by(|a, b| a.id.cmp(&b.id));
    let parents: HashMap<&str, &str> = (boxes.iter())
        .filter_map(|b| Some((b.id.as_str(), b.parent.as_deref()?)))
        .collect();
    let arrows = arrows::lift(
        &parents,
        m.arrows.iter().cloned().chain(code.arrows).collect(),
    );
    let known: HashSet<&str> = boxes
        .iter()
        .map(|b| b.id.as_str())
        .chain(arrows.iter().map(|a| a.id.as_str()))
        .collect();
    let (extra, mut notices) = file_layers(files, &known);
    let mut marks: Vec<Mark> = m.marks.iter().cloned().chain(extra).collect();
    sort_marks(&mut marks);
    let (placed, mut unrouted) = layout(&boxes, &arrows);
    notices.append(&mut unrouted);
    View {
        version: m.version,
        boxes,
        arrows,
        marks,
        placed,
        sources,
        parts,
        notices,
        warnings: vec![],
        // a team kind file that does not read is reported by the spec's read
        kinds: Kinds::load(files)
            .unwrap_or_else(|_| Kinds::builtin().clone())
            .views(),
        proposals: vec![],
        read: (g.nodes.iter().filter(|n| n.is_module()))
            .map(|n| n.file.clone())
            .collect(),
        broken: false,
    }
}

/// `ig view` without an agent: see `run_with`.
pub fn run(env: &Env) -> crate::Result<View> {
    run_with(env, None)
}

/// `ig view`: the spec in `ig/` compared with the code, then the view of it. The first run (no
/// `ig/`) writes the spec from today's map: `.ig/map.json`'s reading when there is one, else the
/// code, read by an AI agent when `ask` consents; after that `ig/` changes only by edits (with
/// `--read`, the AI reading's proposals: `propose`), and `.ig/map.json` is never written. A broken `ig/` is left as is: the view is then of the code.
pub fn run_with(env: &Env, ask: Option<&mut Ask>) -> crate::Result<View> {
    let config = crate::config::load(env.files)?;
    let built = crate::graph(env, env.files, &config, env.vcs.and_then(|v| v.head()))?;
    // no code ig reads: an Ansible tree's roles, when it has any
    let infra = (!built.graph.nodes.iter().any(|n| n.is_module()))
        .then(|| super::ansible::read(env.files))
        .flatten();
    let (g, ws) = (
        infra.as_ref().map_or(&built.graph, |a| &a.graph),
        &built.workspace,
    );
    let boxes = match &infra {
        Some(a) => a.boxes.clone(),
        None => facts(&built.workspace, g, &config),
    };
    let findings = ig_core::rules::arch(g, &config, &ws.packages);
    let mut notices: Vec<String> = config
        .notices
        .iter()
        .map(|k| format!("ig.yaml: `{k}` is no longer read; ignored"))
        .collect();
    // today's map of the code, refreshed over `old`
    let today = |old: Option<&MapFile>| {
        let (mut m, reset) = refresh(old, &boxes, arrows(&boxes, g), &findings);
        relift(&mut m, g, &findings);
        (m, reset)
    };
    let mut broken = false;
    let prior = prior(env);
    let read = match spec::read(env.files) {
        Ok(Some(s)) => match ask.filter(|a| a.all) {
            // `--read`: the reading's edits are in ig/ now, so it is read again
            Some(a) => {
                let ev = Evidence::new(&boxes, &built.extras.imports, &config).for_spec();
                let cm = compare(&s, &boxes, g, prior.as_ref());
                notices.extend(propose::propose(
                    a,
                    &cm,
                    &ev,
                    &config,
                    env.files,
                    env.progress,
                ));
                Ok(spec::read(env.files).ok().flatten().unwrap_or(s))
            }
            None => Ok(s),
        },
        Err(e) => {
            broken = true;
            notices.extend(e);
            notices.push(format!("{}/ left as is; fix it", spec::DIR));
            Err(today(None).0)
        }
        Ok(None) => {
            let (old, rejected) = file::load(env.files);
            let (mut m, reset) = today(old.as_ref());
            notices.extend(reset);
            if rejected.is_empty() {
                if let Some(a) = ask {
                    let ev = Evidence::new(&boxes, &built.extras.imports, &config);
                    notices.extend(offer(a, &mut m, &ev, &config, env.files, env.progress));
                    relift(&mut m, g, &findings);
                }
                let from = if old.is_some() { file::MAP } else { "the code" };
                let (said, wrote) = write_spec(env.files, &m, from)?;
                notices.push(said);
                broken |= !wrote;
                match spec::read(env.files) {
                    Ok(Some(s)) => Ok(s),
                    Ok(None) => {
                        broken = true;
                        Err(m)
                    }
                    Err(e) => {
                        broken = true;
                        notices.extend(e);
                        Err(m)
                    }
                }
            } else {
                broken = true;
                notices.extend(rejected);
                notices.push(format!(
                    "{} left as is; fix or delete it to let ig write {}/",
                    file::MAP,
                    spec::DIR
                ));
                Err(m)
            }
        }
    };
    let mut m = match read {
        Ok(s) => {
            let mut m = compare(&s, &boxes, g, prior.as_ref());
            marks(&mut m, &findings);
            m
        }
        Err(m) => m,
    };
    // what is not read now is read only when ig/ is written: no "ig's guess" banner
    m.boxes.iter_mut().for_each(|b| b.needs_reading = false);
    let ui = components(env.files, g, env.frontends, &config);
    let mut v = view(env.files, &m, g, &ui);
    notices.append(&mut v.notices);
    if !broken {
        // a team kind file that does not read is already reported by the read
        let kinds = Kinds::load(env.files).unwrap_or_else(|_| Kinds::builtin().clone());
        v.warnings = warnings(&m, &kinds);
    }
    v.notices = notices;
    v.broken = broken;
    let (entries, notice) = super::edit::journal::entries(env.files);
    v.notices.extend(notice);
    v.proposals = proposals::proposals(&entries, &v.boxes);
    Ok(v)
}

/// The spec at HEAD: the `ig/*.kdl` committed there, read in one batch; None without a commit
/// or when it does not read.
fn prior(env: &Env) -> Option<MapFile> {
    let vcs = env.vcs?;
    let head = vcs.head()?;
    let texts: Vec<(String, Option<String>)> = (vcs.show_dir(&head, spec::DIR).into_iter())
        .filter(|(p, _)| p.ends_with(".kdl"))
        .map(|(p, t)| (p, Some(t)))
        .collect();
    if texts.is_empty() {
        return None;
    }
    spec::parse(texts).ok()
}

/// The code ig does not read (`origin::not_read`'s globs): the files git lists (none without
/// git) that a frontend handles and `v` did not read, outside ig.yaml's `ignore`, folders whose
/// name starts with `.` or is in `skip` (build output) and the repo root's own files.
pub fn unread(env: &Env, v: &View, skip: &[&str]) -> crate::Result<Vec<String>> {
    let Some(vcs) = env.vcs else {
        return Ok(vec![]);
    };
    let config = crate::config::load(env.files)?;
    let ignore = ig_core::workspace::globset(&config.ignore).unwrap_or_else(|_| GlobSet::empty());
    // a file, or a folder above it, ignored
    let ignored = |f: &str| {
        std::iter::successors(Some(f), |p| p.rsplit_once('/').map(|(d, _)| d))
            .any(|p| ignore.is_match(p))
    };
    let skipped = |f: &str| {
        let dirs = f.rsplit_once('/').map_or("", |(d, _)| d);
        dirs.is_empty() || (dirs.split('/')).any(|d| d.starts_with('.') || skip.contains(&d))
    };
    let listed = vcs.list_files();
    let left: Vec<&str> = (listed.iter().map(String::as_str))
        .filter(|f| !v.read.contains(*f) && !skipped(f) && !ignored(f))
        .filter(|f| env.frontends.iter().any(|fe| fe.handles(f, &config)))
        .collect();
    let read: BTreeSet<&str> = v.read.iter().map(String::as_str).collect();
    Ok(not_read(&read, &left))
}

/// `m`'s arrows lifted over its own boxes, and the marks that read them: an agent's or a
/// person's components are not ig's facts, so arrows lifted over the facts would end nowhere.
fn relift(m: &mut MapFile, g: &Graph, findings: &[ig_core::model::Finding]) {
    let live: Vec<_> = m.boxes.iter().filter(|b| !b.stale).map(as_fact).collect();
    m.arrows = arrows(&live, g);
    marks(m, findings);
}

/// `m`'s marks: the `rules` (with `findings`) and `risks` layers.
fn marks(m: &mut MapFile, findings: &[ig_core::model::Finding]) {
    m.marks = rules(m, findings);
    m.marks.extend(risks(m));
    sort_marks(&mut m.marks);
}

/// Writes `m` as the spec in `ig/` unless `validate` or the reader would reject it (nothing
/// is then written); the notice, and whether it was written (`from`: where its reading came
/// from).
fn write_spec(files: &dyn Files, m: &MapFile, from: &str) -> crate::Result<(String, bool)> {
    let not = |why: &str, e: Vec<String>| {
        let e = e.join("; ");
        Ok((format!("{}/ not written, {why}: {e}", spec::DIR), false))
    };
    if let Err(e) = file::validate(m) {
        return not("ig's map is not valid", e);
    }
    let mut out: Vec<(String, String)> = spec::write(m).into_iter().collect();
    let texts = out.iter().map(|(p, t)| (p.clone(), Some(t.clone())));
    if let Err(e) = spec::parse(texts.collect()) {
        return not("ig would not read it back", e);
    }
    // system.kdl last: an `ig/` without it is a broken spec, never a partial one read as whole
    out.sort_by_key(|(p, _)| p.ends_with("/system.kdl"));
    for (path, text) in out {
        files.write_atomic(&path, text.as_bytes())?;
    }
    Ok((
        format!(
            "wrote {}/: the intended architecture, from {from}; edit it, commit it",
            spec::DIR
        ),
        true,
    ))
}

#[cfg(test)]
mod tests;
