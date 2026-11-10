//! Edits to the spec in `ig/`, made in place: the nodes an edit touches change, every other line
//! keeps its text. Nothing is written unless the whole edited spec reads back; each edit returns
//! its inverse, which gives the files back byte for byte.

use super::ArrowKind;
use super::spec::{self, DIR};
use ig_core::ports::Files;
use kdl::{KdlDocument, KdlEntry, KdlEntryFormat, KdlNode};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

mod arrow;
pub(crate) mod fmt;
pub mod journal;
mod moves;
pub(super) mod set;
pub(crate) mod tidy;
pub use fmt::fmt;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "lowercase")]
pub enum Edit {
    /// `kind` names a built-in kind or a team's (`ig/kinds/<name>.kdl`); `props` are the kind's
    /// own properties (`provider`, `hosted`…), written after the title.
    Add {
        parent: String,
        kind: String,
        id: String,
        title: String,
        at: Vec<String>,
        before: Option<String>,
        #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
        props: BTreeMap<String, String>,
    },
    Remove {
        id: String,
    },
    /// The id stays: arrows pointing at it still resolve. A slot the new parent does not offer
    /// is cleared; `slot` (only in an inverse) is set after the move.
    Move {
        id: String,
        parent: String,
        before: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        slot: Option<String>,
    },
    /// The id; arrows pointing at it follow. A container's components keep theirs.
    Rename {
        id: String,
        to: String,
    },
    /// A property (`title`, `tech`, `description`, `logo`, `slot`, `owner`, or one of the box's
    /// kind's own, as the read takes it), or `at` (globs joined by `\n`); `None` removes it.
    /// `kind` renames the node in place, refused where it would put the box out of place, and
    /// drops the properties the new kind does not take; `restore` (only in an inverse) puts
    /// those back: each its index among the node's entries and its text.
    Set {
        id: String,
        key: String,
        value: Option<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        restore: Vec<(usize, String)>,
    },
    Arrow {
        from: String,
        to: String,
        kind: ArrowKind,
        label: Option<String>,
    },
    Unarrow {
        from: String,
        to: String,
        kind: ArrowKind,
    },
    ArrowSet {
        from: String,
        to: String,
        key: String,
        value: Option<String>,
        // inverse of from and flip: the sibling the arrow was before
        #[serde(default, skip_serializing_if = "Option::is_none")]
        before: Option<String>,
        // inverse of label, note, context: the entry's index and original text
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        restore: Vec<(usize, String)>,
    },
    /// Puts back a removed node verbatim; only made as an inverse. `before` names a sibling box
    /// by id, or a sibling arrow by `from->to`.
    Restore {
        parent: String,
        before: Option<String>,
        kdl: String,
    },
}

/// Applies `e` to the spec in `files` and writes the files it changes (`ig/system.kdl` last);
/// returns the edit that undoes it.
pub fn apply(files: &dyn Files, e: &Edit) -> Result<Edit, Vec<String>> {
    apply_as(files, e, false)
}

/// `apply`; `undoing` (an inverse, from the journal) may put a box back out of place.
fn apply_as(files: &dyn Files, e: &Edit, undoing: bool) -> Result<Edit, Vec<String>> {
    let mut s = Spec::load(files)?;
    s.undoing = undoing;
    let inverse = s.edit(e).map_err(|e| vec![e])?;
    s.save(files)?;
    Ok(inverse)
}

/// Errs when `at` claims `**`, the whole repo, for box `id`: only the system does, or later
/// drift would hide in the box. Every edit and `ig accept` go through it.
pub fn not_whole_repo(id: &str, at: &[String]) -> Result<(), String> {
    match at.iter().any(|g| g == "**") {
        true => Err(format!(
            "`{id}` claims `**`, the whole repo; only the system does"
        )),
        false => Ok(()),
    }
}

const PROPS: [&str; 5] = ["title", "tech", "description", "logo", "slot"];

fn system_path() -> String {
    format!("{DIR}/system.kdl")
}

fn named<T: DeserializeOwned>(s: &str) -> Option<T> {
    serde_json::from_value(serde_json::Value::String(s.into())).ok()
}

fn name_of(v: impl Serialize) -> String {
    let v = serde_json::to_value(v).ok();
    v.as_ref()
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .into()
}

fn first(n: &KdlNode) -> Option<&str> {
    n.entries().first()?.value().as_string()
}

/// As the reader takes them: every node not an arrow nor `at`, `badges` or `pattern` is a box
/// (a team kind's among them).
fn box_id(n: &KdlNode) -> Option<&str> {
    let k = n.name().value();
    if named::<ArrowKind>(k).is_some() || ["at", "badges", "pattern", "context"].contains(&k) {
        return None;
    }
    first(n)
}

