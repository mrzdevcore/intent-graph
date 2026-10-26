//! Byte caps on what an edit writes: shared by the board engine and `design_edit` via `apply`.

use crate::{Design, DesignError, Element, ElementKind, Scope};

/// Byte caps on what an edit writes (the board and `design_edit` alike).
pub const MAX_NAME: usize = 200;
pub const MAX_FILE: usize = 512;
pub const MAX_TEXT: usize = 4096;
pub const MAX_ID: usize = 512;

fn cap(what: &str, s: &str, max: usize) -> Result<(), DesignError> {
    if s.len() > max {
        return Err(DesignError::Invalid(format!(
            "{what} is longer than {max} bytes"
        )));
    }
    Ok(())
}

/// The caps, on the fields the edit changed only: an old over-long value never blocks an edit.
pub(crate) fn caps(old: &Design, d: &Design) -> Result<(), DesignError> {
    type Get = fn(&Element) -> Option<&str>;
    let was: std::collections::HashMap<&str, &Element> =
        old.elements.iter().map(|e| (e.id.as_str(), e)).collect();
    for e in &d.elements {
        let o = was.get(e.id.as_str());
        if o.is_some_and(|o| *o == e) {
            continue;
        }
        let id = &e.id;
        let name = if e.kind == ElementKind::Note {
            MAX_TEXT
        } else {
            MAX_NAME
        };
        let fields: [(&str, Get, usize); 10] = [
            ("the id", |e| Some(&e.id), MAX_ID),
            ("the name", |e| Some(&e.name), name),
            ("the answer", |e| e.answer.as_deref(), MAX_TEXT),
            ("file", |e| e.file.as_deref(), MAX_FILE),
            ("path", |e| e.path.as_deref(), MAX_FILE),
            ("method", |e| e.method.as_deref(), MAX_ID),
            ("icon", |e| e.icon.as_deref(), MAX_ID),
            ("layer", |e| e.layer.as_deref(), MAX_ID),
            ("on", |e| e.on.as_deref(), MAX_ID),
            ("frame", |e| e.frame.as_deref(), MAX_ID),
        ];
        for (f, get, max) in fields {
            // checked when it is new or differs from before
            if let Some(v) = get(e).filter(|v| o.is_none_or(|o| get(o) != Some(*v))) {
                let what = if f == "the id" {
                    "an id".into()
                } else {
                    format!("{id}: {f}")
                };
                cap(&what, v, max)?;
            }
        }
    }
    for l in d.links.iter().filter(|l| !old.links.contains(l)) {
        cap("a link end", &l.from, MAX_ID)?;
        cap("a link end", &l.to, MAX_ID)?;
    }
    for b in d.bridges.iter().filter(|b| !old.bridges.contains(b)) {
        cap("a bridge end", &b.from, MAX_ID)?;
        cap("a bridge end", &b.to, MAX_ID)?;
    }
    for m in d
        .marks
        .iter()
        .filter(|m| !old.marks.iter().any(|x| x.node == m.node))
    {
        cap("a marked node", &m.node, MAX_ID)?;
    }
    if d.scope != old.scope {
        let (Scope::Dir { id } | Scope::Layer { name: id } | Scope::Around { id, .. }) = &d.scope;
        cap("the scope", id, MAX_ID)?;
    }
    if d.title != old.title {
        cap("the title", &d.title, MAX_NAME)?;
    }
    Ok(())
}
