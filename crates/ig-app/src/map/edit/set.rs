//! `set`: a property, `at`, or `kind`. `kind` renames the node in place (id, entries, children
//! and comments kept) and drops the properties the new kind does not take, which its inverse
//! puts back; it is refused when it would put a box out of place that was not.

use super::{Edit, PROPS, Spec, not_whole_repo, set_str};
use crate::map::file::{MapBox, MapFile};
use crate::map::kinds::Kinds;
use crate::map::spec::{self, DIR};
use crate::map::{BoxKind, warn};
use kdl::KdlEntry;
use std::collections::{BTreeMap, HashMap};

/// What every kind takes besides `PROPS`.
const COMMON: [&str; 1] = ["owner"];

impl Spec {
    /// `new` (path → text) read back with the team kinds.
    pub(super) fn parsed(&self, new: &BTreeMap<String, String>) -> Result<MapFile, Vec<String>> {
        let mut t: Vec<_> = (new.iter())
            .map(|(p, t)| (p.clone(), Some(t.clone())))
            .collect();
        t.extend(self.kinds.iter().cloned());
        spec::parse(t)
    }

    pub(super) fn set(
        &mut self,
        id: &str,
        key: &str,
        value: Option<&str>,
        restore: &[(usize, String)],
    ) -> Result<Edit, String> {
        match key {
            "kind" => self.set_kind(id, value, restore),
            "at" => self.set_at(id, value),
            _ => self.set_prop(id, key, value, restore),
        }
    }

    /// A property the box's kind takes; the read back checks its value. A removed one's
    /// inverse puts it back where it was (`restore`).
    fn set_prop(
        &mut self,
        id: &str,
        key: &str,
        value: Option<&str>,
        restore: &[(usize, String)],
    ) -> Result<Edit, String> {
        let loc = self.must(id)?;
        let (kinds, _) = Kinds::team(self.kinds.clone());
        let name = self.node(&loc).name().value().to_string();
        let own = kinds.get(&spec::read::new_name(&name).unwrap_or(name.clone()));
        let own: Vec<&str> = own
            .iter()
            .flat_map(|d| &d.props)
            .map(|p| p.name.as_str())
            .collect();
        if !PROPS.contains(&key) && !COMMON.contains(&key) && !own.contains(&key) {
            let all = [&PROPS[..], &COMMON, &own].concat().join(", ");
            return Err(format!(
                "`{key}` is not a property of a {name}: one of {all}, `kind` or `at`"
            ));
        }
        // the common properties in their order, the others after them
        let ord = PROPS.iter().position(|k| *k == key).unwrap_or(PROPS.len());
        let n = self.node_mut(&loc);
        let is_key = |e: &KdlEntry| e.name().is_some_and(|k| k.value() == key);
        let at = n.entries().iter().position(is_key);
        let old = at.and_then(|i| n.entries()[i].value().as_string().map(String::from));
        let was = at.map(|i| (i, n.entries()[i].to_string()));
        match (value, at.map(|i| &mut n.entries_mut()[i])) {
            (Some(v), Some(e)) => set_str(e, v),
            (Some(_), None) if !restore.is_empty() => put_back(n, restore)?,
            (Some(v), None) => {
                // after the id and the properties written before this one
                let i = n.entries().iter().rposition(|e| {
                    e.name()
                        .is_none_or(|k| PROPS.iter().position(|p| *p == k.value()) < Some(ord))
                });
                let mut e = KdlEntry::new_prop(key, v);
                set_str(&mut e, v);
                n.entries_mut().insert(i.map_or(0, |i| i + 1), e);
            }
            (None, _) => n.entries_mut().retain(|e| !is_key(e)),
        }
        Ok(Edit::Set {
            id: id.into(),
            key: key.into(),
            value: old,
            restore: was.filter(|_| value.is_none()).into_iter().collect(),
        })
    }

