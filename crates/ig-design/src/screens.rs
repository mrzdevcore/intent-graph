//! plans/<slug>/screens/<id>.json (v1): one screen of the user's mockup. Pure: text in, text out.
//! The Rust structs are the schema (camelCase on disk); the same `apply` validates edits.

use crate::caps::{MAX_ID, MAX_NAME, MAX_TEXT};
use crate::{DesignError, VERSION, is_element_id};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// A pen stroke has at most this many points.
// ponytail: point cap; simplify strokes if users hit it
pub const MAX_POINTS: usize = 2000;
/// Entries in an options/columns/tabs list.
const MAX_LIST: usize = 200;
/// Pieces per screen, navs per screen, screens per feature: checked when an edit adds to them.
pub const MAX_PIECES: usize = 2000;
pub const MAX_NAVS: usize = 500;
pub const MAX_SCREENS: usize = 200;
/// Screen ids are file names: the slug length.
const MAX_SCREEN_ID: usize = 64;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Screen {
    pub version: u32,
    pub id: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variant_of: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<String>,
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub background: Option<Background>,
    #[serde(default)]
    pub elements: Vec<Piece>,
    #[serde(default)]
    pub navs: Vec<Nav>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Background {
    /// `<sha256-12>.png|jpg`
    pub asset: String,
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum PieceKind {
    Frame,
    Button,
    Input,
    Select,
    Checkbox,
    Table,
    List,
    Card,
    Tabs,
    Modal,
    Text,
    Heading,
    Image,
    Icon,
    Pen,
    Arrow,
    Note,
}

/// The closed set of props; which ones a kind may carry is `allowed`.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Props {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub primary: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placeholder: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub options: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checked: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub columns: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rows: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub items: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tabs: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub level: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub asset: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub question: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub answer: Option<String>,
}

impl Props {
    pub fn is_empty(&self) -> bool {
        *self == Props::default()
    }
}

/// Every prop name, for refusing a name no kind has.
pub(crate) const PROP_NAMES: [&str; 16] = [
    "text",
    "primary",
    "placeholder",
    "options",
    "checked",
    "columns",
    "rows",
    "items",
    "tabs",
    "active",
    "title",
    "level",
    "asset",
    "name",
    "question",
    "answer",
];

fn allowed(k: PieceKind) -> &'static [&'static str] {
    use PieceKind::*;
    match k {
        Frame | Card | Pen | Arrow => &[],
        Button => &["text", "primary"],
        Input => &["placeholder"],
        Select => &["options"],
        Checkbox => &["checked"],
        Table => &["columns", "rows"],
        List => &["items"],
        Tabs => &["tabs", "active"],
        Modal => &["title"],
        Text => &["text"],
        Heading => &["text", "level"],
        Image => &["asset"],
        Icon => &["name"],
        Note => &["text", "question", "answer"],
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Piece {
    pub id: String,
    pub kind: PieceKind,
    pub label: String,
    #[serde(default, skip_serializing_if = "Props::is_empty")]
    pub props: Props,
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    /// Pen: x, y, pressure 0-100.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub points: Option<Vec<[i32; 3]>>,
    /// Arrow ends: piece ids.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Nav {
    /// A piece id of this screen.
    pub from: String,
    /// A screen id.
    pub to: String,
    pub label: String,
}

fn invalid<T>(msg: String) -> Result<T, DesignError> {
    Err(DesignError::Invalid(msg))
}

/// A stored image name: `^[0-9a-f]{12}\.(png|jpg)$`.
pub fn is_asset_name(s: &str) -> bool {
    s.split_once('.').is_some_and(|(h, e)| {
        h.len() == 12
            && h.bytes().all(|c| matches!(c, b'0'..=b'9' | b'a'..=b'f'))
            && (e == "png" || e == "jpg")
    })
}

pub fn parse_screen(text: &str) -> Result<Screen, DesignError> {
    let v: serde_json::Value = serde_json::from_str(text)?;
    if let Some(n) = v.get("version").and_then(|n| n.as_u64())
        && n != VERSION as u64
    {
        return Err(DesignError::Version(n.min(u32::MAX as u64) as u32));
    }
    let s: Screen = serde_json::from_value(v)?;
    validate_screen(&s)?;
    Ok(s)
}

/// Canonical text: navs sorted, elements in z-order, two-space JSON, trailing newline.
pub fn screen_to_text(s: &Screen) -> String {
    let mut s = s.clone();
    s.navs
        .sort_by(|a, b| (&a.from, &a.to).cmp(&(&b.from, &b.to)));
    let mut t = serde_json::to_string_pretty(&s).expect("screen serializes");
    t.push('\n');
    t
}

/// A screen alone (ids, props, strokes, arrows, navs from its own pieces); what other screens
/// exist is `check_refs`.
pub fn validate_screen(s: &Screen) -> Result<(), DesignError> {
    if s.version != VERSION {
        return Err(DesignError::Version(s.version));
    }
    if !is_element_id(&s.id) || s.id.len() > MAX_SCREEN_ID {
        return invalid(format!("bad screen id {:?}", s.id));
    }
    if s.variant_of.is_some() && s.state.as_deref().is_none_or(str::is_empty) {
        return invalid(format!("{}: a variant needs a state", s.id));
    }
    if let Some(v) = &s.variant_of
        && (!is_element_id(v) || *v == s.id)
    {
        return invalid(format!("{}: bad variantOf {v:?}", s.id));
    }
    if let Some(b) = &s.background
        && !is_asset_name(&b.asset)
    {
        return invalid(format!("{}: bad background asset {:?}", s.id, b.asset));
    }
    let mut ids = std::collections::HashSet::new();
    for p in &s.elements {
        if !is_element_id(&p.id) {
            return invalid(format!("bad element id {:?}", p.id));
        }
        if !ids.insert(p.id.as_str()) {
            return invalid(format!("duplicate element id {}", p.id));
        }
    }
    for p in &s.elements {
        validate_piece(p, &ids)?;
    }
    for n in &s.navs {
        if !ids.contains(n.from.as_str()) {
            return invalid(format!("{}: nav from unknown element {}", s.id, n.from));
        }
        if !is_element_id(&n.to) {
            return invalid(format!("{}: bad nav target {:?}", s.id, n.to));
        }
    }
    Ok(())
}

fn validate_piece(p: &Piece, ids: &std::collections::HashSet<&str>) -> Result<(), DesignError> {
    let id = &p.id;
    let set = match serde_json::to_value(&p.props) {
        Ok(serde_json::Value::Object(m)) => m,
        _ => Default::default(),
    };
    let ok = allowed(p.kind);
    for k in PROP_NAMES.iter().filter(|k| set.contains_key(**k)) {
        if !ok.contains(k) {
            return invalid(format!("{id}: unknown prop {k}"));
        }
    }
    if p.props.level.is_some_and(|l| !(1..=3).contains(&l)) {
        return invalid(format!("{id}: level is 1 to 3"));
    }
    if let Some(a) = &p.props.asset
        && !is_asset_name(a)
    {
        return invalid(format!("{id}: bad asset {a:?}"));
    }
    if p.points.is_some() && p.kind != PieceKind::Pen {
        return invalid(format!("{id}: points are for pens"));
    }
    if let Some(pts) = &p.points {
        if pts.len() > MAX_POINTS {
            return invalid(format!("{id}: more than {MAX_POINTS} points"));
        }
        if pts.iter().any(|q| !(0..=100).contains(&q[2])) {
            return invalid(format!("{id}: pressure is 0 to 100"));
        }
    }
    if (p.from.is_some() || p.to.is_some()) && p.kind != PieceKind::Arrow {
        return invalid(format!("{id}: from and to are for arrows"));
    }
    for e in [&p.from, &p.to].into_iter().flatten() {
        if !ids.contains(e.as_str()) {
            return invalid(format!("{id}: arrow end {e} is not on this screen"));
        }
    }
    Ok(())
}

/// What `s` names elsewhere must exist: its variant parent and its nav targets.
pub fn check_refs(s: &Screen, screens: &BTreeMap<String, Screen>) -> Result<(), DesignError> {
    if let Some(v) = &s.variant_of
        && !screens.contains_key(v)
    {
        return invalid(format!("{}: variantOf names missing screen {v}", s.id));
    }
    for n in &s.navs {
        if !screens.contains_key(&n.to) {
            return invalid(format!("{}: nav to missing screen {}", s.id, n.to));
        }
    }
    Ok(())
}

/// A fresh piece id: `<prefix>-<slug of label>`, numbered when taken. Pens, arrows, notes and
/// labels with no letters or digits are numbered from 1.
pub fn new_piece_id(s: &Screen, kind: PieceKind, label: &str) -> String {
    use PieceKind::*;
    let prefix = match kind {
        Frame => "frame",
        Button => "btn",
        Input => "input",
        Select => "select",
        Checkbox => "check",
        Table => "table",
        List => "list",
        Card => "card",
        Tabs => "tabs",
        Modal => "modal",
        Text => "text",
        Heading => "h",
        Image => "img",
        Icon => "icon",
        Pen => "pen",
        Arrow => "arrow",
        Note => "note",
    };
    let slug = label
        .to_ascii_lowercase()
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    let taken = |id: &str| s.elements.iter().any(|e| e.id == id);
    if matches!(kind, Pen | Arrow | Note) || slug.is_empty() {
        return (1..)
            .map(|n| format!("{prefix}-{n}"))
            .find(|id| !taken(id))
            .unwrap();
    }
    let base = format!("{prefix}-{slug}");
    if !taken(&base) {
        return base;
    }
    (2..)
        .map(|n| format!("{base}-{n}"))
        .find(|id| !taken(id))
        .unwrap()
}

fn cap(what: &str, s: &str, max: usize) -> Result<(), DesignError> {
    if s.len() > max {
        return invalid(format!("{what} is longer than {max} bytes"));
    }
    Ok(())
}

fn cap_list(what: &str, v: &[String]) -> Result<(), DesignError> {
    if v.len() > MAX_LIST {
        return invalid(format!("{what} has more than {MAX_LIST} entries"));
    }
    v.iter().try_for_each(|s| cap(what, s, MAX_NAME))
}

/// The caps, on the pieces, navs and screens the edit changed only (as `caps::caps` does).
pub(crate) fn caps(
    old: &BTreeMap<String, Screen>,
    new: &BTreeMap<String, Screen>,
) -> Result<(), DesignError> {
    for (sid, s) in new {
        let o = old.get(sid);
        if o.is_some_and(|o| o == s) {
            continue;
        }
        if s.elements.len() > MAX_PIECES && o.is_none_or(|o| s.elements.len() > o.elements.len()) {
            return invalid(format!("{sid}: more than {MAX_PIECES} elements"));
        }
        if s.navs.len() > MAX_NAVS && o.is_none_or(|o| s.navs.len() > o.navs.len()) {
            return invalid(format!("{sid}: more than {MAX_NAVS} navs"));
        }
        if o.is_none_or(|o| o.title != s.title) {
            cap(&format!("{sid}: the title"), &s.title, MAX_NAME)?;
        }
        if o.is_none_or(|o| o.state != s.state)
            && let Some(v) = &s.state
        {
            cap(&format!("{sid}: the state"), v, MAX_NAME)?;
        }
        for n in s
            .navs
            .iter()
            .filter(|n| o.is_none_or(|o| !o.navs.contains(n)))
        {
            cap("a nav label", &n.label, MAX_NAME)?;
            cap("a nav end", &n.from, MAX_ID)?;
            cap("a nav end", &n.to, MAX_ID)?;
        }
        for p in s
            .elements
            .iter()
            .filter(|p| o.is_none_or(|o| !o.elements.contains(p)))
        {
            let w = |f: &str| format!("{sid}/{}: {f}", p.id);
            cap(&w("the id"), &p.id, MAX_ID)?;
            cap(&w("the label"), &p.label, MAX_NAME)?;
            let q = &p.props;
            for (f, v, max) in [
                ("text", &q.text, MAX_TEXT),
                ("answer", &q.answer, MAX_TEXT),
                ("placeholder", &q.placeholder, MAX_NAME),
                ("title", &q.title, MAX_NAME),
                ("name", &q.name, MAX_NAME),
                ("from", &p.from, MAX_ID),
                ("to", &p.to, MAX_ID),
            ] {
                if let Some(v) = v {
                    cap(&w(f), v, max)?;
                }
            }
            for (f, v) in [
                ("options", &q.options),
                ("columns", &q.columns),
                ("tabs", &q.tabs),
            ] {
                if let Some(v) = v {
                    cap_list(&w(f), v)?;
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "screens_tests.rs"]
mod tests;
