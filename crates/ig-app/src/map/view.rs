//! What `ig view` draws: the map with its code level, every layer's marks and the notices.

use super::agent::{Ask, Evidence, offer};
use super::code::{as_fact, code_boxes};
use super::file::{self, Field, MapBox, MapFile, refresh};
use super::layers::{Mark, file_layers, risks, rules, sort_marks};
use super::layout::layout;
pub use super::layout::{Frame, Placed, Region, RegionKind};
use super::origin::compare;
use super::{MapArrow, Pattern, arrows, facts, spec};
use crate::Env;
use ig_core::config::Config;
use ig_core::model::{Anatomy, Graph, Part, Range};
use ig_core::ports::{Files, LanguageFrontend};
use serde::Serialize;
use std::collections::{BTreeMap, HashMap, HashSet};

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
    /// `ig/` could not be read: the view is of the code alone.
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
    let mut arrows: Vec<MapArrow> = m.arrows.iter().cloned().chain(code.arrows).collect();
    arrows.sort_by(|a, b| a.id.cmp(&b.id));
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
        broken: false,
    }
}

/// `ig view` without an agent: see `run_with`.
pub fn run(env: &Env) -> crate::Result<View> {
    run_with(env, None)
}

/// Said on `--read` when `ig/` already exists.
const LATER: &str =
    "the AI reading runs when ig/ is written; it will propose edits in a later version";

