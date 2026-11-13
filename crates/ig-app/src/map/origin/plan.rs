use super::{MapBox, MapFile, Origin};
use crate::map::{ArrowKind, BoxKind, MapArrow};
use serde::Serialize;

/// What is left to build or move, what the code does that the spec does not say and the code
/// ig does not read, one line each.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    pub build: Vec<String>,
    pub moves: Vec<String>,
    pub drift: Vec<String>,
    pub not_read: Vec<String>,
    pub items: Vec<Item>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Step {
    Build,
    Move,
    Drift,
    NotRead,
}

/// One plan line in parts: `under` is an arrow's source box, `what` the line without it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    pub step: Step,
    pub id: String,
    pub under: Option<String>,
    pub what: String,
}

impl Plan {
    /// Nothing left to build, move, describe or read.
    pub fn is_empty(&self) -> bool {
        self.lines().next().is_none()
    }

    pub fn lines(&self) -> impl Iterator<Item = &String> {
        (self.build.iter())
            .chain(&self.moves)
            .chain(&self.drift)
            .chain(&self.not_read)
    }

    /// `ig <cmd>: …` and every line; moves and code not read are counted only when there are some.
    pub fn text(&self, cmd: &str) -> String {
        let mut t = format!(
            "ig {cmd}: {} to build, {} drift",
            self.build.len(),
            self.drift.len()
        );
        if !self.moves.is_empty() {
            t += &format!(", {} to move", self.moves.len());
        }
        if !self.not_read.is_empty() {
            t += &format!(", {} not read", self.not_read.len());
        }
        self.lines().fold(t, |t, l| format!("{t}\n{l}"))
    }
}

fn name<T: Serialize>(v: &T) -> String {
    serde_json::to_value(v)
        .ok()
        .and_then(|v| v.as_str().map(String::from))
        .unwrap_or_default()
}

/// One line per box or arrow not built, sorted.
pub fn plan(m: &MapFile) -> Plan {
    plan_of(&m.boxes, &m.arrows, &[])
}

/// `plan` over a view's boxes and arrows, and the code ig does not read (`not_read`'s globs).
pub fn plan_of(boxes: &[MapBox], arrows: &[MapArrow], unread: &[String]) -> Plan {
    let mut p = Plan {
        build: vec![],
        moves: vec![],
        drift: vec![],
        not_read: (unread.iter())
            .map(|g| format!(
                    "not read: {g} (outside every package ig reads; add it to a workspace or to `ignore` in ig.yaml)"
                ))
            .collect(),
        items: (unread.iter())
            .map(|g| Item { step: Step::NotRead, id: g.clone(), under: None, what: g.clone() })
            .collect(),
    };
    let (mut build, mut moves, mut drift) = (vec![], vec![], vec![]);
    let mut item = |step, id: &str, under: Option<&String>, what: String| {
        p.items.push(Item {
            step,
            id: id.into(),
            under: under.cloned(),
            what,
        })
    };
    for b in boxes {
        match b.origin {
            Origin::Built | Origin::Unmanaged => {}
            Origin::Build => {
                let mut l = format!("{} {}", name(&b.kind.value), b.id);
                if b.kind.value == BoxKind::Box {
                    l.push_str(", kind not decided");
                }
                if !b.made_of.is_empty() {
                    l.push_str(&format!(" ({})", b.made_of.join(", ")));
                }
                build.push(format!("to build: {l}"));
                item(Step::Build, &b.id, None, l);
            }
            Origin::Moved => {
                let l = format!("{} → {}", b.moved_from.join(", "), b.made_of.join(", "));
                moves.push(format!("move: {l} ({})", b.id));
                item(Step::Move, &b.id, None, l);
            }
            Origin::Drift => {
                let what = match b.made_of.first() {
                    Some(g) => format!("code under {g} is in no box"),
                    None => format!("{} {}", name(&b.kind.value), b.id),
                };
                drift.push(match b.made_of.first() {
                    Some(_) => format!("drift: {what} ({})", b.id),
                    None => format!("drift: {what} is in the code, not in ig/"),
                });
                item(Step::Drift, &b.id, None, what);
            }
        }
    }
    // each arrow as written: what ig/ says, not the boxes it is drawn between
    let written = arrows.iter().flat_map(|a| match a.parts.is_empty() {
        true => std::slice::from_ref(a),
        false => &a.parts[..],
    });
    for a in written {
        let (from, kind, to) = (&a.from, name::<ArrowKind>(&a.kind), &a.to);
        match a.origin {
            Origin::Built | Origin::Moved | Origin::Unmanaged => {}
            Origin::Build => {
                build.push(format!("to build: {from} {kind} {to}"));
                item(Step::Build, &a.id, Some(from), format!("{kind} {to}"));
            }
            Origin::Drift => {
                drift.push(format!("drift: {from} {kind} {to}, not in ig/ ({})", a.id));
                item(Step::Drift, &a.id, Some(from), format!("{kind} {to}"));
            }
        }
    }
    (p.build, p.moves, p.drift) = (build, moves, drift);
    p.items
        .sort_by(|a, b| (a.step, &a.under, &a.what).cmp(&(b.step, &b.under, &b.what)));
    p.build.sort();
    p.moves.sort();
    p.drift.sort();
    p.not_read.sort();
    p
}
