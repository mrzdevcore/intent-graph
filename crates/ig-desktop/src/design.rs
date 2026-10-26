//! Feature and design commands. The app remembers the bytes it last read or wrote per file: a
//! write is refused ("conflict") when the disk moved on, and a watched change that equals them is
//! the app's own autosave, not news.

use crate::bounded::Gate;
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
/// the code changes. The number counts clears: a load that started before one never fills the cache.
#[derive(Default)]
pub struct HeadCache(pub Mutex<(u64, Option<Arc<ig_app::plan::Head>>)>);

impl HeadCache {
    pub fn clear(&self) {
        let mut g = self.0.lock().unwrap();
        *g = (g.0 + 1, None);
    }

    /// The cached head, or `load`'s, cached unless a clear came in between.
    fn get(
        &self,
        load: impl FnOnce() -> Result<ig_app::plan::Head, String>,
    ) -> Result<Arc<ig_app::plan::Head>, String> {
        let (gen_, cached) = {
            let g = self.0.lock().unwrap();
            (g.0, g.1.clone())
        };
        if let Some(h) = cached {
            return Ok(h);
        }
        let h = Arc::new(load()?);
        let mut g = self.0.lock().unwrap();
        if g.0 == gen_ {
            g.1 = Some(h.clone());
        }
        Ok(h)
    }
}

pub(crate) fn conflict() -> String {
    "conflict".into()
}

pub fn features_for(root: &Path) -> Vec<app::Feature> {
    app::list(Wired::new(root).files())
}

pub fn create_for(root: &Path, known: &Known, title: &str) -> Result<app::Feature, String> {
    let w = Wired::new(root);
    // The whole system, faded: the board's scope picker narrows it later.
    let scope = Scope::Dir { id: ".".into() };
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
    seq: Option<u64>,
) -> Result<serde_json::Value, String> {
    if !app::valid_slug(slug) {
        return Err(format!("bad feature name {slug:?}"));
    }
    let t = std::time::Instant::now();
    let w = Wired::new(root);
    let head = cache.get(|| ig_app::plan::Head::load(&w.env()).map_err(|e| e.to_string()))?;
    let screens = ig_app::screens::pieces(w.files(), slug);
    let mut v =
        app::check(&head, d, &screens, &app::design_path(slug)).map_err(|e| e.to_string())?;
    // Handed back as given, so the caller can drop a response that is no longer the latest.
    v["seq"] = seq.into();
    v["ms"] = (t.elapsed().as_millis() as u64).into();
    Ok(v)
}

/// Whether `plans/<slug>/plan.md` exists and how many blocking (error) findings its check has.
/// Named apart from `ig_core::plan::PlanStatus`.
#[derive(serde::Serialize, Debug, PartialEq)]
pub struct BoardStatus {
    pub exists: bool,
    pub blocking: u32,
}

/// `ig plan check` on the feature's plan.md, without recording it. A plan that does not parse
/// counts as one blocking finding: it can't be applied.
pub fn plan_status_for(root: &Path, slug: &str) -> Result<BoardStatus, String> {
    if !app::valid_slug(slug) {
        return Err(format!("bad feature name {slug:?}"));
    }
    let w = Wired::new(root);
    let path = format!("plans/{slug}/plan.md");
    let Some(bytes) = w.files().read_bytes(&path) else {
        return Ok(BoardStatus {
            exists: false,
            blocking: 0,
        });
    };
    let blocking = match ig_app::plan::check_text(&w.env(), &path, &String::from_utf8_lossy(&bytes))
    {
        Ok(v) => v["findings"].as_array().map_or(0, |f| {
            f.iter().filter(|f| f["severity"] == "error").count() as u32
        }),
        Err(_) => 1,
    };
    Ok(BoardStatus {
        exists: true,
        blocking,
    })
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

pub(crate) fn root_of(project: &crate::commands::Project) -> Result<PathBuf, String> {
    let root = project.0.lock().unwrap().as_ref().map(|(r, _)| r.clone());
    let root = root.ok_or_else(|| "no project open: Project > Open… (⌘O)".to_string())?;
    ig_wire::real_plans(&root)?;
    Ok(root)
}

/// `f` on a blocking thread, at most `bounded`'s limit at once with the other disk commands.
pub(crate) async fn blocking<T: Send + 'static>(
    gate: &Gate,
    f: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    gate.run(f).await?
}

#[tauri::command]
pub async fn features(
    project: tauri::State<'_, crate::commands::Project>,
    gate: tauri::State<'_, Gate>,
) -> Result<Vec<app::Feature>, String> {
    let root = root_of(&project)?;
    blocking(&gate, move || Ok(features_for(&root))).await
}

