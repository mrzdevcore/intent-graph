//! design.json (v1): the user's architecture drawing for a feature. Pure: text in, text out.

mod caps;
mod ops;
mod screen_ops;
mod screens;

pub use caps::{MAX_FILE, MAX_ID, MAX_NAME, MAX_TEXT};
pub use ops::{Board, ElementPatch, Op, Touched, apply};
pub use screen_ops::PiecePatch;
pub use screens::{
    Background, MAX_POINTS, Nav, Piece, PieceKind, Props, Screen, check_refs, is_asset_name,
    new_piece_id, parse_screen, screen_to_text, validate_screen,
};

use serde::{Deserialize, Serialize};
use std::collections::HashSet;

pub const VERSION: u32 = 1;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Design {
    pub version: u32,
    pub title: String,
    pub scope: Scope,
    #[serde(default)]
    pub elements: Vec<Element>,
    #[serde(default)]
    pub marks: Vec<Mark>,
    #[serde(default)]
    pub links: Vec<Link>,
    #[serde(default)]
    pub bridges: Vec<Bridge>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Scope {
    Dir { id: String },
    Layer { name: String },
    Around { id: String, depth: u32 },
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ElementKind {
    Module,
    Function,
    Route,
    System,
    Note,
    Frame,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Element {
    pub id: String,
    pub kind: ElementKind,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub method: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layer: Option<String>,
    /// Note target (an element id or a graph id).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on: Option<String>,
    /// Notes only.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub question: bool,
    /// Notes only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub answer: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frame: Option<String>,
    /// Geometry is optional: absent means unpositioned (the app places it later).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub x: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub y: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub w: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub h: Option<i32>,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum MarkKind {
    Changed,
    Remove,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Mark {
    pub node: String,
    pub mark: MarkKind,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum LinkKind {
    Calls,
    Imports,
    Renders,
    Http,
    Uses,
}

impl LinkKind {
    pub fn as_str(self) -> &'static str {
        match self {
            LinkKind::Calls => "calls",
            LinkKind::Imports => "imports",
            LinkKind::Renders => "renders",
            LinkKind::Http => "http",
            LinkKind::Uses => "uses",
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Link {
    pub from: String,
    pub to: String,
    pub kind: LinkKind,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Bridge {
    /// `<screen-id>/<element-id>`
    pub from: String,
    pub to: String,
}

impl Link {
    pub fn key(&self) -> String {
        format!("{}|{}|{}", self.from, self.kind.as_str(), self.to)
    }
}

impl Bridge {
    pub fn key(&self) -> String {
        format!("{}>{}", self.from, self.to)
    }
}

#[derive(thiserror::Error, Debug)]
pub enum DesignError {
    #[error("design.json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("design.json is version {0}; this ig reads version 1: update ig")]
    Version(u32),
    #[error("design.json: {0}")]
    Invalid(String),
}

/// `^[a-z][a-z0-9-]*$`: the one id rule for elements, screens and bridges.
pub fn is_element_id(s: &str) -> bool {
    let mut b = s.bytes();
    b.next().is_some_and(|c| c.is_ascii_lowercase())
        && b.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
}

pub fn empty(title: &str, scope: Scope) -> Design {
    Design {
        version: VERSION,
        title: title.into(),
        scope,
        elements: vec![],
        marks: vec![],
        links: vec![],
        bridges: vec![],
    }
}

/// A fresh element id: `<prefix>-<slug of name>`, numbered when taken. Notes (and names with
/// no letters or digits) are numbered from 1.
pub fn new_id(d: &Design, kind: ElementKind, name: &str) -> String {
    let prefix = match kind {
        ElementKind::Module => "mod",
        ElementKind::Function => "fn",
        ElementKind::Route => "route",
        ElementKind::System => "sys",
        ElementKind::Note => "note",
        ElementKind::Frame => "frame",
    };
    let slug = name
        .to_ascii_lowercase()
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    let taken = |id: &str| d.elements.iter().any(|e| e.id == id);
    if kind == ElementKind::Note || slug.is_empty() {
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

pub fn parse(text: &str) -> Result<Design, DesignError> {
    let v: serde_json::Value = serde_json::from_str(text)?;
    if let Some(n) = v.get("version").and_then(|n| n.as_u64())
        && n != VERSION as u64
    {
        return Err(DesignError::Version(n.min(u32::MAX as u64) as u32));
    }
    let d: Design = serde_json::from_value(v)?;
    validate(&d)?;
    Ok(d)
}

fn invalid<T>(msg: String) -> Result<T, DesignError> {
    Err(DesignError::Invalid(msg))
}

pub fn validate(d: &Design) -> Result<(), DesignError> {
    let mut ids = HashSet::new();
    for e in &d.elements {
        if !is_element_id(&e.id) {
            return invalid(format!("bad element id {:?}", e.id));
        }
        if !ids.insert(e.id.as_str()) {
            return invalid(format!("duplicate element id {}", e.id));
        }
        if (e.question || e.answer.is_some()) && e.kind != ElementKind::Note {
            return invalid(format!("{}: question and answer are for notes", e.id));
        }
    }
    for e in &d.elements {
        if let Some(f) = &e.frame
            && !ids.contains(f.as_str())
        {
            return invalid(format!("{}: no frame {f}", e.id));
        }
        if let Some(o) = &e.on
            && is_element_id(o)
            && !ids.contains(o.as_str())
        {
            return invalid(format!("{}: note on unknown element {o}", e.id));
        }
    }
    // An end that looks like an element id must name one; anything else is a graph id.
    let check = |what: &str, key: String, end: &str| {
        if is_element_id(end) && !ids.contains(end) {
            return invalid(format!("{what} {key}: no element {end}"));
        }
        Ok(())
    };
    for l in &d.links {
        check("link", l.key(), &l.from)?;
        check("link", l.key(), &l.to)?;
    }
    for b in &d.bridges {
        let ok = b
            .from
            .split_once('/')
            .is_some_and(|(s, e)| is_element_id(s) && is_element_id(e));
        if !ok {
            return invalid(format!(
                "bridge {}: from must be <screen-id>/<element-id>",
                b.key()
            ));
        }
        check("bridge", b.key(), &b.to)?;
    }
    Ok(())
}

/// Canonical text: collections sorted, two-space JSON, trailing newline.
pub fn to_text(d: &Design) -> String {
    let mut d = d.clone();
    d.elements.sort_by(|a, b| a.id.cmp(&b.id));
    d.marks.sort_by(|a, b| a.node.cmp(&b.node));
    d.links.sort_by_key(Link::key);
    d.bridges.sort_by_key(Bridge::key);
    let mut s = serde_json::to_string_pretty(&d).expect("design serializes");
    s.push('\n');
    s
}
