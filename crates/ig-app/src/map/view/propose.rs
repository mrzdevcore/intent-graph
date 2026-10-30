//! `ig view --read` with a spec in `ig/`: the AI reading of the drift and of the spec boxes with
//! no description, recorded as `reading` edits for the person to keep or drop.

use super::super::Level;
use super::super::agent::{Agent, Ask, Evidence, NO_AGENT, read};
use super::super::edit::Edit;
use super::super::edit::journal::{By, record};
use super::super::file::{Field, MapBox, MapFile, Source};
use super::super::origin::Origin;
use ig_core::config::Config;
use ig_core::ports::Files;
use std::collections::{BTreeSet, HashMap};

/// Reads `cm` (the compared map) and records what the reading changed, by box id; the notices.
/// A reading that fails records nothing; an edit that cannot be recorded stops the rest.
pub fn propose(
    a: &mut Ask,
    cm: &MapFile,
    ev: &Evidence,
    cfg: &Config,
    files: &dyn Files,
    progress: &dyn Fn(&str),
) -> Vec<String> {
    let Some(root) = cm.boxes.iter().find(|b| b.level == Level::System) else {
        return vec![];
    };
    let to_read: BTreeSet<&str> = (cm.boxes.iter())
        .filter(|b| !b.stale && (b.origin == Origin::Drift || b.description.is_none()))
        .map(|b| b.id.as_str())
        .collect();
    // the boxes read and their ancestors, so ids and parents resolve; only the first are read
    let parent: HashMap<&str, Option<&str>> = (cm.boxes.iter())
        .map(|b| (b.id.as_str(), b.parent.as_deref()))
        .collect();
    let mut shown = to_read.clone();
    for id in &to_read {
        let mut at = parent.get(id).copied().flatten();
        // stops at a box already shown, so a bad parent chain cannot loop
        while let Some(p) = at.filter(|p| shown.insert(p)) {
            at = parent.get(p).copied().flatten();
        }
    }
    let mut m = MapFile {
        version: cm.version,
        boxes: (cm.boxes.iter())
            .filter(|b| shown.contains(b.id.as_str()))
            .map(|b| MapBox {
                needs_reading: to_read.contains(b.id.as_str()),
                // a spec's fields are a person's, pinned: the reading may propose others
                pinned: vec![],
                ..b.clone()
            })
            .collect(),
        arrows: (cm.arrows.iter())
            .filter(|x| shown.contains(x.from.as_str()) && shown.contains(x.to.as_str()))
            .cloned()
            .collect(),
        marks: vec![],
    };
    let Some(agent) = Agent::pick(cfg, a.flag, a.found) else {
        return vec![NO_AGENT.into()];
    };
    if let Err(e) = read(
        agent, a.template, &mut m, ev, false, a.run, progress, a.meter,
    ) {
        return vec![e];
    }
    let mut edits = vec![];
    for id in to_read {
        // regrouped away (the spec's structure changes only by explicit edits), or not read
        // (left out of its answer, or of a batch that failed)
        let (Some(old), Some(new)) = (find(cm, id), find(&m, id).filter(|b| !b.needs_reading))
        else {
            continue;
        };
        let set = |key: &str, value: &str| Edit::Set {
            id: id.into(),
            key: key.into(),
            value: Some(value.into()),
        };
        let titles = (Some(old.title.clone()), Some(new.title.clone()));
        let fields = [
            ("title", &titles.0, &titles.1),
            ("description", &old.description, &new.description),
            ("tech", &old.tech, &new.tech),
            ("logo", &old.logo, &new.logo),
        ];
        if old.origin == Origin::Drift {
            edits.push(Edit::Add {
                parent: root.id.clone(),
                kind: new.kind.value,
                id: id.into(),
                title: new.title.value.clone(),
                at: old.made_of.clone(),
                before: None,
            });
            // its title is in the Add
            for (key, _, f) in fields.into_iter().skip(1) {
                if let Some(f) = f.as_ref().filter(|f| f.source == Source::Agent) {
                    edits.push(set(key, &f.value));
                }
            }
        } else {
            for (key, o, n) in fields {
                if let Some(n) = n.as_ref().filter(|n| value(o) != Some(&n.value)) {
                    edits.push(set(key, &n.value));
                }
            }
        }
    }
    let mut notices = vec![];
    let mut n = 0;
    for e in &edits {
        if let Err(err) = record(files, By::Reading, e) {
            notices.push(format!(
                "the AI reading stopped: an edit to {} was not recorded: {}",
                target(e),
                err.join("; ")
            ));
            break;
        }
        n += 1;
    }
    if n > 0 {
        notices.push(format!(
            "the AI reading proposed {n} edits; keep or drop them in the app, or review with git \
             diff ig/"
        ));
    } else if notices.is_empty() {
        notices.push("the AI reading found nothing to propose".into());
    }
    notices
}

