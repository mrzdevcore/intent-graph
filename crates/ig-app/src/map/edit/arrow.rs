//! `ArrowSet`: an arrow changed in place, found by the boxes at its ends.

use super::set::context_format;
use super::{Edit, Spec, arrow, box_id, named, set_str, tidy};
use crate::map::ArrowKind;
use crate::map::arrows::Shape;
use kdl::{KdlEntry, KdlNode};

const PROPS: [&str; 4] = ["label", "note", "context", "shape"];

impl Spec {
    pub(super) fn arrow_set(
        &mut self,
        from: &str,
        to: &str,
        key: &str,
        value: Option<&str>,
        before: Option<&str>,
        restore: &[(usize, String)],
    ) -> Result<Edit, String> {
        let (loc, i) = self.arrow_at(from, to)?;
        let at = |i: usize| {
            let mut l = loc.clone();
            l.1.push(i);
            l
        };
        let need = || value.ok_or_else(|| format!("`{key}` needs a value"));
        let set = |k: &str, old: Option<String>| Edit::ArrowSet {
            from: from.into(),
            to: to.into(),
            key: k.into(),
            value: old,
            before: None,
            restore: vec![],
        };
        match key {
            "kind" => {
                let name = need()?;
                named::<ArrowKind>(name).ok_or_else(|| format!("`{name}` is not an arrow kind"))?;
                let n = self.node_mut(&at(i));
                let old = n.name().value().to_string();
                n.set_name(name);
                Ok(set("kind", Some(old)))
            }
            "to" => {
                let new = need()?;
                self.must(new)?;
                let n = self.node_mut(&at(i));
                set_str(&mut n.entries_mut()[0], new);
                Ok(Edit::ArrowSet {
                    from: from.into(),
                    to: new.into(),
                    key: "to".into(),
                    value: Some(to.into()),
                    before: None,
                    restore: vec![],
                })
            }
            "from" => {
                let new = need()?;
                self.must(new)?;
                let was = self.relocate(&at(i), new, None, before)?;
                Ok(Edit::ArrowSet {
                    from: new.into(),
                    to: to.into(),
                    key: "from".into(),
                    value: Some(from.into()),
                    before: was,
                    restore: vec![],
                })
            }
            "flip" => {
                self.must(to)?;
                let was = self.relocate(&at(i), to, Some(from), before)?;
                Ok(Edit::ArrowSet {
                    from: to.into(),
                    to: from.into(),
                    key: "flip".into(),
                    value: None,
                    before: was,
                    restore: vec![],
                })
            }
            k if PROPS.contains(&k) => {
                if let (Some(v), "shape") = (value, k)
                    && named::<Shape>(v).is_none()
                {
                    let all = Shape::NAMES.join(", ");
                    return Err(format!("`{v}` is not an arrow shape: one of {all}"));
                }
                let n = self.node_mut(&at(i));
                let (old, was) = prop(n, k, value, restore)?;
                Ok(Edit::ArrowSet {
                    from: from.into(),
                    to: to.into(),
                    key: k.into(),
                    value: old,
                    before: None,
                    restore: was,
                })
            }
            _ => Err(format!(
                "`{key}` is not something to set on an arrow: one of kind, label, note, context, shape, from, to, flip"
            )),
        }
    }

    pub(super) fn arrow_node(&self, from: &str, to: &str) -> Option<&KdlNode> {
        let (loc, i) = self.arrow_at(from, to).ok()?;
        self.node(&loc).children()?.nodes().get(i)
    }

    fn arrow_at(&self, from: &str, to: &str) -> Result<(super::Loc, usize), String> {
        let loc = self.must(from)?;
        let nodes = (self.node(&loc).children())
            .map(|c| c.nodes())
            .unwrap_or_default();
        let i = (nodes
            .iter()
            .position(|n| arrow(n).is_some_and(|a| a.1 == to)))
        .ok_or_else(|| format!("no arrow `{from}->{to}`"))?;
        Ok((loc, i))
    }

    fn relocate(
        &mut self,
        at: &super::Loc,
        new: &str,
        to: Option<&str>,
        before: Option<&str>,
    ) -> Result<Option<String>, String> {
        let (mut n, was) = self.take(at);
        if let Some(to) = to {
            set_str(&mut n.entries_mut()[0], to);
        }
        let (loc, i) = match before {
            Some(_) => self.place(new, before)?,
            None => {
                let loc = self.must(new)?;
                let nodes = (self.node(&loc).children())
                    .map(|c| c.nodes())
                    .unwrap_or_default();
                let i = nodes.iter().position(|n| box_id(n).is_some());
                let i = i.unwrap_or(nodes.len());
                (loc, i)
            }
        };
        self.put(&loc, i, n);
        Ok(was)
    }
}

type Was = (Option<String>, Vec<(usize, String)>);

fn prop(
    n: &mut KdlNode,
    key: &str,
    value: Option<&str>,
    restore: &[(usize, String)],
) -> Result<Was, String> {
    let is_key = |e: &KdlEntry| e.name().is_some_and(|k| k.value() == key);
    let at = n.entries().iter().position(is_key);
    let old = at.and_then(|i| n.entries()[i].value().as_string().map(String::from));
    let was = at.map(|i| (i, n.entries()[i].to_string()));
    let back = |text: &str| {
        text.parse::<KdlEntry>()
            .map_err(|_| format!("cannot read `{text}` back"))
    };
    let format = |n: &KdlNode, e: &mut KdlEntry, v: &str| match key {
        "context" => e.set_format(context_format(v, &tidy::indent_of(n))),
        _ => set_str(e, v),
    };
    match (value, at, restore.first()) {
        (Some(_), Some(i), Some((_, text))) => n.entries_mut()[i] = back(text)?,
        (Some(v), Some(i), None) => {
            let mut e = n.entries()[i].clone();
            e.set_value(v);
            format(n, &mut e, v);
            n.entries_mut()[i] = e;
        }
        (Some(_), None, Some((i, text))) => {
            let i = (*i).min(n.entries().len());
            n.entries_mut().insert(i, back(text)?);
        }
        (Some(v), None, None) => {
            let ord = PROPS.iter().position(|p| *p == key);
            let i = n.entries().iter().rposition(|e| {
                e.name()
                    .is_none_or(|k| PROPS.iter().position(|p| *p == k.value()) < ord)
            });
            let mut e = KdlEntry::new_prop(key, v);
            format(n, &mut e, v);
            n.entries_mut().insert(i.map_or(0, |i| i + 1), e);
        }
        (None, _, _) => n.entries_mut().retain(|e| !is_key(e)),
    }
    Ok((old, was.into_iter().collect()))
}
