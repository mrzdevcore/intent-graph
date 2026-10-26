//! Recovery copies of board edits that could not be written: plans/<slug>/design.unsaved.json,
//! then design.unsaved.2.json, .3, … A copy is never overwritten: each one kept takes the next
//! free name, and the board offers every one back.

use crate::bounded::Gate;
use crate::design::{blocking, root_of};
use ig_app::design::valid_slug;
use ig_core::ports::Files;
use ig_wire::Wired;
use std::path::Path;

#[derive(serde::Serialize, Debug, PartialEq)]
pub struct Kept {
    /// The file name, as `discard_for` takes it.
    pub name: String,
    pub text: String,
    /// When it was written, ms since the epoch.
    pub at: u64,
}

fn name(n: u32) -> String {
    match n {
        1 => "design.unsaved.json".into(),
        n => format!("design.unsaved.{n}.json"),
    }
}

/// `name`'s number: 1 for design.unsaved.json, n for design.unsaved.<n>.json (n ≥ 2).
fn number(name: &str) -> Option<u32> {
    let mid = name.strip_prefix("design.unsaved")?.strip_suffix(".json")?;
    match mid.strip_prefix('.') {
        None if mid.is_empty() => Some(1),
        Some(n) if !n.starts_with('0') => n.parse().ok().filter(|n| *n >= 2),
        _ => None,
    }
}

fn dir(slug: &str) -> Result<String, String> {
    if !valid_slug(slug) {
        return Err(format!("bad feature name {slug:?}"));
    }
    Ok(format!("plans/{slug}"))
}

/// Keeps `text` under the first free name, atomically; returns that name.
pub fn keep_for(root: &Path, slug: &str, text: &str) -> Result<String, String> {
    let dir = dir(slug)?;
    let w = Wired::new(root);
    let taken = |n: &u32| std::fs::symlink_metadata(root.join(&dir).join(name(*n))).is_ok();
    let n = (1..).find(|n| !taken(n)).expect("a free name");
    w.files()
        .write_atomic(&format!("{dir}/{}", name(n)), text.as_bytes())
        .map_err(|e| e.to_string())?;
    Ok(name(n))
}

/// Every recovery copy, oldest name first.
pub fn list_for(root: &Path, slug: &str) -> Result<Vec<Kept>, String> {
    let dir = dir(slug)?;
    let mut found: Vec<(u32, String)> = std::fs::read_dir(root.join(&dir))
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
        .filter_map(|e| e.file_name().into_string().ok())
        .filter_map(|f| Some((number(&f)?, f)))
        .collect();
    found.sort();
    let files = Wired::new(root);
    Ok(found
        .into_iter()
        .filter_map(|(_, name)| {
            let path = format!("{dir}/{name}");
            let text = String::from_utf8_lossy(&files.files().read_bytes(&path)?).into_owned();
            let at = std::fs::metadata(root.join(&path))
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map_or(0, |d| d.as_millis() as u64);
            Some(Kept { name, text, at })
        })
        .collect())
}

pub fn discard_for(root: &Path, slug: &str, name: &str) -> Result<(), String> {
    let dir = dir(slug)?;
    if number(name).is_none() {
        return Err(format!("not a recovery copy: {name:?}"));
    }
    Wired::new(root)
        .files()
        .remove(&format!("{dir}/{name}"))
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn keep_unsaved(
    project: tauri::State<'_, crate::commands::Project>,
    gate: tauri::State<'_, Gate>,
    slug: String,
    text: String,
) -> Result<String, String> {
    let root = root_of(&project)?;
    blocking(&gate, move || keep_for(&root, &slug, &text)).await
}

#[tauri::command]
pub async fn read_unsaved(
    project: tauri::State<'_, crate::commands::Project>,
    gate: tauri::State<'_, Gate>,
    slug: String,
) -> Result<Vec<Kept>, String> {
    let root = root_of(&project)?;
    blocking(&gate, move || list_for(&root, &slug)).await
}

#[tauri::command]
pub async fn discard_unsaved(
    project: tauri::State<'_, crate::commands::Project>,
    gate: tauri::State<'_, Gate>,
    slug: String,
    name: String,
) -> Result<(), String> {
    let root = root_of(&project)?;
    blocking(&gate, move || discard_for(&root, &slug, &name)).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_round_trip_and_nothing_else_is_a_copy() {
        for n in [1, 2, 10] {
            assert_eq!(number(&name(n)), Some(n));
        }
        for bad in [
            "design.json",
            "design.unsaved.1.json",
            "design.unsaved.02.json",
            "design.unsaved..json",
            "design.unsaved.x.json",
            "design.unsaved.tmp-4",
            "../design.unsaved.json",
        ] {
            assert_eq!(number(bad), None, "{bad}");
        }
    }

    #[test]
    fn a_copy_is_never_overwritten_and_each_is_listed_and_discarded() {
        let root = std::env::temp_dir().join(format!("ig-desktop-unsaved-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        assert_eq!(list_for(&root, "export"), Ok(vec![]));
        assert_eq!(
            keep_for(&root, "export", "one").unwrap(),
            "design.unsaved.json"
        );
        assert_eq!(
            keep_for(&root, "export", "two").unwrap(),
            "design.unsaved.2.json"
        );
        let l = list_for(&root, "export").unwrap();
        let texts: Vec<_> = l
            .iter()
            .map(|k| (k.name.as_str(), k.text.as_str()))
            .collect();
        assert_eq!(
            texts,
            [
                ("design.unsaved.json", "one"),
                ("design.unsaved.2.json", "two")
            ]
        );
        assert!(l[0].at > 0);
        assert!(!root.join("plans/export/design.json").exists());
        discard_for(&root, "export", "design.unsaved.json").unwrap();
        // the first name is free again; the second copy stays
        assert_eq!(
            keep_for(&root, "export", "three").unwrap(),
            "design.unsaved.json"
        );
        assert_eq!(list_for(&root, "export").unwrap().len(), 2);
        assert!(keep_for(&root, "../x", "{}").is_err());
        assert!(discard_for(&root, "export", "design.json").is_err());
        assert!(root.join("plans/export").read_dir().unwrap().count() == 2);
        let _ = std::fs::remove_dir_all(&root);
    }
}
