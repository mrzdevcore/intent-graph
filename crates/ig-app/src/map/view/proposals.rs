//! The journal's entries nobody kept yet, as one line each for the viewer's proposals bar.

use super::super::edit::Edit;
use super::super::edit::journal::{By, Entry};
use super::super::file::MapBox;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct Proposal {
    pub n: u64,
    pub by: By,
    pub summary: String,
    /// The box and arrow ids the edit touches, for highlighting.
    pub ids: Vec<String>,
}

fn word<T: Serialize>(v: &T) -> String {
    serde_json::to_value(v)
        .ok()
        .and_then(|v| v.as_str().map(String::from))
        .unwrap_or_default()
}

/// A line for each entry not kept and not undone; box titles where the view knows the box.
pub fn proposals(entries: &[Entry], boxes: &[MapBox]) -> Vec<Proposal> {
    let t = |id: &str| {
        boxes
            .iter()
            .find(|b| b.id == id)
            .map_or(id.to_string(), |b| b.title.value.clone())
    };
    let ids = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    (entries.iter().filter(|e| !e.kept && !e.undone))
        .map(|e| {
            let (summary, ids) = match &e.edit {
                Edit::Add {
                    kind, id, title, ..
                } => (format!("add {} {title}", word(kind)), ids(&[id])),
                Edit::Remove { id } => (format!("remove {}", t(id)), ids(&[id])),
                Edit::Move { id, parent, .. } => (
                    format!("move {} → {}", t(id), t(parent)),
                    ids(&[id, parent]),
                ),
                Edit::Rename { id, to } => (format!("rename {} → {to}", t(id)), ids(&[id, to])),
                Edit::Set { id, key, .. } => (format!("{}: {key}", t(id)), ids(&[id])),
                Edit::Arrow {
                    from,
                    to,
                    kind,
                    label,
                } => (
                    format!(
                        "arrow {} {} {}{}",
                        t(from),
                        word(kind),
                        t(to),
                        label.as_ref().map_or(String::new(), |l| format!(" {l}"))
                    ),
                    ids(&[&format!("{from}->{to}")]),
                ),
                Edit::Unarrow { from, to, kind } => (
                    format!("unarrow {} {} {}", t(from), word(kind), t(to)),
                    ids(&[&format!("{from}->{to}")]),
                ),
                Edit::Restore { parent, .. } => ("restore".into(), ids(&[parent])),
            };
            Proposal {
                n: e.n,
                by: e.by,
                summary,
                ids,
            }
        })
        .collect()
}