#[tauri::command]
pub async fn create_feature(
    app: tauri::AppHandle,
    project: tauri::State<'_, crate::commands::Project>,
    gate: tauri::State<'_, Gate>,
    title: String,
) -> Result<app::Feature, String> {
    let root = root_of(&project)?;
    blocking(&gate, move || {
        create_for(&root, &app.state::<Known>(), &title)
    })
    .await
}

#[tauri::command]
pub async fn read_design(
    app: tauri::AppHandle,
    project: tauri::State<'_, crate::commands::Project>,
    gate: tauri::State<'_, Gate>,
    slug: String,
) -> Result<Design, String> {
    let root = root_of(&project)?;
    blocking(&gate, move || read_for(&root, &app.state::<Known>(), &slug)).await
}

#[tauri::command]
pub async fn write_design(
    app: tauri::AppHandle,
    project: tauri::State<'_, crate::commands::Project>,
    gate: tauri::State<'_, Gate>,
    slug: String,
    design: Design,
) -> Result<(), String> {
    let root = root_of(&project)?;
    blocking(&gate, move || {
        write_for(&root, &app.state::<Known>(), &slug, &design)
    })
    .await
}

#[tauri::command]
pub async fn check_design(
    app: tauri::AppHandle,
    project: tauri::State<'_, crate::commands::Project>,
    gate: tauri::State<'_, Gate>,
    slug: String,
    design: Design,
    seq: Option<u64>,
) -> Result<serde_json::Value, String> {
    let root = root_of(&project)?;
    blocking(&gate, move || {
        check_for(&root, &app.state::<HeadCache>(), &slug, &design, seq)
    })
    .await
}

#[tauri::command]
pub async fn plan_status(
    project: tauri::State<'_, crate::commands::Project>,
    gate: tauri::State<'_, Gate>,
    slug: String,
) -> Result<BoardStatus, String> {
    let root = root_of(&project)?;
    blocking(&gate, move || plan_status_for(&root, &slug)).await
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
        let v = check_for(&root, &c, "f", &d, Some(7)).unwrap();
        let rules: Vec<_> = v["findings"]
            .as_array()
            .unwrap()
            .iter()
            .map(|f| f["ruleId"].as_str().unwrap())
            .collect();
        assert!(rules.contains(&"design/bridge-screen-missing"), "{rules:?}");
        assert!(v["ms"].is_u64() && c.0.lock().unwrap().1.is_some());
        assert_eq!(v["seq"], 7);
        assert!(check_for(&root, &c, "../x", &d, None).is_err());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn plan_status_reports_missing_and_blocking_plans() {
        let root = std::env::temp_dir().join(format!("ig-desktop-status-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("plans/f")).unwrap();
        std::fs::write(root.join("ig.yaml"), "layers: {}\n").unwrap();
        let none = BoardStatus {
            exists: false,
            blocking: 0,
        };
        assert_eq!(plan_status_for(&root, "f"), Ok(none));
        std::fs::write(
            root.join("plans/f/plan.md"),
            "# F\n\n```ig-plan\nnodes: []\n```\n",
        )
        .unwrap();
        let clean = BoardStatus {
            exists: true,
            blocking: 0,
        };
        assert_eq!(plan_status_for(&root, "f"), Ok(clean));
        // a design element the plan neither builds nor omits: a blocking design gap
        std::fs::write(
            root.join("plans/f/design.json"),
            r#"{"version":1,"title":"D","scope":{"kind":"dir","id":"."},"elements":[{"id":"mod-a","kind":"module","name":"a","file":"z.ts"}]}"#,
        )
        .unwrap();
        assert_eq!(plan_status_for(&root, "f").unwrap().blocking, 1);
        assert!(!root.join(".ig/current-plan").exists(), "never recorded");
        assert!(plan_status_for(&root, "../x").is_err());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_load_that_a_clear_overtook_is_not_cached() {
        let c = HeadCache::default();
        let load = || {
            c.clear(); // the code changed while the graph was being built
            Ok(ig_app::plan::Head {
                cfg: ig_core::config::Config::default(),
                graph: ig_core::model::Graph::empty(),
                packages: vec![],
            })
        };
        assert!(c.get(load).is_ok());
        assert!(c.0.lock().unwrap().1.is_none());
    }

    #[cfg(unix)]
    #[test]
    fn a_symlinked_plans_folder_is_refused() {
        let root = std::env::temp_dir().join(format!("ig-desktop-link-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        std::os::unix::fs::symlink("/tmp", root.join("plans")).unwrap();
        assert!(ig_wire::real_plans(&root).is_err());
        let _ = std::fs::remove_dir_all(&root);
    }
}
