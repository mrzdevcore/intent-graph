//! `ig/*.kdl`: the map written as the committed spec of the intended architecture. The system
//! and its small containers share `ig/system.kdl`; each container with children of its own gets
//! a file. Facts that are not intent (sources, pins, file counts, marks) are not written.

use super::file::{MapBox, MapFile};
use super::{ArrowKind, Level, MapArrow, Pattern};
use kdl::{KdlDocument, KdlEntry, KdlEntryFormat, KdlNode};
use std::collections::BTreeMap;

/// The spec directory, relative to the ig root.
pub const DIR: &str = "ig";
const HEADER: &str = "// ig's intended architecture: edit freely; ig never rewrites this file.\n";

/// The spec files for `m`: relative path to text. Code-level boxes are not written.
pub fn write(m: &MapFile) -> BTreeMap<String, String> {
    let mut files = BTreeMap::new();
    let mut roots: Vec<&MapBox> = m
        .boxes
        .iter()
        .filter(|b| b.parent.is_none() && b.level != Level::Code)
        .collect();
    roots.sort_by_key(|b| (b.level != Level::System, b.id.as_str()));
    for (i, root) in roots.into_iter().enumerate() {
        // The first root is the system and owns system.kdl; any other root gets its own file.
        let first = i == 0 && root.level == Level::System;
        let n = node(m, root, &mut |child, n| {
            if first && has_kids(m, child) {
                files.insert(path(&child.id), text(vec![n]));
                return None;
            }
            Some(n)
        });
        files.insert(path(if first { "system" } else { &root.id }), text(vec![n]));
    }
    files
}

fn path(id: &str) -> String {
    format!("{DIR}/{}.kdl", id.replace('/', "-"))
}

fn kids<'a>(m: &'a MapFile, b: &MapBox) -> Vec<&'a MapBox> {
    let mut v: Vec<_> = m
        .boxes
        .iter()
        .filter(|c| c.parent.as_deref() == Some(&b.id) && c.level != Level::Code)
        .collect();
    v.sort_by(|a, b| a.id.cmp(&b.id));
    v
}

fn has_kids(m: &MapFile, b: &MapBox) -> bool {
    !kids(m, b).is_empty()
}

fn text(nodes: Vec<KdlNode>) -> String {
    let mut doc = KdlDocument::new();
    doc.nodes_mut().extend(nodes);
    doc.autoformat();
    quote(doc.nodes_mut());
    let mut text = doc.to_string();
    text.insert_str(0, HEADER);
    text
}

/// Autoformat writes plain words bare; the spec quotes every string so that it reads one way.
fn quote(nodes: &mut [KdlNode]) {
    for n in nodes {
        for e in n.entries_mut() {
            let Some(s) = e.value().as_string().map(str::to_string) else {
                continue;
            };
            let mut esc = String::new();
            for c in s.chars() {
                match c {
                    '\\' => esc.push_str("\\\\"),
                    '"' => esc.push_str("\\\""),
                    '\n' => esc.push_str("\\n"),
                    '\r' => esc.push_str("\\r"),
                    '\t' => esc.push_str("\\t"),
                    c if c.is_control() || disallowed(c) => {
                        esc.push_str(&format!("\\u{{{:04X}}}", c as u32))
                    }
                    c => esc.push(c),
                }
            }
            e.set_format(KdlEntryFormat {
                value_repr: format!("\"{esc}\""),
                leading: " ".into(),
                ..Default::default()
            });
        }
        if let Some(c) = n.children_mut() {
            quote(c.nodes_mut());
        }
    }
}

/// Code points KDL forbids in a document (bidi controls, the BOM) or reads as a newline.
fn disallowed(c: char) -> bool {
    matches!(c, '\u{200E}' | '\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}' | '\u{FEFF}' | '\u{2028}' | '\u{2029}')
}

/// A box as a node; `split` sees each direct child's node and may take it (to a file of its own).
fn node(
    m: &MapFile,
    b: &MapBox,
    split: &mut dyn FnMut(&MapBox, KdlNode) -> Option<KdlNode>,
) -> KdlNode {
    let kind = serde_json::to_value(b.kind.value).ok();
    let mut n = KdlNode::new(kind.as_ref().and_then(|k| k.as_str()).unwrap_or("box"));
    n.push(KdlEntry::new(b.id.as_str()));
    n.push(KdlEntry::new_prop("title", b.title.value.as_str()));
    let opt = [
        ("tech", &b.tech),
        ("description", &b.description),
        ("logo", &b.logo),
        ("slot", &b.slot),
    ];
    for (key, f) in opt {
        if let Some(f) = f {
            n.push(KdlEntry::new_prop(key, f.value.as_str()));
        }
    }
    let body = n.ensure_children();
    if !b.made_of.is_empty() {
        body.nodes_mut().push(strings("at", &narrow(b)));
    }
    if let Some(f) = b.badges.as_ref().filter(|f| !f.value.is_empty()) {
        body.nodes_mut().push(strings("badges", &f.value));
    }
    if let Some(f) = &b.pattern {
        body.nodes_mut().push(pattern(&f.value));
    }
    let mut out: Vec<&MapArrow> = m.arrows.iter().filter(|a| a.from == b.id).collect();
    out.sort_by(|a, b| a.id.cmp(&b.id));
    for a in out {
        // A bundle is derived from its kinds: the spec says the main one.
        let kind = match a.kind {
            ArrowKind::Bundle => a.kinds.first().copied().unwrap_or(ArrowKind::Uses),
            k => k,
        };
        let name = serde_json::to_value(kind).ok();
        let mut an = KdlNode::new(name.as_ref().and_then(|k| k.as_str()).unwrap_or("uses"));
        an.push(KdlEntry::new(a.to.as_str()));
        // A bundle's label is a derived count, not intent.
        if let Some(l) = a.label.as_ref().filter(|_| a.kind != ArrowKind::Bundle) {
            an.push(KdlEntry::new_prop("label", l.as_str()));
        }
        body.nodes_mut().push(an);
    }
    for c in kids(m, b) {
        let cn = node(m, c, &mut |_, n| Some(n));
        if let Some(cn) = split(c, cn) {
            body.nodes_mut().push(cn);
        }
    }
    if body.nodes().is_empty() {
        n.clear_children();
    }
    n
}

/// A box (not the system) at `**`, a package at the repo root, is written at the top-level
/// folders holding its files (`*` for loose root files), so that new code elsewhere is drift.
fn narrow(b: &MapBox) -> Vec<String> {
    if b.level == Level::System || b.made_of != ["**"] || b.nodes.is_empty() {
        return b.made_of.clone();
    }
    let dirs: std::collections::BTreeSet<String> = (b.nodes.iter())
        .map(|n| match n.split_once('/') {
            Some((d, _)) => format!("{d}/**"),
            None => "*".into(),
        })
        .collect();
    dirs.into_iter().collect()
}

fn strings(name: &str, vals: &[String]) -> KdlNode {
    let mut n = KdlNode::new(name);
    for v in vals {
        n.push(KdlEntry::new(v.as_str()));
    }
    n
}

fn pattern(p: &Pattern) -> KdlNode {
    match p {
        Pattern::Onion => strings("pattern", &["onion".into()]),
        Pattern::Hexagonal => strings("pattern", &["hexagonal".into()]),
        Pattern::None => strings("pattern", &["none".into()]),
        Pattern::Layered { bands } => strings(
            "pattern",
            &[vec!["layered".to_string()], bands.clone()].concat(),
        ),
    }
}

mod read;
pub use read::{parse, read};

#[cfg(test)]
mod tests;
