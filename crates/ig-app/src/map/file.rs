//! `.ig/map.json`: ig's facts plus the reading an agent or a person wrote on top. The file is
//! committed and shared, so it is written deterministically (keys and boxes sorted) and every
//! read of it, or of an agent's annotations, is validated.

use super::layers::{Mark, sort_marks};
use super::{BoxKind, Level, MapArrow, Pattern};
use globset::{GlobBuilder, GlobSet, GlobSetBuilder};
use ig_core::ports::Files;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// The map file, relative to the ig root.
pub const MAP: &str = ".ig/map.json";
const MAX_BYTES: usize = 2 * 1024 * 1024;
/// Field names a box may pin (as written in the JSON).
pub(super) const PINNABLE: &[&str] = &[
    "kind",
    "title",
    "tech",
    "description",
    "logo",
    "badges",
    "pattern",
    "slot",
    "serviceName",
    "madeOf",
];

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    #[default]
    Ig,
    Agent,
    Person,
}

/// How sure an agent is of what it wrote (as graphify grades its edges).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Confidence {
    Extracted,
    Inferred,
    Ambiguous,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Field<T> {
    pub value: T,
    pub source: Source,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub confidence: Option<Confidence>,
}

impl<T> Field<T> {
    pub fn ig(value: T) -> Field<T> {
        Field {
            value,
            source: Source::Ig,
            confidence: None,
        }
    }
    pub fn person(value: T) -> Field<T> {
        Field {
            value,
            source: Source::Person,
            confidence: None,
        }
    }
    /// An agent's field; `Inferred` unless it said otherwise.
    pub fn agent(value: T, confidence: Option<Confidence>) -> Field<T> {
        Field {
            value,
            source: Source::Agent,
            confidence: Some(confidence.unwrap_or(Confidence::Inferred)),
        }
    }
}

fn no(b: &bool) -> bool {
    !b
}