/// `ig view`: the spec in `ig/` compared with the code, then the view of it. The first run (no
/// `ig/`) writes the spec from today's map: `.ig/map.json`'s reading when there is one, else the
/// code, read by an AI agent when `ask` consents; `ig/` is never rewritten after that, and
/// `.ig/map.json` is never written. A broken `ig/` is left as is: the view is then of the code.
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
    let read = match spec::read(env.files) {
        Ok(Some(s)) => {
            if ask.is_some_and(|a| a.all) {
                notices.push(LATER.into());
            }
            Ok(s)
        }
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
                notices.push(write_spec(env.files, &m, from)?);
                match spec::read(env.files) {
                    Ok(Some(s)) => Ok(s),
                    Ok(None) => Err(m),
                    Err(e) => {
                        broken = true;
                        notices.extend(e);
                        Err(m)
                    }
                }
            } else {
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
            let mut m = compare(&s, &boxes, g);
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
    v.notices = notices;
    v.broken = broken;
    Ok(v)
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

/// Writes `m` as the spec in `ig/` unless `validate` rejects it; the notice either way (`from`:
/// where its reading came from).
fn write_spec(files: &dyn Files, m: &MapFile, from: &str) -> crate::Result<String> {
    if let Err(e) = file::validate(m) {
        return Ok(format!(
            "{}/ not written, ig's map is not valid: {}",
            spec::DIR,
            e.join("; ")
        ));
    }
    for (path, text) in spec::write(m) {
        files.write_atomic(&path, text.as_bytes())?;
    }
    Ok(format!(
        "wrote {}/: the intended architecture, from {from}; edit it, commit it",
        spec::DIR
    ))
}

#[cfg(test)]
mod tests {
    use super::super::origin::Origin;
    use super::*;
    use crate::testing::{Fake, env};
    use ig_core::testing::MemFiles;

    #[test]
    fn notices_reach_the_view_and_a_rejected_map_is_kept() {
        let files = MemFiles::with(&[
            ("ig.yaml", "complexityMax: 9\n"),
            ("src/a.ts", "export function f() {}\n"),
            (file::MAP, "{ not json"),
        ]);
        let fe = Fake;
        let v = run(&env(&files, None, &fe)).unwrap();
        assert!(v.notices[0].contains("complexityMax"), "{:?}", v.notices);
        assert!(
            v.notices.iter().any(|n| n.contains("left as is")),
            "{:?}",
            v.notices
        );
        assert_eq!(files.read(file::MAP).as_deref(), Some("{ not json"));
        assert!(
            files.read("ig/system.kdl").is_none(),
            "its reading is not lost"
        );
    }

    #[test]
    fn the_first_run_writes_ig_and_later_runs_never_rewrite_it() {
        let files = MemFiles::with(&[("src/a.ts", "export function f() {}\n")]);
        let v = run(&env(&files, None, &Fake)).unwrap();
        assert!(
            v.notices.iter().any(|n| n.starts_with("wrote ig/")),
            "{:?}",
            v.notices
        );
        // a person narrows the package to `src/`
        let pkg = files
            .read("ig/pkg.kdl")
            .unwrap()
            .replacen("at \"**\"", "at \"src/**\"", 1);
        files.write_atomic("ig/pkg.kdl", pkg.as_bytes()).unwrap();
        let before = files.read("ig/system.kdl").unwrap();
        files
            .write_atomic("lib/c.ts", b"export const y = 2;\n")
            .unwrap(); // outside every `at`: drift
        let v = run(&env(&files, None, &Fake)).unwrap();
        assert_eq!(files.read("ig/system.kdl").unwrap(), before);
        assert_eq!(files.read("ig/pkg.kdl").unwrap(), pkg);
        assert!(v.boxes.iter().any(|b| b.origin == Origin::Drift));
        assert!(!v.notices.iter().any(|n| n.starts_with("wrote ig/")));
        assert!(v.boxes.iter().all(|b| !b.needs_reading));
        assert!(
            files.read(file::MAP).is_none(),
            "the map file is never written"
        );
    }

    #[test]
    fn a_map_json_is_migrated_with_its_readings() {
        let files = MemFiles::with(&[("src/a.ts", "export function f() {}\n")]);
        let e = env(&files, None, &Fake);
        let cfg = crate::config::load(&files).unwrap();
        let built = crate::graph(&e, &files, &cfg, None).unwrap();
        let b = super::super::facts(&built.workspace, &built.graph, &cfg);
        let mut m = refresh(None, &b, arrows(&b, &built.graph), &[]).0;
        let sys = (m.boxes.iter_mut())
            .find(|b| b.level == super::super::Level::System)
            .unwrap();
        sys.title = Field::person("Core".into());
        sys.pinned.push("title".into());
        file::save(&files, &m).unwrap();
        let map = files.read(file::MAP).unwrap();
        let v = run(&e).unwrap();
        assert!(
            files
                .read("ig/system.kdl")
                .unwrap()
                .contains("title=\"Core\"")
        );
        assert!(
            v.notices.iter().any(|n| n.contains("from .ig/map.json")),
            "{:?}",
            v.notices
        );
        assert_eq!(files.read(file::MAP).unwrap(), map, "left where it is");
    }

    #[test]
    fn a_broken_spec_is_reported_and_kept() {
        let files = MemFiles::with(&[
            ("src/a.ts", "export function f() {}\n"),
            ("ig/system.kdl", "system {"),
        ]);
        let v = run(&env(&files, None, &Fake)).unwrap();
        assert!(
            v.notices.iter().any(|n| n.starts_with("ig/system.kdl:")),
            "{:?}",
            v.notices
        );
        assert!(v.notices.iter().any(|n| n == "ig/ left as is; fix it"));
        assert!(!v.boxes.is_empty(), "the code is still drawn");
        assert_eq!(files.read("ig/system.kdl").as_deref(), Some("system {"));
    }

    #[test]
    fn a_function_box_carries_its_code() {
        let (ws, g, cfg) = crate::map::shop::load();
        let b = super::super::facts(&ws, &g, &cfg);
        let m = refresh(None, &b, arrows(&b, &g), &[]).0;
        let f = g.nodes.iter().find(|n| n.kind == "function").unwrap();
        let (a, z) = (f.range.start_line, f.range.end_line);
        let src: String = (1..=z + 2).map(|i| format!("line {i}\n")).collect();
        let v = view(
            &MemFiles::with(&[(f.file.as_str(), src.as_str())]),
            &m,
            &g,
            &[],
        );
        let s = &v.sources[&f.id];
        assert_eq!((s.file.as_str(), s.line), (f.file.as_str(), a));
        let want: Vec<String> = (a..=z).map(|i| format!("line {i}")).collect();
        assert_eq!(s.text, want.join("\n"));
        assert!(
            v.sources.values().all(|s| s.file == f.file),
            "unreadable files have none"
        );
    }

    #[test]
    fn arrows_end_at_the_components_an_agent_drew() {
        let (ws, g, cfg) = crate::map::shop::load();
        let b = super::super::facts(&ws, &g, &cfg);
        let mut old = refresh(None, &b, arrows(&b, &g), &[]).0;
        // an agent renamed every component: same files, its own ids
        for x in old
            .boxes
            .iter_mut()
            .filter(|x| x.level == super::super::Level::Component)
        {
            x.id = format!("{}-read", x.id);
            x.made_of_source = file::Source::Agent;
        }
        let (mut m, _) = refresh(Some(&old), &b, arrows(&b, &g), &[]);
        relift(&mut m, &g, &[]);
        let ids: HashSet<&str> = m.boxes.iter().map(|x| x.id.as_str()).collect();
        assert!(m.arrows.iter().any(|a| a.from.ends_with("-read")));
        assert!(
            (m.arrows.iter()).all(|a| ids.contains(a.from.as_str()) && ids.contains(a.to.as_str())),
            "{:?}",
            m.arrows
        );
    }

    #[test]
    fn an_invalid_map_is_never_written() {
        let files = MemFiles::with(&[]);
        let mut m = MapFile {
            version: 2,
            boxes: vec![],
            arrows: vec![],
            marks: vec![],
        };
        let n = write_spec(&files, &m, "the code").unwrap();
        assert!(n.contains("not written") && n.contains("version 2"), "{n}");
        assert!(files.glob(&["ig/*".into()]).unwrap().is_empty());
        m.version = 1;
        let n = write_spec(&files, &m, "the code").unwrap();
        assert!(n.starts_with("wrote ig/"), "{n}");
    }
}
