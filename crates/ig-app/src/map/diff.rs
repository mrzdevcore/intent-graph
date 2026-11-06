//! `ig diff`: the architecture of two sides of a change, compared box by box and arrow by arrow,
//! with what changed in `ig/` and the problems the change brings or fixes.

use super::file::{Field, MapBox, MapFile};
use super::origin::{Origin, glob};
use super::{ArrowKind, Level, MarkState};
use globset::{GlobSet, GlobSetBuilder};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

pub enum SpecSide {
    Read(MapFile),
    Missing,
    Broken(String),
}

pub struct Side {
    pub arch: MapFile,
    pub spec: SpecSide,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Diff {
    pub base: String,
    pub head: Option<String>,
    pub source: Source,
    pub summary: String,
    pub boxes: BoxChanges,
    pub arrows: ArrowChanges,
    pub intent: Intent,
    pub problems: Problems,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    Spec,
    Code,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BoxRef {
    pub id: String,
    pub title: String,
    pub parent: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangedBox {
    pub id: String,
    pub title: String,
    pub parent: Option<String>,
    pub files: Vec<String>,
    pub files_changed: u32,
}

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BoxChanges {
    pub added: Vec<BoxRef>,
    pub removed: Vec<BoxRef>,
    pub changed: Vec<ChangedBox>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArrowRef {
    pub from: String,
    pub to: String,
    pub kind: String,
}

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArrowChanges {
    pub added: Vec<ArrowRef>,
    pub removed: Vec<ArrowRef>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum IntentStatus {
    Same,
    Changed,
    Added,
    Removed,
    BaseBroken,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Moved {
    pub id: String,
    pub from: Option<String>,
    pub to: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FieldsChanged {
    pub id: String,
    pub fields: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Intent {
    pub status: IntentStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    pub added: Vec<BoxRef>,
    pub removed: Vec<BoxRef>,
    pub moved: Vec<Moved>,
    pub changed: Vec<FieldsChanged>,
    pub arrows_added: Vec<ArrowRef>,
    pub arrows_removed: Vec<ArrowRef>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ProblemKind {
    Rule,
    Drift,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Problem {
    pub kind: ProblemKind,
    pub text: String,
    pub boxes: Vec<String>,
}

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Problems {
    pub new: Vec<Problem>,
    pub fixed: Vec<Problem>,
}

pub fn diff(
    base: &Side,
    head: &Side,
    changed: &[String],
    base_rev: &str,
    head_rev: Option<&str>,
) -> Diff {
    let source = match (&base.spec, &head.spec) {
        (SpecSide::Read(_), SpecSide::Read(_)) => Source::Spec,
        _ => Source::Code,
    };
    let (bk, hk) = (keys(&base.arch), keys(&head.arch));
    let (boxes, direct) = boxes(&base.arch, &head.arch, &bk, &hk, changed);
    let arrows = ArrowChanges {
        added: arrows_missing(&head.arch, &hk, &base.arch, &bk),
        removed: arrows_missing(&base.arch, &bk, &head.arch, &hk),
    };
    let intent = intent(&base.spec, &head.spec);
    let problems = match source {
        Source::Spec => {
            let (b, h) = (problems(&base.arch), problems(&head.arch));
            Problems {
                new: missing(&h, &b),
                fixed: missing(&b, &h),
            }
        }
        Source::Code => Problems::default(),
    };
    let mut d = Diff {
        base: base_rev.to_string(),
        head: head_rev.map(String::from),
        source,
        summary: String::new(),
        boxes,
        arrows,
        intent,
        problems,
    };
    d.summary = summary(&d, direct, &head.arch);
    d
}

// drift ids carry a counter that shifts between sides: drift is matched by its sorted globs
fn keys(m: &MapFile) -> HashMap<&str, String> {
    (m.boxes.iter())
        .map(|b| {
            let k = match b.origin {
                Origin::Drift => format!("\0drift {}", sorted_globs(b)),
                _ => b.id.clone(),
            };
            (b.id.as_str(), k)
        })
        .collect()
}

fn sorted_globs(b: &MapBox) -> String {
    let mut g = b.made_of.clone();
    g.sort();
    g.join(", ")
}

fn key<'a>(keys: &'a HashMap<&str, String>, id: &'a str) -> &'a str {
    keys.get(id).map_or(id, String::as_str)
}

fn box_ref(b: &MapBox) -> BoxRef {
    BoxRef {
        id: b.id.clone(),
        title: b.title.value.clone(),
        parent: b.parent.clone(),
    }
}

// drift is reported as a problem, not as a box
fn boxes_missing(
    a: &MapFile,
    ak: &HashMap<&str, String>,
    bk: &HashMap<&str, String>,
) -> Vec<BoxRef> {
    let theirs: HashSet<&String> = bk.values().collect();
    let mut out: Vec<BoxRef> = (a.boxes.iter())
        .filter(|b| b.origin != Origin::Drift && !theirs.contains(&ak[b.id.as_str()]))
        .map(box_ref)
        .collect();
    out.sort_by(|x, y| x.id.cmp(&y.id));
    out
}

fn boxes(
    base: &MapFile,
    head: &MapFile,
    bk: &HashMap<&str, String>,
    hk: &HashMap<&str, String>,
    changed: &[String],
) -> (BoxChanges, usize) {
    let added = boxes_missing(head, hk, bk);
    let removed = boxes_missing(base, bk, hk);
    let is_added: HashSet<&str> = added.iter().map(|b| b.id.as_str()).collect();
    let head_by_key: HashMap<&str, usize> = (head.boxes.iter().enumerate())
        .map(|(i, b)| (hk[b.id.as_str()].as_str(), i))
        .collect();
    let head_by_id: HashMap<&str, usize> = (head.boxes.iter().enumerate())
        .map(|(i, b)| (b.id.as_str(), i))
        .collect();
    let mut claims = claim(head, changed);
    let unclaimed: Vec<String> = (changed.iter())
        .filter(|f| !claims.iter().any(|(c, _)| *c == f.as_str()))
        .cloned()
        .collect();
    // a file under a box's old `at` only, still a change of that box
    for (f, i) in claim(base, &unclaimed) {
        if let Some(&h) = head_by_key.get(key(bk, &base.boxes[i].id)) {
            claims.push((f, h));
        }
    }
    let mut files: BTreeMap<usize, BTreeSet<&str>> = BTreeMap::new();
    let mut direct: HashSet<usize> = HashSet::new();
    for (f, i) in &claims {
        if !is_added.contains(head.boxes[*i].id.as_str()) {
            direct.insert(*i);
        }
        let mut at = Some(*i);
        let mut hops = 0;
        while let Some(j) = at.filter(|_| hops <= head.boxes.len()) {
            if !is_added.contains(head.boxes[j].id.as_str()) {
                files.entry(j).or_default().insert(f);
            }
            at = (head.boxes[j].parent.as_deref()).and_then(|p| head_by_id.get(p).copied());
            hops += 1;
        }
    }
    let mut changed: Vec<ChangedBox> = (files.into_iter())
        .map(|(i, fs)| {
            let b = &head.boxes[i];
            ChangedBox {
                id: b.id.clone(),
                title: b.title.value.clone(),
                parent: b.parent.clone(),
                files_changed: fs.len() as u32,
                files: fs.into_iter().map(String::from).collect(),
            }
        })
        .collect();
    changed.sort_by(|a, b| (b.files_changed.cmp(&a.files_changed)).then(a.id.cmp(&b.id)));
    let n = direct.len();
    (
        BoxChanges {
            added,
            removed,
            changed,
        },
        n,
    )
}

// the system's `at` is the whole repo: it claims nothing
fn claim<'f>(m: &MapFile, files: &'f [String]) -> Vec<(&'f str, usize)> {
    let mut set = GlobSetBuilder::new();
    let mut glob_box: Vec<usize> = vec![];
    for (i, b) in m.boxes.iter().enumerate() {
        if b.level == Level::System || b.stale {
            continue;
        }
        for g in b.made_of.iter().filter_map(|g| glob(g)) {
            set.add(g);
            glob_box.push(i);
        }
    }
    let set = set.build().unwrap_or_else(|_| GlobSet::empty());
    let parents: HashMap<&str, Option<&str>> = (m.boxes.iter())
        .map(|b| (b.id.as_str(), b.parent.as_deref()))
        .collect();
    let depth = |id: &str| {
        let mut d = 0;
        let mut at = parents.get(id).copied().flatten();
        while let Some(p) = at.filter(|_| d <= parents.len()) {
            d += 1;
            at = parents.get(p).copied().flatten();
        }
        d
    };
    (files.iter())
        .filter_map(|f| {
            let hits = set.matches(f).into_iter().map(|k| glob_box[k]);
            let i = hits.max_by_key(|&i| depth(&m.boxes[i].id))?;
            Some((f.as_str(), i))
        })
        .collect()
}

fn kind_name(k: ArrowKind) -> String {
    serde_json::to_value(k)
        .ok()
        .and_then(|v| v.as_str().map(String::from))
        .unwrap_or_default()
}

fn arrows_missing(
    a: &MapFile,
    ak: &HashMap<&str, String>,
    b: &MapFile,
    bk: &HashMap<&str, String>,
) -> Vec<ArrowRef> {
    let theirs: HashSet<(&str, &str, ArrowKind)> = (b.arrows.iter())
        .map(|x| (key(bk, &x.from), key(bk, &x.to), x.kind))
        .collect();
    let mut out: Vec<ArrowRef> = (a.arrows.iter())
        .filter(|x| !theirs.contains(&(key(ak, &x.from), key(ak, &x.to), x.kind)))
        .map(|x| ArrowRef {
            from: x.from.clone(),
            to: x.to.clone(),
            kind: kind_name(x.kind),
        })
        .collect();
    out.sort();
    out.dedup();
    out
}

fn intent(base: &SpecSide, head: &SpecSide) -> Intent {
    let mut i = Intent {
        status: IntentStatus::Same,
        note: None,
        added: vec![],
        removed: vec![],
        moved: vec![],
        changed: vec![],
        arrows_added: vec![],
        arrows_removed: vec![],
    };
    match (base, head) {
        (SpecSide::Broken(e), _) => {
            i.status = IntentStatus::BaseBroken;
            i.note = Some(format!("base ig/ does not read: {e}"));
        }
        (SpecSide::Missing, SpecSide::Read(h)) => {
            i.status = IntentStatus::Added;
            i.note = Some(format!(
                "ig/ added ({})",
                count(h.boxes.len(), "box", "boxes")
            ));
        }
        (SpecSide::Read(_), SpecSide::Missing) => {
            i.status = IntentStatus::Removed;
            i.note = Some("ig/ removed".into());
        }
        (SpecSide::Read(b), SpecSide::Read(h)) => {
            compare(&mut i, b, h);
            let same = i.added.is_empty()
                && i.removed.is_empty()
                && i.moved.is_empty()
                && i.changed.is_empty()
                && i.arrows_added.is_empty()
                && i.arrows_removed.is_empty();
            if !same {
                i.status = IntentStatus::Changed;
            }
        }
        _ => {}
    }
    i
}

fn compare(i: &mut Intent, base: &MapFile, head: &MapFile) {
    let (bk, hk) = (ids(base), ids(head));
    i.added = boxes_missing(head, &hk, &bk);
    i.removed = boxes_missing(base, &bk, &hk);
    i.arrows_added = arrows_missing(head, &hk, base, &bk);
    i.arrows_removed = arrows_missing(base, &bk, head, &hk);
    let olds: HashMap<&str, &MapBox> = base.boxes.iter().map(|b| (b.id.as_str(), b)).collect();
    let mut news: Vec<&MapBox> = head.boxes.iter().collect();
    news.sort_by(|a, b| a.id.cmp(&b.id));
    for new in news {
        let Some(old) = olds.get(new.id.as_str()) else {
            continue;
        };
        if old.parent != new.parent {
            i.moved.push(Moved {
                id: new.id.clone(),
                from: old.parent.clone(),
                to: new.parent.clone(),
            });
        }
        let fields = fields(old, new);
        if !fields.is_empty() {
            i.changed.push(FieldsChanged {
                id: new.id.clone(),
                fields,
            });
        }
    }
}

fn ids(m: &MapFile) -> HashMap<&str, String> {
    m.boxes
        .iter()
        .map(|b| (b.id.as_str(), b.id.clone()))
        .collect()
}

fn value<T>(f: &Option<Field<T>>) -> Option<&T> {
    f.as_ref().map(|f| &f.value)
}

// by value: who wrote a field is no change of it
fn fields(old: &MapBox, new: &MapBox) -> Vec<String> {
    let mut out: Vec<String> = vec![];
    let mut differ = |name: &str, d: bool| {
        if d {
            out.push(name.into());
        }
    };
    differ("title", old.title.value != new.title.value);
    differ(
        "description",
        value(&old.description) != value(&new.description),
    );
    differ(
        "kind",
        old.kind.value != new.kind.value || old.custom != new.custom,
    );
    differ("tech", value(&old.tech) != value(&new.tech));
    differ("at", old.made_of != new.made_of);
    differ("pattern", value(&old.pattern) != value(&new.pattern));
    let props: BTreeSet<&String> = old.props.keys().chain(new.props.keys()).collect();
    for p in props {
        differ(p, old.props.get(p) != new.props.get(p));
    }
    differ("badges", value(&old.badges) != value(&new.badges));
    differ("logo", value(&old.logo) != value(&new.logo));
    out
}

fn problems(m: &MapFile) -> BTreeMap<(u8, String, String, String), Problem> {
    let mut out = BTreeMap::new();
    for b in m.boxes.iter().filter(|b| b.origin == Origin::Drift) {
        let globs = sorted_globs(b);
        let p = Problem {
            kind: ProblemKind::Drift,
            text: format!("code under {globs} is in no box"),
            boxes: vec![b.id.clone()],
        };
        out.insert((1, globs, String::new(), String::new()), p);
    }
    let marks = m.marks.iter().filter(|k| k.layer == "rules");
    for k in marks.filter(|k| k.state == MarkState::Bad) {
        let text = (k.badge.as_ref().or(k.note.as_ref())).map_or(k.target.clone(), Clone::clone);
        let (from, to) = match m.arrows.iter().find(|a| a.id == k.target) {
            Some(a) => (a.from.clone(), a.to.clone()),
            None => (k.target.clone(), String::new()),
        };
        let boxes = [&from, &to].into_iter().filter(|s| !s.is_empty()).cloned();
        let p = Problem {
            kind: ProblemKind::Rule,
            text: text.clone(),
            boxes: boxes.collect(),
        };
        out.insert((0, text, from, to), p);
    }
    out
}

fn missing<K: Ord>(a: &BTreeMap<K, Problem>, b: &BTreeMap<K, Problem>) -> Vec<Problem> {
    (a.iter())
        .filter(|(k, _)| !b.contains_key(k))
        .map(|(_, p)| p.clone())
        .collect()
}

fn count(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

fn summary(d: &Diff, direct: usize, head: &MapFile) -> String {
    let mut parts: Vec<String> = vec![];
    let mut push = |n: usize, s: String| {
        if n > 0 {
            parts.push(s);
        }
    };
    push(direct, count(direct, "box", "boxes") + " changed");
    let (added, removed) = (d.boxes.added.len(), d.boxes.removed.len());
    push(added, count(added, "box", "boxes") + " added");
    push(removed, count(removed, "box", "boxes") + " removed");
    let arrows = d.arrows.added.len();
    let title = |id: &str| {
        (head.boxes.iter().find(|b| b.id == id)).map_or(id.to_string(), |b| b.title.value.clone())
    };
    push(
        arrows,
        match d.arrows.added.as_slice() {
            [a] => format!("1 new arrow {} → {}", title(&a.from), title(&a.to)),
            _ => format!("{arrows} new arrows"),
        },
    );
    let gone = d.arrows.removed.len();
    push(gone, count(gone, "arrow", "arrows") + " removed");
    let ps = &d.problems;
    let rules = ps
        .new
        .iter()
        .filter(|p| p.kind == ProblemKind::Rule)
        .count();
    let drift = ps.new.len() - rules;
    let fixed = ps.fixed.len();
    if parts.is_empty() && ps.new.is_empty() && fixed == 0 && d.intent.status == IntentStatus::Same
    {
        return "no architecture change".into();
    }
    match (d.intent.status, &d.intent.note) {
        (IntentStatus::Same, _) if d.source == Source::Code => {}
        (IntentStatus::Same, _) => parts.push("ig/ unchanged".into()),
        (IntentStatus::Changed, _) => parts.push("ig/ changed".into()),
        (_, Some(n)) => parts.push(n.clone()),
        (_, None) => {}
    }
    let mut push = |n: usize, s: String| {
        if n > 0 {
            parts.push(s);
        }
    };
    push(rules, count(rules, "new rule break", "new rule breaks"));
    push(drift, format!("drift +{drift}"));
    push(fixed, count(fixed, "problem", "problems") + " fixed");
    parts.join(", ")
}

mod build;
pub use build::run;

#[cfg(test)]
mod tests;