fn arrow(n: &KdlNode) -> Option<(ArrowKind, &str)> {
    Some((named(n.name().value())?, first(n)?))
}

/// How `before` names `n`, a node in `parent`'s block.
fn sibling_id(parent: &str, n: &KdlNode) -> Option<String> {
    box_id(n)
        .map(String::from)
        .or_else(|| arrow(n).map(|(_, to)| format!("{parent}->{to}")))
}

fn set_str(e: &mut KdlEntry, v: &str) {
    e.set_value(v);
    e.set_format(KdlEntryFormat {
        value_repr: spec::repr(v),
        leading: e.format().map_or(" ".into(), |f| f.leading.clone()),
        ..Default::default()
    });
}

/// The box ids written in `d`.
fn ids(d: &KdlDocument) -> std::collections::BTreeSet<String> {
    let mut out = std::collections::BTreeSet::new();
    for n in d.nodes() {
        if let Some(id) = box_id(n) {
            out.insert(id.to_string());
            out.extend(n.children().map(ids).unwrap_or_default());
        }
    }
    out
}

/// Where a node is: its file and the child indices down to it.
type Loc = (String, Vec<usize>);

struct Spec {
    sys: String,
    /// The files as edited; a file gone from here is deleted.
    docs: BTreeMap<String, KdlDocument>,
    /// The files as read.
    texts: BTreeMap<String, String>,
    /// The team kinds, `ig/kinds/*.kdl`, as read; never edited.
    kinds: Vec<(String, Option<String>)>,
    /// Applying an inverse: `set kind` may restore a misplacement.
    undoing: bool,
}

impl Spec {
    fn load(files: &dyn Files) -> Result<Spec, Vec<String>> {
        let (kinds, read) = spec::read::texts(files)?;
        let mut docs = BTreeMap::new();
        let mut texts = BTreeMap::new();
        for (p, t) in &read {
            let Some(d) = t.as_ref().and_then(|t| t.parse::<KdlDocument>().ok()) else {
                return Err(spec::parse([read, kinds].concat())
                    .err()
                    .unwrap_or_default());
            };
            docs.insert(p.clone(), d);
            texts.insert(p.clone(), t.clone().unwrap_or_default());
        }
        let sys = docs
            .get(&system_path())
            .and_then(|d| d.nodes().iter().find(|n| n.name().value() == "system"))
            .and_then(first)
            .ok_or_else(|| vec![format!("{DIR}/system.kdl:1: no system node")])?
            .to_string();
        Ok(Spec {
            sys,
            docs,
            texts,
            kinds,
            undoing: false,
        })
    }

    /// Reads the edited spec back in memory, then changes the files so that a crash part way
    /// leaves a node written twice, never lost: every write first (the files that gain a box,
    /// new files among them, then the files that only lose boxes or change, then
    /// `ig/system.kdl` last of the writes), and the deletions after every write.
    fn save(self, files: &dyn Files) -> Result<(), Vec<String>> {
        let new: BTreeMap<String, String> = (self.docs.iter())
            .map(|(p, d)| (p.clone(), d.to_string()))
            .collect();
        self.parsed(&new)?;
        let sys = system_path();
        let old_ids = |p: &str| {
            let d = self
                .texts
                .get(p)
                .and_then(|t| t.parse::<KdlDocument>().ok());
            d.map(|d| ids(&d)).unwrap_or_default()
        };
        let (mut gain, mut lose) = (vec![], vec![]);
        for (p, t) in &new {
            if *p == sys || self.texts.get(p) == Some(t) {
                continue;
            }
            let old = old_ids(p);
            match ids(&self.docs[p]).iter().all(|id| old.contains(id)) {
                true => lose.push(p),
                false => gain.push(p),
            }
        }
        let write = |p: &String| {
            files
                .write_atomic(p, new[p].as_bytes())
                .map_err(|e| vec![format!("{p}: {e}")])
        };
        for p in gain.into_iter().chain(lose) {
            write(p)?;
        }
        if new.get(&sys) != self.texts.get(&sys) {
            write(&sys)?;
        }
        for p in self.texts.keys().filter(|p| !new.contains_key(*p)) {
            files.remove(p).map_err(|e| vec![format!("{p}: {e}")])?;
        }
        Ok(())
    }

