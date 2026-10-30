//! The map's edits: each runs in the project root, goes through the journal, and returns the
//! fresh View. A refused edit changes nothing and returns the reader's text.

use crate::bounded::Gate;
use crate::commands::{Project, view_data_for};
use ig_app::map::edit::Edit;
use ig_app::map::edit::journal::{self, By, entries};
use ig_wire::Wired;
use serde_json::Value;
use std::path::Path;

type Done = Result<Value, String>;

fn joined(e: Vec<String>) -> String {
    e.join("; ")
}

/// Runs `f` on the project's files, then returns the fresh View.
fn then_view(
    root: &Path,
    f: impl FnOnce(&dyn ig_core::ports::Files) -> Result<(), Vec<String>>,
) -> Done {
    let w = Wired::new(root);
    f(w.env().files).map_err(joined)?;
    view_data_for(root)
}

pub fn apply(root: &Path, edit: Edit) -> Done {
    if matches!(edit, Edit::Restore { .. }) {
        return Err("restore is only made by undo".into());
    }
    // a first run writes ig/ from the code; edit what the person sees
    view_data_for(root)?;
    then_view(root, |f| journal::record(f, By::Person, &edit).map(drop))
}

pub fn keep(root: &Path, n: Option<u64>) -> Done {
    then_view(root, |f| journal::keep(f, n))
}

pub fn drop_one(root: &Path, n: u64) -> Done {
    then_view(root, |f| journal::drop(f, n))
}

/// Drops every entry not kept and not undone, newest first; stops at the first refusal.
pub fn drop_all(root: &Path) -> Done {
    then_view(root, |f| {
        let open: Vec<u64> = (entries(f).0.iter().rev())
            .filter(|e| !e.kept && !e.undone)
            .map(|e| e.n)
            .collect();
        for (k, n) in open.iter().enumerate() {
            journal::drop(f, *n)
                .map_err(|e| vec![format!("dropped {k}; entry {n}: {}", joined(e))])?;
        }
        Ok(())
    })
}

pub fn undo(root: &Path) -> Done {
    then_view(root, |f| journal::undo(f).map(drop))
}

pub fn redo(root: &Path) -> Done {
    then_view(root, |f| journal::redo(f).map(drop))
}

async fn in_project<F>(project: &Project, gate: &Gate, f: F) -> Done
where
    F: FnOnce(&Path) -> Done + Send + 'static,
{
    let root = project.0.lock().unwrap().as_ref().map(|(r, _)| r.clone());
    let Some(root) = root else {
        return Err("no project open: Project > Open… (⌘O)".into());
    };
    gate.run(move || f(&root)).await?
}

#[tauri::command]
pub async fn edit_apply(
    project: tauri::State<'_, Project>,
    gate: tauri::State<'_, Gate>,
    edit: Edit,
) -> Done {
    in_project(&project, &gate, move |r| apply(r, edit)).await
}

#[tauri::command]
pub async fn edit_keep(
    project: tauri::State<'_, Project>,
    gate: tauri::State<'_, Gate>,
    n: Option<u64>,
) -> Done {
    in_project(&project, &gate, move |r| keep(r, n)).await
}

#[tauri::command]
pub async fn edit_drop(
    project: tauri::State<'_, Project>,
    gate: tauri::State<'_, Gate>,
    n: u64,
) -> Done {
    in_project(&project, &gate, move |r| drop_one(r, n)).await
}

#[tauri::command]
pub async fn edit_drop_all(
    project: tauri::State<'_, Project>,
    gate: tauri::State<'_, Gate>,
) -> Done {
    in_project(&project, &gate, drop_all).await
}

#[tauri::command]
pub async fn edit_undo(project: tauri::State<'_, Project>, gate: tauri::State<'_, Gate>) -> Done {
    in_project(&project, &gate, undo).await
}

