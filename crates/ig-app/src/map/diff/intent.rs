//! What changed in `ig/` between the two sides: boxes, moves, fields and arrows, by id.

use super::{
    FieldsChanged, Intent, IntentStatus, Moved, SpecSide, arrows_missing, boxes_missing, count,
};
use crate::map::file::{Field, MapBox, MapFile};
use std::collections::{BTreeSet, HashMap};

pub(super) fn intent(base: &SpecSide, head: &SpecSide) -> Intent {
    let mut i = Intent {
        status: IntentStatus::Same,
        note: None,
        added: vec![],
        removed: vec![],
        moved: vec![],
        changed: vec![],
        arrows_added: vec![],
        arrows_removed: vec![],
    };
    match (base, head) {
        (SpecSide::Broken(e), _) => {
            i.status = IntentStatus::BaseBroken;
            i.note = Some(format!("base ig/ does not read: {e}"));
        }
        (SpecSide::Missing, SpecSide::Read(h)) => {
            i.status = IntentStatus::Added;
            i.note = Some(format!(
                "ig/ added ({})",
                count(h.boxes.len(), "box", "boxes")
            ));
        }
        (SpecSide::Read(_), SpecSide::Missing) => {
            i.status = IntentStatus::Removed;
            i.note = Some("ig/ removed".into());
        }
        (SpecSide::Read(b), SpecSide::Read(h)) => {
            compare(&mut i, b, h);
            let same = i.added.is_empty()
                && i.removed.is_empty()
                && i.moved.is_empty()
                && i.changed.is_empty()
                && i.arrows_added.is_empty()
                && i.arrows_removed.is_empty();
            if !same {
                i.status = IntentStatus::Changed;
            }
        }
        _ => {}
    }
    i
}

fn compare(i: &mut Intent, base: &MapFile, head: &MapFile) {
    let (bk, hk) = (ids(base), ids(head));
    i.added = boxes_missing(head, &hk, &bk);
    i.removed = boxes_missing(base, &bk, &hk);
    i.arrows_added = arrows_missing(head, &hk, base, &bk);
    i.arrows_removed = arrows_missing(base, &bk, head, &hk);
    let olds: HashMap<&str, &MapBox> = base.boxes.iter().map(|b| (b.id.as_str(), b)).collect();
    let mut news: Vec<&MapBox> = head.boxes.iter().collect();
    news.sort_by(|a, b| a.id.cmp(&b.id));
    for new in news {
        let Some(old) = olds.get(new.id.as_str()) else {
            continue;
        };
        if old.parent != new.parent {
            i.moved.push(Moved {
                id: new.id.clone(),
                from: old.parent.clone(),
                to: new.parent.clone(),
            });
        }
        let fields = fields(old, new);
        if !fields.is_empty() {
            i.changed.push(FieldsChanged {
                id: new.id.clone(),
                fields,
            });
        }
    }
}

fn ids(m: &MapFile) -> HashMap<&str, String> {
    m.boxes
        .iter()
        .map(|b| (b.id.as_str(), b.id.clone()))
        .collect()
}

fn value<T>(f: &Option<Field<T>>) -> Option<&T> {
    f.as_ref().map(|f| &f.value)
}

// by value: who wrote a field is no change of it
fn fields(old: &MapBox, new: &MapBox) -> Vec<String> {
    let mut out: Vec<String> = vec![];
    let mut differ = |name: &str, d: bool| {
        if d {
            out.push(name.into());
        }
    };
    differ("title", old.title.value != new.title.value);
    differ(
        "description",
        value(&old.description) != value(&new.description),
    );
    differ(
        "kind",
        old.kind.value != new.kind.value || old.custom != new.custom,
    );
    differ("tech", value(&old.tech) != value(&new.tech));
    differ("at", old.made_of != new.made_of);
    differ("pattern", value(&old.pattern) != value(&new.pattern));
    let props: BTreeSet<&String> = old.props.keys().chain(new.props.keys()).collect();
    for p in props {
        differ(p, old.props.get(p) != new.props.get(p));
    }
    differ("badges", value(&old.badges) != value(&new.badges));
    differ("logo", value(&old.logo) != value(&new.logo));
    out
}