fn find<'a>(m: &'a MapFile, id: &str) -> Option<&'a MapBox> {
    m.boxes.iter().find(|b| b.id == id)
}

fn target(e: &Edit) -> &str {
    match e {
        Edit::Add { id, .. } | Edit::Set { id, .. } => id,
        _ => "",
    }
}

fn value(f: &Option<Field<String>>) -> Option<&String> {
    f.as_ref().map(|f| &f.value)
}

#[cfg(test)]
mod tests {
    use super::super::run_with;
    use crate::map::agent::{Agent, Ask};
    use crate::map::edit::Edit;
    use crate::map::edit::journal::{By, entries};
    use crate::map::origin::Origin;
    use crate::testing::{Fake, env};
    use ig_core::ports::Files;
    use ig_core::testing::MemFiles;

    /// A repo whose `ig/` is written, then a file outside every `at` (drift).
    fn repo() -> MemFiles {
        let files = MemFiles::with(&[("src/a.ts", "export function f() {}\n")]);
        super::super::run(&env(&files, None, &Fake)).unwrap();
        files
            .write_atomic("lib/c.ts", b"export const y = 2;\n")
            .unwrap();
        files
    }

    /// `ig view --read` with an agent answering `reply` for the ids it is asked about.
    fn read(
        files: &MemFiles,
        reply: &dyn Fn(&[String]) -> Result<String, String>,
    ) -> super::super::View {
        let mut run = |_: Agent, _: &str, ids: &[String]| reply(ids);
        run_with(
            &env(files, None, &Fake),
            Some(&mut Ask {
                all: true,
                flag: None,
                template: "{{boxes}}",
                found: &|_| true,
                confirm: None,
                run: &mut run,
                meter: &|_, _, _| {},
            }),
        )
        .unwrap()
    }

    fn describe(ids: &[String]) -> Result<String, String> {
        let notes: Vec<_> = (ids.iter())
            .map(|id| serde_json::json!({"id": id, "title": "Read", "description": "What it does"}))
            .collect();
        Ok(format!(
            "<ig-annotations>{}</ig-annotations>",
            serde_json::Value::Array(notes)
        ))
    }

    #[test]
    fn the_reading_proposes_sets_and_adds() {
        let files = repo();
        let drift = super::super::run(&env(&files, None, &Fake)).unwrap();
        let drift: Vec<String> = (drift.boxes.iter())
            .filter(|b| b.origin == Origin::Drift)
            .map(|b| b.id.clone())
            .collect();
        assert!(!drift.is_empty());
        let v = read(&files, &describe);
        let (es, _) = entries(&files);
        assert!(!es.is_empty());
        assert!(es.iter().all(|e| e.by == By::Reading && !e.kept), "{es:?}");
        let want = format!(
            "the AI reading proposed {} edits; keep or drop them in the app, or review with git diff ig/",
            es.len()
        );
        assert!(v.notices.contains(&want), "{:?}", v.notices);
        // a spec box without description: a Set
        assert!(es.iter().any(|e| matches!(&e.edit,
            Edit::Set { key, value: Some(d), .. } if key == "description" && d == "What it does")));
        // each drift box: an Add with the reading's title and its own `at`
        for id in &drift {
            assert!(
                es.iter().any(|e| matches!(&e.edit,
                    Edit::Add { id: i, title, .. } if i == id && title == "Read")),
                "{es:?}"
            );
        }
        let all: String = (files.glob(&["ig/*.kdl".into()]).unwrap().iter())
            .filter_map(|p| files.read(p))
            .collect();
        assert!(all.contains("What it does"), "{all}");
        // the drift is in the spec now
        assert!(v.boxes.iter().all(|b| b.origin != Origin::Drift));
    }

    #[test]
    fn a_reading_that_fails_changes_nothing() {
        let files = repo();
        let before: Vec<_> = (files.glob(&["ig/*".into()]).unwrap().iter())
            .map(|p| (p.clone(), files.read(p)))
            .collect();
        let v = read(&files, &|_| Err("claude is down".into()));
        let after: Vec<_> = (files.glob(&["ig/*".into()]).unwrap().iter())
            .map(|p| (p.clone(), files.read(p)))
            .collect();
        assert_eq!(before, after);
        assert!(entries(&files).0.is_empty());
        assert!(
            v.notices.iter().any(|n| n.contains("claude is down")),
            "{:?}",
            v.notices
        );
    }

    #[test]
    fn nothing_read_is_nothing_proposed() {
        let files = repo();
        let v = read(
            &files,
            &|_| Ok("<ig-annotations>[]</ig-annotations>".into()),
        );
        assert!(entries(&files).0.is_empty());
        assert!(
            v.notices
                .iter()
                .any(|n| n == "the AI reading found nothing to propose"),
            "{:?}",
            v.notices
        );
    }
}
