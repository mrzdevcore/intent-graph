//! What `ig view` draws: the map with its code level, every layer's marks and the notices.

use super::agent::{Ask, Evidence, offer};
use super::code::{as_fact, code_boxes};
use super::file::{self, Field, MapBox, MapFile, refresh};
use super::layers::{Mark, file_layers, risks, rules, sort_marks};
use super::layout::layout;
pub use super::layout::{Frame, Placed, Region, RegionKind};
use super::{MapArrow, Pattern, arrows, facts};
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
    }
}

/// `ig view` without an agent: see `run_with`.
pub fn run(env: &Env) -> crate::Result<View> {
    run_with(env, None)
}

/// `ig view`: the facts of the working tree, refreshed into `.ig/map.json` and, with `ask`, read
/// by an AI agent (written back unless the file there was rejected: it is left for its owner to
/// fix), then the view of it.
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
    let (old, rejected) = file::load(env.files);
    let (mut m, reset) = refresh(old.as_ref(), &boxes, arrows(&boxes, g), &findings);
    relift(&mut m, g, &findings);
    let mut notices: Vec<String> = config
        .notices
        .iter()
        .map(|k| format!("ig.yaml: `{k}` is no longer read; ignored"))
        .collect();
    if rejected.is_empty() {
        if let Some(a) = ask {
            let ev = Evidence::new(&boxes, &built.extras.imports, &config);
            notices.extend(offer(a, &mut m, &ev, &config, env.files, env.progress));
            relift(&mut m, g, &findings);
        }
        notices.extend(save_valid(env.files, &m)?);
    } else {
        notices.extend(rejected);
        notices.push(format!(
            "{} left as is; fix or delete it to let ig rewrite it",
            file::MAP
        ));
    }
    notices.extend(reset);
    let ui = components(env.files, g, env.frontends, &config);
    let mut v = view(env.files, &m, g, &ui);
    notices.append(&mut v.notices);
    v.notices = notices;
    Ok(v)
}

/// `m`'s arrows lifted over its own boxes, and the marks that read them: an agent's or a
/// person's components are not ig's facts, so arrows lifted over the facts would end nowhere.
fn relift(m: &mut MapFile, g: &Graph, findings: &[ig_core::model::Finding]) {
    let live: Vec<_> = m.boxes.iter().filter(|b| !b.stale).map(as_fact).collect();
    m.arrows = arrows(&live, g);
    m.marks = rules(m, findings);
    m.marks.extend(risks(m));
    sort_marks(&mut m.marks);
}

/// Saves `m` unless `validate` rejects it (the next run would drop the whole file): then the
/// notice why.
fn save_valid(files: &dyn Files, m: &MapFile) -> crate::Result<Option<String>> {
    if let Err(e) = file::validate(m) {
        return Ok(Some(format!(
            "{} not written, ig's map is not valid: {}",
            file::MAP,
            e.join("; ")
        )));
    }
    file::save(files, m)?;
    Ok(None)
}

#[cfg(test)]
mod tests {
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
        let fresh = MemFiles::with(&[("src/a.ts", "export function f() {}\n")]);
        run(&env(&fresh, None, &fe)).unwrap();
        assert!(file::load(&fresh).0.is_some(), "a missing map is written");
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
        let n = save_valid(&files, &m).unwrap().unwrap();
        assert!(n.contains("not written") && n.contains("version 2"), "{n}");
        assert!(files.read(file::MAP).is_none());
        m.version = 1;
        assert_eq!(save_valid(&files, &m).unwrap(), None);
        assert!(files.read(file::MAP).is_some());
    }
}
