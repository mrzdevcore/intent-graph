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
    /// The edited box or arrow ids only: what gets the proposal look, chip and Keep/Drop.
    pub marks: Vec<String>,
}

fn word<T: Serialize>(v: &T) -> String {
    serde_json::to_value(v)
        .ok()
        .and_then(|v| v.as_str().map(String::from))
        .unwrap_or_default()
}

fn ids(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| s.to_string()).collect()
}

fn describe(e: &Edit, t: &dyn Fn(&str) -> String) -> (String, Vec<String>) {
    match e {
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
        Edit::ArrowSet { from, to, key, .. } => (
            format!("{} → {}: {key}", t(from), t(to)),
            ids(&[&format!("{from}->{to}")]),
        ),
        Edit::Batch { edits } => {
            let parts: Vec<_> = edits.iter().map(|e| describe(e, t)).collect();
            let summary = parts.iter().map(|p| p.0.as_str()).collect::<Vec<_>>();
            (
                summary.join("; "),
                parts.into_iter().flat_map(|p| p.1).collect(),
            )
        }
        Edit::Restore { parent, .. } => ("restore".into(), ids(&[parent])),
    }
}

/// A line for each entry not kept and not undone; box titles where the view knows the box.
pub fn proposals(entries: &[Entry], boxes: &[MapBox]) -> Vec<Proposal> {
    let t = |id: &str| {
        boxes
            .iter()
            .find(|b| b.id == id)
            .map_or(id.to_string(), |b| b.title.value.clone())
    };
    (entries.iter().filter(|e| !e.kept && !e.undone))
        .map(|e| {
            let (summary, ids) = describe(&e.edit, &t);
            // a move's destination is lit on hover but is not what changed
            let marks = match &e.edit {
                Edit::Move { id, .. } => vec![id.clone()],
                _ => ids.clone(),
            };
            Proposal {
                n: e.n,
                by: e.by,
                summary,
                ids,
                marks,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_edit_kind_has_its_summary() {
        let cases = [
            (
                r#"{"op":"add","parent":"p","kind":"store","id":"a","title":"A","at":[],"before":null}"#,
                "add store A",
            ),
            (r#"{"op":"remove","id":"a"}"#, "remove a"),
            (
                r#"{"op":"move","id":"a","parent":"p","before":null}"#,
                "move a → p",
            ),
            (r#"{"op":"rename","id":"a","to":"b"}"#, "rename a → b"),
            (
                r#"{"op":"set","id":"a","key":"tech","value":"x"}"#,
                "a: tech",
            ),
            (
                r#"{"op":"arrow","from":"a","to":"b","kind":"calls","label":"hi"}"#,
                "arrow a calls b hi",
            ),
            (
                r#"{"op":"unarrow","from":"a","to":"b","kind":"calls"}"#,
                "unarrow a calls b",
            ),
            (
                r#"{"op":"restore","parent":"p","before":null,"kdl":""}"#,
                "restore",
            ),
        ];
        for (json, want) in cases {
            let edit: Edit = serde_json::from_str(json).unwrap();
            let e = Entry {
                n: 1,
                by: By::Claude,
                kept: false,
                undone: false,
                inverse: edit.clone(),
                edit,
            };
            assert_eq!(proposals(&[e], &[])[0].summary, want, "{json}");
        }
    }

    #[test]
    fn a_move_marks_only_the_moved_box() {
        let edit: Edit =
            serde_json::from_str(r#"{"op":"move","id":"a","parent":"p","before":null}"#).unwrap();
        let e = Entry {
            n: 1,
            by: By::Claude,
            kept: false,
            undone: false,
            inverse: edit.clone(),
            edit,
        };
        let p = &proposals(&[e], &[])[0];
        assert_eq!(
            (p.ids.clone(), p.marks.clone()),
            (vec!["a".into(), "p".into()], vec!["a".into()])
        );
    }
}