    fn edit(&mut self, e: &Edit) -> Result<Edit, String> {
        match e {
            Edit::Add {
                parent,
                kind,
                id,
                title,
                at,
                before,
                props,
            } => {
                self.unused(id)?;
                not_whole_repo(id, at)?;
                let mut n = KdlNode::new(kind.as_str());
                n.push(KdlEntry::new(id.as_str()));
                n.push(KdlEntry::new_prop("title", title.as_str()));
                for (k, v) in props {
                    n.push(KdlEntry::new_prop(k.as_str(), v.as_str()));
                }
                if !at.is_empty() {
                    n.ensure_children()
                        .nodes_mut()
                        .push(spec::strings("at", at));
                }
                spec::quote(std::slice::from_mut(&mut n));
                let (loc, i) = self.place(parent, before.as_deref())?;
                self.put(&loc, i, n);
                self.settle(parent)?;
                Ok(Edit::Remove { id: id.clone() })
            }
            Edit::Remove { id } => {
                let loc = self.must(id)?;
                let mut why = vec![];
                let kids = self.kids(id);
                if !kids.is_empty() {
                    why.push(format!("`{id}` has children: {}", kids.join(", ")));
                }
                let into: Vec<String> = (self.arrows().into_iter())
                    .filter(|(f, t)| t == id && f != id)
                    .map(|(f, t)| format!("{f}->{t}"))
                    .collect();
                if !into.is_empty() {
                    why.push(format!("arrows point at `{id}`: {}", into.join(", ")));
                }
                if !why.is_empty() {
                    return Err(format!("{}; remove those first", why.join("; ")));
                }
                let parent = self.parent(&loc).ok_or("the system cannot be removed")?;
                let (n, before) = self.take(&loc);
                self.settle(&parent)?;
                Ok(Edit::Restore {
                    parent,
                    before,
                    kdl: n.to_string(),
                })
            }
            Edit::Restore {
                parent,
                before,
                kdl,
            } => {
                let mut d: KdlDocument =
                    kdl.parse().map_err(|_| "cannot read the node to restore")?;
                if d.nodes().len() != 1 {
                    return Err("restore takes exactly one node".into());
                }
                let n = d.nodes_mut().remove(0);
                let inverse = if let Some(id) = box_id(&n) {
                    self.unused(id)?;
                    Edit::Remove { id: id.into() }
                } else if let Some((kind, to)) = arrow(&n) {
                    Edit::Unarrow {
                        from: parent.clone(),
                        to: to.into(),
                        kind,
                    }
                } else {
                    return Err("restore takes a box or an arrow".into());
                };
                let (loc, i) = self.place(parent, before.as_deref())?;
                self.put(&loc, i, n);
                self.settle(parent)?;
                Ok(inverse)
            }
            Edit::Move {
                id,
                parent,
                before,
                slot,
            } => self.mv(id, parent, before.as_deref(), slot.as_deref()),
            Edit::Rename { id, to } => {
                let loc = self.must(id)?;
                self.unused(to)?;
                self.rekey(id, to);
                let path = spec::path(to);
                if self.own_file(&loc) && loc.0 == spec::path(id) && !self.docs.contains_key(&path)
                {
                    let d = self.docs.remove(&loc.0).unwrap_or_default();
                    self.docs.insert(path, d);
                }
                Ok(Edit::Rename {
                    id: to.clone(),
                    to: id.clone(),
                })
            }
            Edit::Set {
                id,
                key,
                value,
                restore,
            } => self.set(id, key, value.as_deref(), restore),
            Edit::Arrow {
                from,
                to,
                kind,
                label,
            } => {
                let loc = self.must(from)?;
                let mut n = KdlNode::new(name_of(kind));
                n.push(KdlEntry::new(to.as_str()));
                if let Some(l) = label {
                    n.push(KdlEntry::new_prop("label", l.as_str()));
                }
                spec::quote(std::slice::from_mut(&mut n));
                // after the arrows, before the child boxes
                let block = self.node(&loc).children();
                let nodes = block.map(|c| c.nodes()).unwrap_or_default();
                let i = nodes.iter().position(|n| box_id(n).is_some());
                self.put(&loc, i.unwrap_or(nodes.len()), n);
                Ok(Edit::Unarrow {
                    from: from.clone(),
                    to: to.clone(),
                    kind: *kind,
                })
            }
            Edit::ArrowSet {
                from,
                to,
                key,
                value,
                before,
                restore,
            } => self.arrow_set(from, to, key, value.as_deref(), before.as_deref(), restore),
            Edit::Unarrow { from, to, kind } => {
                let (path, mut at) = self.must(from)?;
                let nodes = (self.node(&(path.clone(), at.clone())).children())
                    .map(|c| c.nodes())
                    .unwrap_or_default();
                let i = (nodes
                    .iter()
                    .position(|n| arrow(n) == Some((*kind, to.as_str()))))
                .ok_or_else(|| format!("no {} arrow `{from}->{to}`", name_of(kind)))?;
                at.push(i);
                let (n, before) = self.take(&(path, at));
                Ok(Edit::Restore {
                    parent: from.clone(),
                    before,
                    kdl: n.to_string(),
                })
            }
        }
    }

