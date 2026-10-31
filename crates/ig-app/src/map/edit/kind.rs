//! `set kind`: the node renamed in place (id, entries, children and comments kept), refused
//! when the new kind may not sit where the box is.

use super::{Edit, Spec};
use crate::map::file::{MapBox, MapFile};
use crate::map::kinds::Kinds;
use crate::map::spec::{self, DIR};
use crate::map::{BoxKind, warn};
use std::collections::{BTreeMap, HashMap};

impl Spec {
    /// `new` (path → text) read back with the team kinds.
    pub(super) fn parsed(&self, new: &BTreeMap<String, String>) -> Result<MapFile, Vec<String>> {
        let mut t: Vec<_> = (new.iter())
            .map(|(p, t)| (p.clone(), Some(t.clone())))
            .collect();
        t.extend(self.kinds.iter().cloned());
        spec::parse(t)
    }

    pub(super) fn set_kind(&mut self, id: &str, to: Option<&str>) -> Result<Edit, String> {
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
        if let Some(why) = warn::misplaced(&by, b, def.base, &def.name) {
            return Err(why);
        }
        let loc = self.must(id)?;
        let n = self.node_mut(&loc);
        let old = n.name().value().to_string();
        n.set_name(to);
        Ok(Edit::Set {
            id: id.into(),
            key: "kind".into(),
            value: Some(old),
        })
    }
}
