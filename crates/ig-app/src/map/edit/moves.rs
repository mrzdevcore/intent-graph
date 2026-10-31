//! `Edit::Move`: a box to another parent, its slot cleared when the new parent does not offer it.

use super::{Edit, Spec};
use crate::map::file::slots;
use crate::map::spec;

impl Spec {
    pub(super) fn mv(
        &mut self,
        id: &str,
        parent: &str,
        before: Option<&str>,
        slot: Option<&str>,
    ) -> Result<Edit, String> {
        let loc = self.must(id)?;
        self.must(parent)?;
        let mut up = Some(parent.to_string());
        while let Some(p) = up {
            if p == id {
                return Err(format!("cannot move `{id}` into itself or below it"));
            }
            up = self.find(&p).and_then(|l| self.parent(&l));
        }
        let old = self.parent(&loc).ok_or("the system cannot be moved")?;
        let (n, was) = self.take(&loc);
        let (loc, i) = self.place(parent, before)?;
        self.put(&loc, i, n);
        let mut cleared = None;
        match slot {
            Some(s) => {
                self.set(id, "slot", Some(s), &[])?;
            }
            None => {
                let n = self.node(&self.must(id)?);
                let had = n.get("slot").and_then(|v| v.as_string()).map(String::from);
                if let Some(s) = had.filter(|s| !self.offers(parent, s)) {
                    self.set(id, "slot", None, &[])?;
                    cleared = Some(s);
                }
            }
        }
        for c in [old.as_str(), parent, id] {
            self.settle(c)?;
        }
        Ok(Edit::Move {
            id: id.into(),
            parent: old,
            before: was,
            slot: cleared,
        })
    }

    /// Whether box `parent`'s pattern has the slot `s`.
    fn offers(&self, parent: &str, s: &str) -> bool {
        let Some(loc) = self.find(parent) else {
            return false;
        };
        let kids = (self.node(&loc).children())
            .map(|c| c.nodes())
            .unwrap_or_default();
        let words: Option<Vec<String>> =
            (kids.iter())
                .find(|n| n.name().value() == "pattern")
                .map(|n| {
                    (n.entries().iter())
                        .filter_map(|e| e.value().as_string().map(String::from))
                        .collect()
                });
        let p = words.and_then(|w| spec::read::pattern(&w).ok());
        slots(p.as_ref()).contains(&s)
    }
}