    fn find(&self, id: &str) -> Option<Loc> {
        fn walk(d: &KdlDocument, id: &str, at: &mut Vec<usize>) -> bool {
            for (i, n) in d.nodes().iter().enumerate() {
                let Some(b) = box_id(n) else { continue };
                at.push(i);
                if b == id || n.children().is_some_and(|c| walk(c, id, at)) {
                    return true;
                }
                at.pop();
            }
            false
        }
        self.docs.iter().find_map(|(p, d)| {
            let mut at = vec![];
            walk(d, id, &mut at).then(|| (p.clone(), at))
        })
    }

    fn must(&self, id: &str) -> Result<Loc, String> {
        self.find(id).ok_or_else(|| format!("no box `{id}`"))
    }

    fn unused(&self, id: &str) -> Result<(), String> {
        match self.find(id) {
            Some(_) => Err(format!("id `{id}` is already used")),
            None => Ok(()),
        }
    }

    fn node(&self, (p, at): &Loc) -> &KdlNode {
        let mut n = &self.docs[p].nodes()[at[0]];
        for &i in &at[1..] {
            n = &n.children().expect("a located node's block").nodes()[i];
        }
        n
    }

    fn node_mut(&mut self, (p, at): &Loc) -> &mut KdlNode {
        let mut n = &mut self.docs.get_mut(p).expect("a located file").nodes_mut()[at[0]];
        for &i in &at[1..] {
            n = &mut n
                .children_mut()
                .as_mut()
                .expect("a located node's block")
                .nodes_mut()[i];
        }
        n
    }

    /// The top node of a file other than `system.kdl`: a container with a file of its own.
    fn own_file(&self, (p, at): &Loc) -> bool {
        at.len() == 1 && *p != system_path()
    }

    fn parent(&self, loc: &Loc) -> Option<String> {
        let (p, at) = loc;
        if at.len() > 1 {
            let up = (p.clone(), at[..at.len() - 1].to_vec());
            return box_id(self.node(&up)).map(String::from);
        }
        self.own_file(loc).then(|| self.sys.clone())
    }

    /// The child boxes of `id`, wherever they are written.
    fn kids(&self, id: &str) -> Vec<String> {
        let loc = self.find(id);
        let block = loc.as_ref().and_then(|l| self.node(l).children());
        let mut v: Vec<String> = (block.map(|c| c.nodes()).unwrap_or_default().iter())
            .filter_map(|n| box_id(n).map(String::from))
            .collect();
        if id == self.sys {
            let own = self.docs.iter().filter(|(p, _)| **p != system_path());
            v.extend(own.filter_map(|(_, d)| d.nodes().first().and_then(box_id).map(String::from)));
        }
        v
    }

    /// Every arrow as (from, to).
    fn arrows(&self) -> Vec<(String, String)> {
        fn walk(d: &KdlDocument, from: &str, out: &mut Vec<(String, String)>) {
            for n in d.nodes() {
                if let Some((_, to)) = arrow(n) {
                    out.push((from.into(), to.into()));
                } else if let (Some(b), Some(c)) = (box_id(n), n.children()) {
                    walk(c, b, out);
                }
            }
        }
        let mut out = vec![];
        for d in self.docs.values() {
            walk(d, "", &mut out);
        }
        out
    }

    /// Renames box `old` to `new` and the arrows pointing at it.
    fn rekey(&mut self, old: &str, new: &str) {
        fn walk(d: &mut KdlDocument, re: &dyn Fn(&str) -> Option<String>) {
            for n in d.nodes_mut() {
                let is_box = box_id(n).is_some();
                if (is_box || arrow(n).is_some())
                    && let Some(v) = first(n).and_then(re)
                {
                    set_str(&mut n.entries_mut()[0], &v);
                }
                if let Some(c) = n.children_mut().as_mut().filter(|_| is_box) {
                    walk(c, re);
                }
            }
        }
        let re = |s: &str| (s == old).then(|| new.to_string());
        for d in self.docs.values_mut() {
            walk(d, &re);
        }
    }
}

#[cfg(test)]
mod tests;
