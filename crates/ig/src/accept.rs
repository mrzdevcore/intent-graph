//! `ig accept`: the code's drift becomes the spec, one person edit per item.

use anyhow::{Result, bail};
use ig_app::map::edit::Edit;
use ig_app::map::edit::journal::{By, record};
use ig_app::map::file::MapBox;
use ig_app::map::origin::Origin;
use ig_app::map::{ArrowKind, Level, MapArrow, view};
use ig_core::ports::Files;

/// Records the drift named by `ids` (all of it for `all`); returns the exit code.
pub fn run(files: &dyn Files, v: &view::View, all: bool, ids: &[String]) -> Result<u8> {
    let boxes: Vec<&MapBox> = v
        .boxes
        .iter()
        .filter(|b| b.origin == Origin::Drift)
        .collect();
    let arrows: Vec<&MapArrow> = v
        .arrows
        .iter()
        .filter(|a| a.origin == Origin::Drift)
        .collect();
    let unknown: Vec<&String> = (ids.iter())
        .filter(|i| !boxes.iter().any(|b| &b.id == *i) && !arrows.iter().any(|a| &a.id == *i))
        .collect();
    if !unknown.is_empty() {
        let list: Vec<&str> = unknown.iter().map(|s| s.as_str()).collect();
        bail!("not drift: {} (see `ig plan`)", list.join(", "));
    }
    let want = |id: &str| all || ids.iter().any(|i| i == id);
    let Some(root) = v.boxes.iter().find(|b| b.level == Level::System) else {
        bail!("no system box in ig/");
    };
    // a box claiming the whole repo would hide every later drift
    if let Some(b) = boxes
        .iter()
        .find(|b| want(&b.id) && b.made_of.iter().any(|g| g == "**"))
    {
        bail!(
            "refusing to accept {}: it claims `**`, the whole repo",
            b.id
        );
    }
    let mut done: Vec<String> = vec![];
    let mut msgs: Vec<String> = vec![];
    for b in boxes.iter().filter(|b| want(&b.id)) {
        let edit = Edit::Add {
            parent: root.id.clone(),
            kind: b.kind.value,
            id: b.id.clone(),
            title: b.title.value.clone(),
            at: b.made_of.clone(),
            before: None,
        };
        record(files, By::Person, &edit).map_err(|e| anyhow::anyhow!(e.join("\n")))?;
        done.push(b.id.clone());
    }
    // an arrow needs both ends in the spec: the boxes this run added count
    let added = done.clone();
    let in_spec = |id: &str| {
        added.iter().any(|d| d == id)
            || v.boxes
                .iter()
                .any(|b| b.id == id && b.origin != Origin::Drift)
    };
    for a in arrows.iter().filter(|a| want(&a.id)) {
        if !in_spec(&a.from) || !in_spec(&a.to) {
            msgs.push(format!("skipped {}: accept its box first", a.id));
            continue;
        }
        // a bundle is written as its main kind
        let kind = a.spec_kind();
        let edit = Edit::Arrow {
            from: a.from.clone(),
            to: a.to.clone(),
            kind,
            // a bundle's label is a derived count, not intent
            label: a.label.clone().filter(|_| a.kind != ArrowKind::Bundle),
        };
        record(files, By::Person, &edit).map_err(|e| anyhow::anyhow!(e.join("\n")))?;
        done.push(a.id.clone());
    }
    done.iter().for_each(|d| println!("accepted {d}"));
    msgs.iter().for_each(|m| println!("{m}"));
    Ok(0)
}
