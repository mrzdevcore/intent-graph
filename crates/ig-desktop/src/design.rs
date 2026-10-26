//! Feature and design commands. The app remembers the bytes it last read or wrote per file: a
//! write is refused ("conflict") when the disk moved on, and a watched change that equals them is
//! the app's own autosave, not news.

use ig_app::design as app;
use ig_core::design::{Design, Scope};
use ig_core::ports::Files;
use ig_wire::Wired;
use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tauri::{Emitter, Manager};

/// Path (relative to the project) -> the bytes last read or written.
#[derive(Default)]
pub struct Known(pub Mutex<HashMap<String, Vec<u8>>>);

/// The code graph the live checks compare designs with: built on the first check, dropped when
/// the code changes.
#[derive(Default)]
pub struct HeadCache(pub Mutex<Option<Arc<ig_app::plan::Head>>>);

fn conflict() -> String {
    "conflict".into()
}

pub fn features_for(root: &Path) -> Vec<app::Feature> {
    app::list(Wired::new(root).files())
}

pub fn create_for(root: &Path, known: &Known, title: &str) -> Result<app::Feature, String> {
    let w = Wired::new(root);
    // The whole system, faded: the user (or Task 5) narrows it later.
    let scope = Scope::Layer {
        name: String::new(),
    };
    let slug = app::free_slug(w.files(), title).map_err(|e| e.to_string())?;
    let f = app::create_as(w.files(), &slug, title, scope).map_err(|e| e.to_string())?;
    remember(w.files(), known, &f.slug);
    Ok(f)
}

fn remember(files: &dyn Files, known: &Known, slug: &str) {
    let path = app::design_path(slug);
    if let Some(b) = files.read_bytes(&path) {
        known.0.lock().unwrap().insert(path, b);
    }
}

pub fn read_for(root: &Path, known: &Known, slug: &str) -> Result<Design, String> {
    let (d, bytes) = app::read_raw(Wired::new(root).files(), slug).map_err(|e| e.to_string())?;
    known
        .0
        .lock()
        .unwrap()
        .insert(app::design_path(slug), bytes);
    Ok(d)
}

pub fn write_for(root: &Path, known: &Known, slug: &str, d: &Design) -> Result<(), String> {
    let path = app::design_path(slug);
    // Held across the write: a second autosave must see the first one's bytes.
    let mut k = known.0.lock().unwrap();
    let expect = k.get(&path).cloned();
    let files = Wired::new(root);
    if !app::write(files.files(), slug, d, expect.as_deref()).map_err(|e| e.to_string())? {
        return Err(conflict());
    }
    k.insert(path, ig_core::design::to_text(d).into_bytes());
    Ok(())
}

/// Checks an unsaved `design` of feature `slug`. Reads no design file; `ms` is the time the check
/// took here, graph build included when the cache was cold.
pub fn check_for(
    root: &Path,
    cache: &HeadCache,
    slug: &str,
    d: &Design,
) -> Result<serde_json::Value, String> {
    if !app::valid_slug(slug) {
        return Err(format!("bad feature name {slug:?}"));
    }
    let t = std::time::Instant::now();
    let w = Wired::new(root);
    let cached = cache.0.lock().unwrap().clone();
    let head = match cached {
        Some(h) => h,
        None => {
            let h = Arc::new(ig_app::plan::Head::load(&w.env()).map_err(|e| e.to_string())?);
            *cache.0.lock().unwrap() = Some(h.clone());
            h
        }
    };
    let screens: Vec<String> = w
        .files()
        .glob(&[format!("plans/{slug}/screens/*.json")])
        .unwrap_or_default()
        .iter()
        .filter_map(|p| {
            p.rsplit('/')
                .next()?
                .strip_suffix(".json")
                .map(String::from)
        })
        .collect();
    let mut v =
        app::check(&head, d, &screens, &app::design_path(slug)).map_err(|e| e.to_string())?;
    v["ms"] = (t.elapsed().as_millis() as u64).into();
    Ok(v)
}

/// The slugs among `slugs` whose design.json differs from what the app last read or wrote.
pub fn changed_outside(root: &Path, known: &Known, slugs: &BTreeSet<String>) -> Vec<String> {
    let files = Wired::new(root);
    let k = known.0.lock().unwrap();
    slugs
        .iter()
        .filter(|s| {
            let p = app::design_path(s);
            files.files().read_bytes(&p) != k.get(&p).cloned()
        })
        .cloned()
        .collect()
}

pub fn emit_changed(app: &tauri::AppHandle, root: &Path, slugs: &BTreeSet<String>) {
    for slug in changed_outside(root, &app.state::<Known>(), slugs) {
        let _ = app.emit("design-changed", serde_json::json!({ "slug": slug }));
    }
}

