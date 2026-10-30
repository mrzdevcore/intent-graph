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

/// What the file holds. `next` is the number the next entry gets: it only grows, so a number
/// is never reused, not even after a drop or an undo.
#[derive(Default, Serialize, Deserialize)]
struct Journal {
    next: u64,
    entries: Vec<Entry>,
}

impl Journal {
    fn take(&mut self) -> u64 {
        let n = self
            .next
            .max(self.entries.iter().map(|x| x.n + 1).max().unwrap_or(0))
            .max(1);
        self.next = n + 1;
        n
    }
}

fn read(files: &dyn Files) -> (Journal, Option<String>, Option<Vec<u8>>) {
    let Some(bytes) = files.read_bytes(JOURNAL) else {
        return (Journal::default(), None, None);
    };
    match serde_json::from_slice(&bytes) {
        Ok(j) => (j, None, None),
        Err(e) => (
            Journal::default(),
            Some(format!(
                "{JOURNAL} cannot be read ({e}); starting an empty journal"
            )),
            Some(bytes),
        ),
    }
}

/// The entries, and a notice when the journal exists but cannot be read (then it counts as
/// empty).
pub fn entries(files: &dyn Files) -> (Vec<Entry>, Option<String>) {
    let (j, notice, _) = read(files);
    (j.entries, notice)
}

/// For a change: a corrupt journal is copied to `.ig/journal.json.bad` before it is overwritten.
fn load(files: &dyn Files) -> Result<Journal, Vec<String>> {
    let (j, _, bad) = read(files);
    if let Some(bytes) = bad {
        files
            .write_atomic(&format!("{JOURNAL}.bad"), &bytes)
            .map_err(|e| vec![format!("cannot keep the unreadable journal: {e}")])?;
    }
    Ok(j)
}

fn save(files: &dyn Files, j: &Journal) -> Result<(), Vec<String>> {
    let bytes = serde_json::to_vec_pretty(j).map_err(|e| vec![e.to_string()])?;
    files.write_atomic(JOURNAL, &bytes).map_err(|e| {
        vec![format!(
            "the spec changed but the journal could not be saved: {e}"
        )]
    })
}

/// Applies `e` and appends it; undone entries are discarded (a new edit ends the redo line).
pub fn record(files: &dyn Files, by: By, e: &Edit) -> Result<Entry, Vec<String>> {
    let mut j = load(files)?;
    let inverse = apply(files, e)?;
    j.entries.retain(|x| !x.undone);
    let n = j.take();
    let entry = Entry {
        n,
        by,
        kept: by == By::Person,
        undone: false,
        edit: e.clone(),
        inverse,
    };
    j.entries.push(entry.clone());
    save(files, &j)?;
    Ok(entry)
}

/// Marks entry `n` kept, or every entry not kept yet. Never touches the spec.
pub fn keep(files: &dyn Files, n: Option<u64>) -> Result<(), Vec<String>> {
    let mut j = load(files)?;
    j.entries
        .iter_mut()
        .filter(|x| n.is_none_or(|n| x.n == n))
        .for_each(|x| x.kept = true);
    save(files, &j)
}

/// Applies the inverse of entry `n` (unless it is undone, then the spec is already back) and
/// removes the entry; on error nothing changes.
pub fn drop(files: &dyn Files, n: u64) -> Result<(), Vec<String>> {
    let mut j = load(files)?;
    let i = j
        .entries
        .iter()
        .position(|x| x.n == n)
        .ok_or_else(|| vec![format!("no journal entry {n}")])?;
    if !j.entries[i].undone {
        apply(files, &j.entries[i].inverse)?;
    }
    j.entries.remove(i);
    save(files, &j)
}

/// Undoes the last entry not undone; `None` when there is nothing to undo.
pub fn undo(files: &dyn Files) -> Result<Option<Entry>, Vec<String>> {
    let mut j = load(files)?;
    let Some(i) = j.entries.iter().rposition(|x| !x.undone) else {
        return Ok(None);
    };
    apply(files, &j.entries[i].inverse)?;
    j.entries[i].undone = true;
    save(files, &j)?;
    Ok(Some(j.entries[i].clone()))
}

/// Re-applies the entry undone last; `None` when there is nothing to redo.
pub fn redo(files: &dyn Files) -> Result<Option<Entry>, Vec<String>> {
    let mut j = load(files)?;
    // Undone entries are always the tail (record discards them), so the one undone last is the
    // first of them.
    let Some(i) = j.entries.iter().position(|x| x.undone) else {
        return Ok(None);
    };
    apply(files, &j.entries[i].edit)?;
    j.entries[i].undone = false;
    save(files, &j)?;
    Ok(Some(j.entries[i].clone()))
}