    fn set_at(&mut self, id: &str, value: Option<&str>) -> Result<Edit, String> {
        let loc = self.must(id)?;
        let globs: Vec<String> = (value.unwrap_or_default().split('\n'))
            .filter(|g| !g.is_empty())
            .map(String::from)
            .collect();
        if id != self.sys {
            not_whole_repo(id, &globs)?;
        }
        let nodes = (self.node(&loc).children())
            .map(|c| c.nodes())
            .unwrap_or_default();
        let i = nodes.iter().position(|n| n.name().value() == "at");
        let old = i.map(|i| {
            let e = nodes[i].entries().iter();
            e.filter_map(|e| e.value().as_string())
                .collect::<Vec<_>>()
                .join("\n")
        });
        let mut at = spec::strings("at", &globs);
        spec::quote(std::slice::from_mut(&mut at));
        match (i, globs.is_empty()) {
            (Some(i), false) => {
                let (p, mut path) = loc;
                path.push(i);
                *self.node_mut(&(p, path)).entries_mut() = at.entries().to_vec();
            }
            (Some(i), true) => {
                let (p, mut path) = loc;
                path.push(i);
                self.take(&(p, path));
            }
            (None, false) => self.put(&loc, 0, at),
            (None, true) => {}
        }
        Ok(Edit::Set {
            id: id.into(),
            key: "at".into(),
            value: old,
            restore: vec![],
        })
    }

    fn set_kind(
        &mut self,
        id: &str,
        to: Option<&str>,
        restore: &[(usize, String)],
    ) -> Result<Edit, String> {
        let to = to.ok_or("a box keeps a kind; `kind` cannot be removed")?;
        let new = spec::read::new_name(to);
        let (kinds, _) = Kinds::team(self.kinds.clone());
        let def = (kinds.get(new.as_deref().unwrap_or(to)))
            .ok_or_else(|| format!("`{to}` is not a kind (no {DIR}/kinds/{to}.kdl)"))?;
        if id == self.sys || def.base == BoxKind::System {
            return Err(format!("`{id}` cannot become a `system` or stop being one"));
        }
        let now = (self.docs.iter())
            .map(|(p, d)| (p.clone(), d.to_string()))
            .collect();
        let m = self.parsed(&now).map_err(|e| e.join("; "))?;
        let by: HashMap<&str, &MapBox> = m.boxes.iter().map(|b| (b.id.as_str(), b)).collect();
        let b = by.get(id).ok_or_else(|| format!("no box `{id}`"))?;
        // never worse: a box already out of place may stay so, and an undo may put it back
        if !self.undoing
            && warn::misplaced(&by, b, b.kind.value, "").is_none()
            && let Some(why) = warn::misplaced(&by, b, def.base, &def.name)
        {
            return Err(why);
        }
        let takes = |e: &KdlEntry| {
            e.name().is_none_or(|k| {
                let k = k.value();
                PROPS.contains(&k) || COMMON.contains(&k) || def.props.iter().any(|p| p.name == k)
            })
        };
        let loc = self.must(id)?;
        let n = self.node_mut(&loc);
        let old = n.name().value().to_string();
        let dropped: Vec<(usize, String)> = (n.entries().iter().enumerate())
            .filter(|(_, e)| !takes(e))
            .map(|(i, e)| (i, e.to_string()))
            .collect();
        n.entries_mut().retain(takes);
        n.set_name(to);
        put_back(n, restore)?;
        Ok(Edit::Set {
            id: id.into(),
            key: "kind".into(),
            value: Some(old),
            restore: dropped,
        })
    }
}

/// Puts entries back at their index, in order.
fn put_back(n: &mut kdl::KdlNode, restore: &[(usize, String)]) -> Result<(), String> {
    for (i, text) in restore {
        let e: KdlEntry = (text.parse()).map_err(|_| format!("cannot read `{text}` back"))?;
        let i = (*i).min(n.entries().len());
        n.entries_mut().insert(i, e);
    }
    Ok(())
}