fn root_of(project: &crate::commands::Project) -> Result<PathBuf, String> {
    let root = project.0.lock().unwrap().as_ref().map(|(r, _)| r.clone());
    let root = root.ok_or_else(|| "no project open: Project > Open… (⌘O)".to_string())?;
    check_plans(&root)?;
    Ok(root)
}

fn check_plans(root: &Path) -> Result<(), String> {
    match std::fs::symlink_metadata(root.join("plans")) {
        Ok(m) if m.file_type().is_symlink() => {
            Err("plans/ is a symlink; design files need a real plans folder".into())
        }
        _ => Ok(()),
    }
}

async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn features(
    project: tauri::State<'_, crate::commands::Project>,
) -> Result<Vec<app::Feature>, String> {
    let root = root_of(&project)?;
    blocking(move || Ok(features_for(&root))).await
}

#[tauri::command]
pub async fn create_feature(
    app: tauri::AppHandle,
    project: tauri::State<'_, crate::commands::Project>,
    title: String,
) -> Result<app::Feature, String> {
    let root = root_of(&project)?;
    blocking(move || create_for(&root, &app.state::<Known>(), &title)).await
}

#[tauri::command]
pub async fn read_design(
    app: tauri::AppHandle,
    project: tauri::State<'_, crate::commands::Project>,
    slug: String,
) -> Result<Design, String> {
    let root = root_of(&project)?;
    blocking(move || read_for(&root, &app.state::<Known>(), &slug)).await
}

#[tauri::command]
pub async fn write_design(
    app: tauri::AppHandle,
    project: tauri::State<'_, crate::commands::Project>,
    slug: String,
    design: Design,
) -> Result<(), String> {
    let root = root_of(&project)?;
    blocking(move || write_for(&root, &app.state::<Known>(), &slug, &design)).await
}

#[tauri::command]
pub async fn check_design(
    app: tauri::AppHandle,
    project: tauri::State<'_, crate::commands::Project>,
    slug: String,
    design: Design,
) -> Result<serde_json::Value, String> {
    let root = root_of(&project)?;
    blocking(move || check_for(&root, &app.state::<HeadCache>(), &slug, &design)).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn autosave_is_not_news_and_a_stale_write_conflicts() {
        let root = std::env::temp_dir().join(format!("ig-desktop-design-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let k = Known::default();
        let f = create_for(&root, &k, "Patient export").unwrap();
        assert_eq!(
            create_for(&root, &k, "Patient export").unwrap().slug,
            "patient-export-2"
        );
        let mut d = read_for(&root, &k, &f.slug).unwrap();
        d.title = "Mine".into();
        write_for(&root, &k, &f.slug, &d).unwrap();
        let slugs = BTreeSet::from([f.slug.clone()]);
        assert!(changed_outside(&root, &k, &slugs).is_empty());
        // Someone else writes: our next write conflicts and the change is news.
        d.title = "Theirs".into();
        let theirs = ig_core::design::to_text(&d);
        std::fs::write(root.join(app::design_path(&f.slug)), theirs).unwrap();
        assert_eq!(changed_outside(&root, &k, &slugs), vec![f.slug.clone()]);
        assert_eq!(write_for(&root, &k, &f.slug, &d), Err("conflict".into()));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn check_lists_screens_and_reuses_the_head() {
        let root = std::env::temp_dir().join(format!("ig-desktop-check-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("plans/f/screens")).unwrap();
        std::fs::write(root.join("ig.yaml"), "layers: {}\n").unwrap();
        std::fs::write(root.join("plans/f/screens/list.json"), "{}").unwrap();
        let d: Design = serde_json::from_str(
            r#"{"version":1,"title":"D","scope":{"kind":"layer","name":"x"},"bridges":[
            {"from":"list/b","to":"a.ts#a"},{"from":"nope/b","to":"a.ts#a"}]}"#,
        )
        .unwrap();
        let c = HeadCache::default();
        let v = check_for(&root, &c, "f", &d).unwrap();
        let rules: Vec<_> = v["findings"]
            .as_array()
            .unwrap()
            .iter()
            .map(|f| f["ruleId"].as_str().unwrap())
            .collect();
        assert!(rules.contains(&"design/bridge-screen-missing"), "{rules:?}");
        assert!(v["ms"].is_u64() && c.0.lock().unwrap().is_some());
        assert!(check_for(&root, &c, "../x", &d).is_err());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[cfg(unix)]
    #[test]
    fn a_symlinked_plans_folder_is_refused() {
        let root = std::env::temp_dir().join(format!("ig-desktop-link-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        std::os::unix::fs::symlink("/tmp", root.join("plans")).unwrap();
        assert!(check_plans(&root).is_err());
        let _ = std::fs::remove_dir_all(&root);
    }
}
