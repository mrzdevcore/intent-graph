//! The journal in `.ig/journal.json`: every spec edit with who made it, its inverse, whether the
//! person kept it, and whether it is undone. Losing it loses undo, never the spec.

use super::{Edit, apply};
use ig_core::ports::Files;
use serde::{Deserialize, Serialize};

pub const JOURNAL: &str = ".ig/journal.json";

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum By {
    Person,
    Claude,
    Reading,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    pub n: u64,
    pub by: By,
    pub kept: bool,
    pub undone: bool,
    pub edit: Edit,
    pub inverse: Edit,
}

/// The entries, and a notice when the journal exists but cannot be read (then it counts as
/// empty).
pub fn entries(files: &dyn Files) -> (Vec<Entry>, Option<String>) {
    let Some(bytes) = files.read_bytes(JOURNAL) else {
        return (vec![], None);
    };
    match serde_json::from_slice(&bytes) {
        Ok(v) => (v, None),
        Err(e) => (
            vec![],
            Some(format!(
                "{JOURNAL} cannot be read ({e}); starting an empty journal"
            )),
        ),
    }
}

fn save(files: &dyn Files, list: &[Entry]) -> Result<(), Vec<String>> {
    let bytes = serde_json::to_vec_pretty(list).map_err(|e| vec![e.to_string()])?;
    files.write_atomic(JOURNAL, &bytes).map_err(|e| {
        vec![format!(
            "the spec changed but the journal could not be saved: {e}"
        )]
    })
}

/// Applies `e` and appends it; undone entries are discarded (a new edit ends the redo line).
pub fn record(files: &dyn Files, by: By, e: &Edit) -> Result<Entry, Vec<String>> {
    let inverse = apply(files, e)?;
    let mut list = entries(files).0;
    list.retain(|x| !x.undone);
    let n = list.iter().map(|x| x.n).max().map_or(1, |m| m + 1);
    let entry = Entry {
        n,
        by,
        kept: by == By::Person,
        undone: false,
        edit: e.clone(),
        inverse,
    };
    list.push(entry.clone());
    save(files, &list)?;
    Ok(entry)
}

/// Marks entry `n` kept, or every entry not kept yet. Never touches the spec.
pub fn keep(files: &dyn Files, n: Option<u64>) -> Result<(), Vec<String>> {
    let mut list = entries(files).0;
    list.iter_mut()
        .filter(|x| n.is_none_or(|n| x.n == n))
        .for_each(|x| x.kept = true);
    save(files, &list)
}

/// Applies the inverse of entry `n` and removes the entry; on error nothing changes.
pub fn drop(files: &dyn Files, n: u64) -> Result<(), Vec<String>> {
    let mut list = entries(files).0;
    let i = list
        .iter()
        .position(|x| x.n == n)
        .ok_or_else(|| vec![format!("no journal entry {n}")])?;
    apply(files, &list[i].inverse)?;
    list.remove(i);
    save(files, &list)
}

/// Undoes the last entry not undone; `None` when there is nothing to undo.
pub fn undo(files: &dyn Files) -> Result<Option<Entry>, Vec<String>> {
    let mut list = entries(files).0;
    let Some(i) = list.iter().rposition(|x| !x.undone) else {
        return Ok(None);
    };
    apply(files, &list[i].inverse)?;
    list[i].undone = true;
    save(files, &list)?;
    Ok(Some(list[i].clone()))
}

/// Re-applies the entry undone last; `None` when there is nothing to redo.
pub fn redo(files: &dyn Files) -> Result<Option<Entry>, Vec<String>> {
    let mut list = entries(files).0;
    // Undone entries are always the tail (record discards them), so the one undone last is the
    // first of them.
    let Some(i) = list.iter().position(|x| x.undone) else {
        return Ok(None);
    };
    apply(files, &list[i].edit)?;
    list[i].undone = false;
    save(files, &list)?;
    Ok(Some(list[i].clone()))
}