fn is_ig(s: &Source) -> bool {
    *s == Source::Ig
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MapBox {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub parent: Option<String>,
    pub level: Level,
    pub made_of: Vec<String>,
    /// Who drew the box's borders: an agent's or a person's grouping survives a refresh.
    #[serde(skip_serializing_if = "is_ig", default)]
    pub made_of_source: Source,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub package: Option<String>,
    pub files: u32,
    pub kind: Field<BoxKind>,
    /// A team kind's name (`ig/kinds/<name>.kdl`); `kind` is its base.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub custom: Option<String>,
    /// The kind's own properties, and `owner`.
    #[serde(skip_serializing_if = "BTreeMap::is_empty", default)]
    pub props: BTreeMap<String, String>,
    pub title: Field<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub tech: Option<Field<String>>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub description: Option<Field<String>>,
    /// A simple-icons slug.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub logo: Option<Field<String>>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub badges: Option<Field<Vec<String>>>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub pattern: Option<Field<Pattern>>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub slot: Option<Field<String>>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub context: Option<Field<String>>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub service_name: Option<String>,
    /// Field names (`PINNABLE`) a person fixed; ig and agents leave them alone.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub pinned: Vec<String>,
    #[serde(skip_serializing_if = "no", default)]
    pub needs_reading: bool,
    #[serde(skip_serializing_if = "no", default)]
    pub stale: bool,
    /// Built, to build or drift: the spec against the code.
    #[serde(skip_serializing_if = "super::origin::is_built", default)]
    pub origin: super::origin::Origin,
    /// A moved box's `at` at the last commit; not stored.
    #[serde(skip)]
    pub moved_from: Vec<String>,
    /// The box's files (module node ids), from the facts of this run; not stored.
    #[serde(skip)]
    pub nodes: Vec<String>,
}

impl MapBox {
    fn pins(&self, field: &str) -> bool {
        self.pinned.iter().any(|p| p == field)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MapFile {
    pub version: u32,
    pub boxes: Vec<MapBox>,
    pub arrows: Vec<MapArrow>,
    pub marks: Vec<Mark>,
}

/// One agent annotation of a box; absent fields are left as they are.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Annotation {
    pub id: String,
    pub kind: Option<BoxKind>,
    pub title: Option<String>,
    pub tech: Option<String>,
    pub description: Option<String>,
    pub logo: Option<String>,
    pub badges: Option<Vec<String>>,
    pub pattern: Option<Pattern>,
    pub slot: Option<String>,
    pub service_name: Option<String>,
    pub made_of: Option<Vec<String>>,
    pub confidence: Option<Confidence>,
}

/// The map file, or None (with a notice) when it is too big, unparsable or invalid. A missing
/// file is None without a notice.
pub fn load(files: &dyn Files) -> (Option<MapFile>, Vec<String>) {
    let Some(bytes) = files.read_bytes(MAP) else {
        return (None, vec![]);
    };
    if bytes.len() > MAX_BYTES {
        return (None, vec![format!("{MAP} is over 2 MB; ignored")]);
    }
    let m: MapFile = match serde_json::from_slice(&bytes) {
        Ok(m) => m,
        Err(e) => return (None, vec![format!("{MAP}: {e}; ignored")]),
    };
    match validate(&m) {
        Ok(()) => (Some(m), vec![]),
        Err(e) => (None, vec![format!("{MAP}: {}; ignored", e.join("; "))]),
    }
}

/// Writes the map with keys sorted, boxes and arrows sorted by id, marks by layer, target, badge.
pub fn save(files: &dyn Files, m: &MapFile) -> crate::Result<()> {
    let mut m = m.clone();
    m.boxes.sort_by(|a, b| a.id.cmp(&b.id));
    m.arrows.sort_by(|a, b| a.id.cmp(&b.id));
    sort_marks(&mut m.marks);
    let mut v = serde_json::to_value(&m)?;
    sort_keys(&mut v);
    let mut text = serde_json::to_string_pretty(&v)?;
    text.push('\n');
    files.write_atomic(MAP, text.as_bytes())?;
    Ok(())
}

/// Sorts every object's keys (serde_json keeps insertion order when another crate in the
/// build enables `preserve_order`).
fn sort_keys(v: &mut serde_json::Value) {
    match v {
        serde_json::Value::Object(o) => {
            o.sort_keys();
            o.values_mut().for_each(sort_keys);
        }
        serde_json::Value::Array(a) => a.iter_mut().for_each(sort_keys),
        _ => {}
    }
}

/// Applies a JSON array of `Annotation`s as the agent's reading. Pinned fields are skipped;
/// each annotated box no longer needs reading. Returns the count of fields set, or every
/// problem found (the map is then unchanged).
pub fn annotate(map: &mut MapFile, json: &str) -> Result<usize, Vec<String>> {
    annotate_with(map, json, true)
}

/// `annotate` for a map read from the spec: checked with `validate_spec`.
pub fn annotate_spec(map: &mut MapFile, json: &str) -> Result<usize, Vec<String>> {
    annotate_with(map, json, false)
}

/// `prefixed`: as `check`.
fn annotate_with(map: &mut MapFile, json: &str, prefixed: bool) -> Result<usize, Vec<String>> {
    let notes: Vec<Annotation> =
        serde_json::from_str(json).map_err(|e| vec![format!("annotations: {e}")])?;
    let mut m = map.clone();
    let mut errs = vec![];
    for a in notes.iter().filter(|a| a.id.ends_with("/unread")) {
        errs.push(format!(
            "`{}` holds the files no component claims; regroup them into real components with \
             madeOf instead of annotating it",
            a.id
        ));
    }
    let mut n = regroup(&mut m, &notes, &mut errs);
    for a in notes {
        let Some(b) = m.boxes.iter_mut().find(|b| b.id == a.id) else {
            errs.push(format!("unknown box `{}`", a.id));
            continue;
        };
        let c = a.confidence;
        let pinned = b.pinned.clone();
        let mut set = |name: &str, has: bool| {
            let ok = has && !pinned.iter().any(|p| p == name);
            n += ok as usize;
            ok
        };
        if set("kind", a.kind.is_some()) {
            b.kind = Field::agent(a.kind.unwrap(), c);
        }
        if set("title", a.title.is_some()) {
            b.title = Field::agent(a.title.unwrap(), c);
        }
        if set("tech", a.tech.is_some()) {
            b.tech = a.tech.map(|v| Field::agent(v, c));
        }
        if set("description", a.description.is_some()) {
            b.description = a.description.map(|v| Field::agent(v, c));
        }
        if set("logo", a.logo.is_some()) {
            b.logo = a.logo.map(|v| Field::agent(v, c));
        }
        if set("badges", a.badges.is_some()) {
            b.badges = a.badges.map(|v| Field::agent(v, c));
        }
        if set("pattern", a.pattern.is_some()) {
            b.pattern = a.pattern.map(|v| Field::agent(v, c));
        }
        if set("slot", a.slot.is_some()) {
            b.slot = a.slot.map(|v| Field::agent(v, c));
        }
        if set("serviceName", a.service_name.is_some()) {
            b.service_name = a.service_name;
        }
        b.needs_reading = false;
    }
    let by: std::collections::HashMap<&str, &MapBox> =
        m.boxes.iter().map(|b| (b.id.as_str(), b)).collect();
    let out_of_place: Vec<(String, Field<BoxKind>)> = (m.boxes.iter())
        .filter(|b| b.kind.source == Source::Agent)
        .filter(|b| {
            let name = super::kinds::base_name(b.kind.value);
            super::warn::misplaced(&by, b, b.kind.value, &name).is_some()
        })
        .map(|b| {
            let was = map.boxes.iter().find(|o| o.id == b.id);
            let kind = match was {
                Some(o) if o.kind.source != Source::Agent => o.kind.clone(),
                // an earlier batch's kind, made wrong by this batch moving the parent
                _ => Field::ig(BoxKind::Module),
            };
            (b.id.clone(), kind)
        })
        .collect();
    for (id, kind) in out_of_place {
        if let Some(b) = m.boxes.iter_mut().find(|b| b.id == id) {
            b.kind = kind;
            n = n.saturating_sub(1);
        }
    }
    // ig's own slots follow a pattern the agent changed; its own and pinned ones must fit
    let igs = |b: &MapBox| !b.pins("slot") && b.slot.as_ref().is_some_and(|s| is_ig(&s.source));
    refresh::fit_slots(&mut m.boxes, &[], &igs);
    if let Err(e) = check(&m, prefixed) {
        errs.extend(e);
    }
    if !errs.is_empty() {
        return Err(errs);
    }
    m.boxes.sort_by(|a, b| a.id.cmp(&b.id));
    *map = m;
    Ok(n)
}

/// The annotations with `madeOf` replace the components of their container: every file of the
/// old components must land in exactly one new one. Returns the count of `madeOf`s set.
fn regroup(m: &mut MapFile, notes: &[Annotation], errs: &mut Vec<String>) -> usize {
    let mut by_parent: BTreeMap<&str, Vec<&Annotation>> = BTreeMap::new();
    for a in notes.iter().filter(|a| a.made_of.is_some()) {
        let parent = a.id.split_once('/').map(|(p, _)| p);
        match parent.filter(|p| {
            m.boxes
                .iter()
                .any(|b| b.id == *p && b.level == Level::Container)
        }) {
            Some(p) => by_parent.entry(p).or_default().push(a),
            None => errs.push(format!(
                "`{}`: madeOf only regroups components (container/slug)",
                a.id
            )),
        }
    }
    let mut n = 0;
    for (parent, group) in by_parent {
        let is_kid =
            |b: &MapBox| b.parent.as_deref() == Some(parent) && b.level == Level::Component;
        let mut files: Vec<String> = m
            .boxes
            .iter()
            .filter(|b| is_kid(b))
            .flat_map(|b| b.nodes.clone())
            .collect();
        files.sort();
        files.dedup();
        let mut old: BTreeMap<String, MapBox> = BTreeMap::new();
        m.boxes.retain(|b| {
            if is_kid(b) {
                old.insert(b.id.clone(), b.clone());
            }
            !is_kid(b)
        });
        for o in old.values() {
            if !o.pinned.is_empty() && !group.iter().any(|a| a.id == o.id) {
                errs.push(format!(
                    "`{}` has pinned fields; it cannot be regrouped away",
                    o.id
                ));
            }
        }
        let package = m
            .boxes
            .iter()
            .find(|b| b.id == parent)
            .and_then(|b| b.package.clone());
        let mut kids: Vec<(MapBox, Option<GlobSet>)> = vec![];
        for a in group {
            let mut b = old.remove(&a.id).unwrap_or_else(|| MapBox {
                context: None,
                id: a.id.clone(),
                parent: Some(parent.into()),
                level: Level::Component,
                made_of: vec![],
                made_of_source: Source::Agent,
                package: package.clone(),
                files: 0,
                kind: Field::agent(BoxKind::Group, a.confidence),
                title: Field::agent(a.id[parent.len() + 1..].to_string(), a.confidence),
                tech: None,
                description: None,
                logo: None,
                badges: None,
                pattern: None,
                slot: None,
                service_name: None,
                pinned: vec![],
                needs_reading: false,
                stale: false,
                nodes: vec![],
                moved_from: vec![],
                origin: Default::default(),
                custom: None,
                props: Default::default(),
            });
            if !b.pins("madeOf") {
                b.made_of = a.made_of.clone().unwrap_or_default();
                b.made_of_source = Source::Agent;
                n += 1;
            }
            let set = globs(&b.made_of).ok();
            kids.push((b, set));
        }
        for f in &files {
            let hits: Vec<usize> = (0..kids.len())
                .filter(|&k| kids[k].1.as_ref().is_some_and(|s| s.is_match(f)))
                .collect();
            if hits.len() != 1 {
                errs.push(format!(
                    "`{f}` is in {} components of {parent}; it must be in exactly 1",
                    hits.len()
                ));
            }
            if let Some(&k) = hits.first() {
                kids[k].0.nodes.push(f.clone());
            }
        }
        for (mut b, _) in kids {
            b.files = b.nodes.len() as u32;
            m.boxes.push(b);
        }
    }
    n
}

pub(super) fn globs(patterns: &[String]) -> Result<GlobSet, globset::Error> {
    let mut set = GlobSetBuilder::new();
    for p in patterns {
        set.add(GlobBuilder::new(p).literal_separator(true).build()?);
    }
    set.build()
}

fn is_slug(s: &str) -> bool {
    let b = s.as_bytes();
    (1..=64).contains(&b.len())
        && b[0] != b'-'
        && b.iter()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == b'-')
}

/// `b` is refresh's box of `parent`'s files no component claims.
pub(crate) fn is_unread(b: &MapBox, parent: &str) -> bool {
    b.id.strip_prefix(parent) == Some("/unread") && b.kind.value == BoxKind::Group
}

/// The slots a box can take inside a parent drawn with `p`.
pub(crate) fn slots(p: Option<&Pattern>) -> Vec<&str> {
    match p {
        Some(Pattern::Onion) => vec!["domain", "application", "infrastructure"],
        Some(Pattern::Hexagonal) => vec!["core", "ports", "adapters"],
        Some(Pattern::Layered { bands }) => bands.iter().map(String::as_str).collect(),
        Some(Pattern::None) | None => vec![],
    }
}

/// Every problem with the map: version, ids, parents, lengths, globs, slots, pins. Where a kind
/// sits is not one: the table (`kinds::may_sit`) gives warnings.
pub fn validate(m: &MapFile) -> Result<(), Vec<String>> {
    check(m, true)
}

/// `validate` for a map read from the spec in `ig/`: a box's id is a slug or a slug path
/// (`api/orders`) that need not start with its parent's id, so that it stays the same when the
/// box moves.
pub fn validate_spec(m: &MapFile) -> Result<(), Vec<String>> {
    check(m, false)
}

/// `prefixed`: a component's id is `<container>/<slug>`.
fn check(m: &MapFile, prefixed: bool) -> Result<(), Vec<String>> {
    let mut errs = vec![];
    let by_id: BTreeMap<&str, &MapBox> = m.boxes.iter().map(|b| (b.id.as_str(), b)).collect();
    if by_id.len() != m.boxes.len() {
        let mut ids: Vec<&str> = m.boxes.iter().map(|b| b.id.as_str()).collect();
        ids.sort();
        for w in ids.windows(2).filter(|w| w[0] == w[1]) {
            errs.push(format!("id `{}` is used twice", w[0]));
        }
    }
    for b in &m.boxes {
        let id = &b.id;
        let path = |p: Option<&str>| {
            id.split_once('/')
                .is_some_and(|(c, s)| p.is_none_or(|p| c == p) && is_slug(c) && is_slug(s))
        };
        let ok = match (b.level, b.parent.as_deref()) {
            (Level::System, _) | (_, None) => is_slug(id),
            // the levels follow the kinds above a box, which may sit anywhere: an id of either form
            _ if !prefixed => is_slug(id) || path(None),
            (Level::Component | Level::Code, p) => path(p),
            _ => is_slug(id),
        };
        if !ok {
            errs.push(format!(
                "id `{id}` is not a slug (a-z, 0-9, -; components container/slug)"
            ));
        }
        let long = |what: &str, s: &str, max: usize, errs: &mut Vec<String>| {
            if s.chars().count() > max {
                errs.push(format!("`{id}`: {what} is over {max} characters"));
            }
        };
        long("title", &b.title.value, 60, &mut errs);
        if let Some(t) = &b.tech {
            long("tech", &t.value, 60, &mut errs);
        }
        if let Some(d) = &b.description {
            long("description", &d.value, 200, &mut errs);
        }
        if let Some(bs) = &b.badges {
            if bs.value.len() > 8 {
                errs.push(format!("`{id}`: over 8 badges"));
            }
            for x in &bs.value {
                long("a badge", x, 40, &mut errs);
            }
        }
        for g in &b.made_of {
            if g.starts_with('/') || g.split('/').any(|s| s == "..") {
                errs.push(format!("`{id}`: glob `{g}` must be relative, without `..`"));
            } else if let Err(e) = globs(std::slice::from_ref(g)) {
                errs.push(format!("`{id}`: glob `{g}`: {e}"));
            }
        }
        // a stale box's parent may be gone or drawn otherwise now
        if let Some(s) = b.slot.as_ref().filter(|_| !b.stale) {
            let parent = b.parent.as_deref().and_then(|p| by_id.get(p));
            let ok = slots(parent.and_then(|p| p.pattern.as_ref()).map(|f| &f.value));
            if !ok.contains(&s.value.as_str()) {
                errs.push(format!("`{id}`: slot `{}` is not one of {ok:?}", s.value));
            }
        }
        for p in &b.pinned {
            if !PINNABLE.contains(&p.as_str()) {
                errs.push(format!("`{id}`: `{p}` cannot be pinned"));
            }
        }
    }
    if m.version != 1 {
        errs.push(format!("version {} is not 1", m.version));
    }
    for b in &m.boxes {
        let id = &b.id;
        match (
            b.level,
            b.parent.as_deref().map(|p| (p, by_id.contains_key(p))),
        ) {
            (Level::System, None) => {}
            (Level::System, Some(_)) => errs.push(format!("`{id}`: the system has no parent")),
            (Level::Code, _) => errs.push(format!("`{id}`: code boxes are not stored")),
            (_, None) => errs.push(format!("`{id}`: no parent")),
            (_, Some((p, false))) => errs.push(format!("`{id}`: parent `{p}` is not in the map")),
            // where a kind may sit (`kinds::may_sit`) is a warning, never a read error
            (_, Some(_)) => {}
        }
        // this keeps a bad file from hanging a reader
        let mut seen = std::collections::BTreeSet::from([id.as_str()]);
        let mut at = b.parent.as_deref();
        while let Some(p) = at {
            if p == id.as_str() {
                errs.push(format!("`{id}` reaches itself through its parents"));
            }
            // a loop above this box is reported by the boxes in it
            if !seen.insert(p) {
                break;
            }
            at = by_id.get(p).and_then(|x| x.parent.as_deref());
        }
    }
    if errs.is_empty() { Ok(()) } else { Err(errs) }
}

mod refresh;
pub(crate) use refresh::fresh;
pub use refresh::refresh;

#[cfg(test)]
mod tests;