#[tauri::command]
pub async fn edit_redo(project: tauri::State<'_, Project>, gate: tauri::State<'_, Gate>) -> Done {
    in_project(&project, &gate, redo).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ig-edits-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../ig/tests/fixtures/monorepo");
        let ok = std::process::Command::new("cp")
            .arg("-R")
            .arg(fixture)
            .arg(&dir)
            .status()
            .unwrap()
            .success();
        assert!(ok);
        dir.canonicalize().unwrap()
    }

    fn spec(root: &Path) -> String {
        let mut out = String::new();
        let mut names: Vec<_> = std::fs::read_dir(root.join("ig"))
            .unwrap()
            .map(|e| e.unwrap().path())
            .collect();
        names.sort();
        for p in names {
            out += &std::fs::read_to_string(p).unwrap();
        }
        out
    }

    fn add(parent: &str, id: &str) -> Edit {
        // a glob that matches no code: the box is to build
        serde_json::from_value(serde_json::json!({
            "op": "add", "parent": parent, "kind": "store", "id": id,
            "title": id, "at": ["src/nothing/**"], "before": null
        }))
        .unwrap()
    }

    /// The id of a box that can parent a new one.
    fn parent(root: &Path) -> String {
        let v = view_data_for(root).unwrap();
        v["boxes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|b| b["parent"].is_null())
            .unwrap()["id"]
            .as_str()
            .unwrap()
            .to_string()
    }

    fn has(v: &Value, id: &str) -> Option<Value> {
        v["boxes"]
            .as_array()?
            .iter()
            .find(|b| b["id"] == id)
            .cloned()
    }

    fn proposed(v: &Value) -> usize {
        v["proposals"].as_array().map_or(0, Vec::len)
    }

    #[test]
    fn apply_adds_a_box_the_person_keeps() {
        let r = scratch("apply");
        let p = parent(&r);
        let v = apply(&r, add(&p, "cache")).unwrap();
        assert_eq!(has(&v, "cache").unwrap()["origin"], "build");
        assert_eq!(has(&v, "cache").unwrap()["title"]["source"], "person");
        assert_eq!(proposed(&v), 0);
    }

    #[test]
    fn a_claude_entry_is_a_proposal_until_kept_and_drop_reverts_the_file() {
        let r = scratch("proposal");
        let p = parent(&r);
        view_data_for(&r).unwrap();
        let w = Wired::new(&r);
        let e = journal::record(w.env().files, By::Claude, &add(&p, "cache")).unwrap();
        let v = view_data_for(&r).unwrap();
        assert_eq!(v["proposals"][0]["n"], e.n);
        assert_eq!(v["proposals"][0]["by"], "claude");
        assert_eq!(v["proposals"][0]["summary"], "add store cache");
        assert_eq!(v["proposals"][0]["ids"][0], "cache");
        let v = keep(&r, Some(e.n)).unwrap();
        assert_eq!(proposed(&v), 0);
        assert!(has(&v, "cache").is_some());
        let e = journal::record(w.env().files, By::Claude, &add(&p, "queue")).unwrap();
        let v = drop_one(&r, e.n).unwrap();
        assert!(has(&v, "queue").is_none());
        assert_eq!(proposed(&v), 0);
        assert!(has(&v, "cache").is_some());
    }

    #[test]
    fn drop_all_reverts_every_proposal() {
        let r = scratch("dropall");
        let p = parent(&r);
        view_data_for(&r).unwrap();
        let before = spec(&r);
        let w = Wired::new(&r);
        for id in ["a1", "a2"] {
            journal::record(w.env().files, By::Claude, &add(&p, id)).unwrap();
        }
        let v = drop_all(&r).unwrap();
        assert_eq!(proposed(&v), 0);
        assert_eq!(spec(&r), before);
    }

    #[test]
    fn undo_and_redo_round_trip() {
        let r = scratch("undo");
        let p = parent(&r);
        apply(&r, add(&p, "cache")).unwrap();
        let v = undo(&r).unwrap();
        assert!(has(&v, "cache").is_none());
        let v = redo(&r).unwrap();
        assert!(has(&v, "cache").is_some());
    }

    #[test]
    fn a_refused_edit_errs_and_changes_nothing() {
        let r = scratch("refused");
        let p = parent(&r);
        view_data_for(&r).unwrap();
        let before = spec(&r);
        let e = apply(&r, add(&p, "nowhere/../x y")).unwrap_err();
        assert!(!e.is_empty());
        let e2 = apply(&r, add("no-such-parent", "z")).unwrap_err();
        assert!(!e2.is_empty());
        assert_eq!(spec(&r), before);
        assert!(
            apply(
                &r,
                serde_json::from_value(serde_json::json!({
                    "op": "restore", "parent": p, "before": null, "kdl": "box"
                }))
                .unwrap()
            )
            .is_err()
        );
    }
}
