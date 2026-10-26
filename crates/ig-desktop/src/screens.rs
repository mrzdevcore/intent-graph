//! Screen and image commands. Like design.json, the app remembers the bytes it last read or
//! wrote per screen file: a write is refused ("conflict") when the disk moved on, and a watched
//! change that equals them is the app's own autosave, not news.

use crate::bounded::Gate;
use crate::design::{Known, blocking, conflict, root_of};
use base64::Engine as _;
use ig_app::screens as app;
use ig_core::design::{Screen, screen_to_text};
use ig_core::ports::Files;
use ig_wire::Wired;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::path::Path;
use tauri::{Emitter, Manager};

pub fn list_for(root: &Path, known: &Known, slug: &str) -> Result<Vec<Screen>, String> {
    let all = app::read_all(Wired::new(root).files(), slug).map_err(|e| e.to_string())?;
    let mut k = known.0.lock().unwrap();
    let pre = format!("plans/{slug}/screens/");
    k.retain(|p, _| !p.starts_with(&pre)); // a screen removed outside is not remembered
    let mut out = vec![];
    for (id, (s, bytes)) in all {
        k.insert(app::screen_path(slug, &id), bytes);
        out.push(s);
    }
    Ok(out)
}

pub fn write_for(root: &Path, known: &Known, slug: &str, s: &Screen) -> Result<(), String> {
    let path = app::screen_path(slug, &s.id);
    // Held across the write: a second autosave must see the first one's bytes.
    let mut k = known.0.lock().unwrap();
    let expect = k.get(&path).cloned();
    let w = Wired::new(root);
    if !app::write(w.files(), slug, s, expect.as_deref()).map_err(|e| e.to_string())? {
        return Err(conflict());
    }
    k.insert(path, screen_to_text(s).into_bytes());
    Ok(())
}

pub fn remove_for(root: &Path, known: &Known, slug: &str, id: &str) -> Result<(), String> {
    if !ig_app::design::valid_slug(slug) || !ig_core::design::is_element_id(id) {
        return Err(format!("bad screen {slug:?}/{id:?}"));
    }
    let path = app::screen_path(slug, id);
    let mut k = known.0.lock().unwrap();
    let w = Wired::new(root);
    if w.files().read_bytes(&path) != k.get(&path).cloned() {
        return Err(conflict());
    }
    app::remove(w.files(), slug, id).map_err(|e| e.to_string())?;
    k.remove(&path);
    Ok(())
}

/// Stores a PNG or JPEG sent as base64; the name is the first 12 hex digits of its SHA-256.
pub fn put_asset_for(root: &Path, slug: &str, data: &str) -> Result<String, String> {
    let refused = || "Only PNG or JPEG up to 10 MB".to_string();
    // refuse before decoding what could not fit anyway
    if data.len() > app::MAX_ASSET / 3 * 4 + 8 {
        return Err(refused());
    }
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(data)
        .map_err(|_| refused())?;
    let hash: String = Sha256::digest(&bytes)
        .iter()
        .take(6)
        .map(|b| format!("{b:02x}"))
        .collect();
    app::put_asset(Wired::new(root).files(), slug, &bytes, &hash).map_err(|e| e.to_string())
}

pub fn read_asset_for(root: &Path, slug: &str, name: &str) -> Result<Vec<u8>, String> {
    // only a regular file of a sane size: never through a symlink, never a huge file in memory
    let p = root.join(format!("plans/{slug}/assets/{name}"));
    if let Ok(m) = std::fs::symlink_metadata(&p)
        && (!m.file_type().is_file() || m.len() > ig_app::screens::MAX_ASSET as u64)
    {
        return Err(format!("{name}: not a regular image file"));
    }
    app::read_asset(Wired::new(root).files(), slug, name).map_err(|e| e.to_string())
}

/// Screens that differ from what the app last read or wrote, and the images that appeared.
pub fn emit_changed(
    app: &tauri::AppHandle,
    root: &Path,
    screens: &BTreeSet<(String, String)>,
    assets: &BTreeSet<(String, String)>,
) {
    let files = Wired::new(root);
    for (slug, id) in screens {
        let p = ig_app::screens::screen_path(slug, id);
        let news =
            files.files().read_bytes(&p) != app.state::<Known>().0.lock().unwrap().get(&p).cloned();
        if news {
            let _ = app.emit(
                "screen-changed",
                serde_json::json!({ "slug": slug, "id": id }),
            );
        }
    }
    for (slug, name) in assets {
        let _ = app.emit(
            "asset-changed",
            serde_json::json!({ "slug": slug, "name": name }),
        );
    }
}

#[tauri::command]
pub async fn list_screens(
    app: tauri::AppHandle,
    project: tauri::State<'_, crate::commands::Project>,
    gate: tauri::State<'_, Gate>,
    slug: String,
) -> Result<Vec<Screen>, String> {
    let root = root_of(&project)?;
    blocking(&gate, move || list_for(&root, &app.state::<Known>(), &slug)).await
}

#[tauri::command]
pub async fn write_screen(
    app: tauri::AppHandle,
    project: tauri::State<'_, crate::commands::Project>,
    gate: tauri::State<'_, Gate>,
    slug: String,
    screen: Screen,
) -> Result<(), String> {
    let root = root_of(&project)?;
    blocking(&gate, move || {
        write_for(&root, &app.state::<Known>(), &slug, &screen)
    })
    .await
}

#[tauri::command]
pub async fn remove_screen(
    app: tauri::AppHandle,
    project: tauri::State<'_, crate::commands::Project>,
    gate: tauri::State<'_, Gate>,
    slug: String,
    id: String,
) -> Result<(), String> {
    let root = root_of(&project)?;
    blocking(&gate, move || {
        remove_for(&root, &app.state::<Known>(), &slug, &id)
    })
    .await
}

#[tauri::command]
pub async fn put_asset(
    project: tauri::State<'_, crate::commands::Project>,
    gate: tauri::State<'_, Gate>,
    slug: String,
    base64: String,
) -> Result<String, String> {
    let root = root_of(&project)?;
    blocking(&gate, move || put_asset_for(&root, &slug, &base64)).await
}

#[tauri::command]
pub async fn read_asset(
    project: tauri::State<'_, crate::commands::Project>,
    gate: tauri::State<'_, Gate>,
    slug: String,
    name: String,
) -> Result<tauri::ipc::Response, String> {
    let root = root_of(&project)?;
    blocking(&gate, move || read_asset_for(&root, &slug, &name))
        .await
        .map(tauri::ipc::Response::new)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(n: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("ig-desktop-{n}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn screen(id: &str) -> Screen {
        ig_core::design::parse_screen(&format!(
            r#"{{"version":1,"id":"{id}","title":"T","x":0,"y":0,"w":9,"h":9}}"#
        ))
        .unwrap()
    }

    #[test]
    fn screens_conflict_like_designs_and_autosave_is_not_news() {
        let root = tmp("screens");
        let k = Known::default();
        let mut s = screen("home");
        write_for(&root, &k, "f", &s).unwrap();
        assert_eq!(list_for(&root, &k, "f").unwrap(), [s.clone()]);
        s.title = "Mine".into();
        write_for(&root, &k, "f", &s).unwrap();
        s.title = "Theirs".into();
        let p = root.join(app::screen_path("f", "home"));
        std::fs::write(&p, screen_to_text(&s)).unwrap();
        assert_eq!(write_for(&root, &k, "f", &s), Err("conflict".into()));
        assert_eq!(remove_for(&root, &k, "f", "home"), Err("conflict".into()));
        list_for(&root, &k, "f").unwrap();
        remove_for(&root, &k, "f", "home").unwrap();
        assert!(!p.exists());
        assert!(write_for(&root, &k, "../x", &s).is_err());
        for (slug, id) in [("../x", "home"), ("f", "../../ig"), ("f", "a/b")] {
            assert!(
                remove_for(&root, &k, slug, id)
                    .unwrap_err()
                    .starts_with("bad screen")
            );
        }
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn assets_are_named_by_hash_and_refused_unless_png_or_jpeg() {
        let root = tmp("assets");
        let enc = |b: &[u8]| base64::engine::general_purpose::STANDARD.encode(b);
        let png = b"\x89PNG\r\n\x1a\nxyz";
        let n = put_asset_for(&root, "f", &enc(png)).unwrap();
        assert_eq!(n.len(), 16);
        assert!(n.ends_with(".png"));
        assert_eq!(put_asset_for(&root, "f", &enc(png)).unwrap(), n);
        assert_eq!(read_asset_for(&root, "f", &n).unwrap(), png);
        for bad in [
            enc(b"<svg/>"),
            "!!not base64".into(),
            "A".repeat(15 * 1024 * 1024),
        ] {
            assert_eq!(
                put_asset_for(&root, "f", &bad),
                Err("Only PNG or JPEG up to 10 MB".into())
            );
        }
        assert!(read_asset_for(&root, "f", "../../ig.yaml").is_err());
        // a symlink named like an asset is refused, whatever it points at
        #[cfg(unix)]
        {
            std::fs::write(root.join("secret.png"), png).unwrap();
            let link = root.join("plans/f/assets/ffffffffffff.png");
            std::os::unix::fs::symlink(root.join("secret.png"), &link).unwrap();
            assert!(read_asset_for(&root, "f", "ffffffffffff.png").is_err());
        }
        // a file that is not an image any more is refused
        std::fs::write(root.join("plans/f/assets/eeeeeeeeeeee.png"), "<svg/>").unwrap();
        assert!(read_asset_for(&root, "f", "eeeeeeeeeeee.png").is_err());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[cfg(unix)]
    #[test]
    fn symlinks_below_plans_never_lead_writes_outside() {
        let (root, out) = (tmp("escape"), tmp("escape-out"));
        std::fs::create_dir_all(root.join("plans/x")).unwrap();
        for p in ["plans/x/screens", "plans/x/assets", "plans/y"] {
            std::os::unix::fs::symlink(&out, root.join(p)).unwrap();
        }
        let k = Known::default();
        let png = base64::engine::general_purpose::STANDARD.encode(b"\x89PNG\r\n\x1a\nxyz");
        let d = ig_core::design::empty("Y", ig_core::design::Scope::Dir { id: ".".into() });
        let errs = [
            write_for(&root, &k, "x", &screen("home")).unwrap_err(),
            put_asset_for(&root, "x", &png).unwrap_err(),
            crate::design::write_for(&root, &k, "y", &d).unwrap_err(),
        ];
        let left = std::fs::read_dir(&out).unwrap().count();
        let _ = (
            std::fs::remove_dir_all(&root),
            std::fs::remove_dir_all(&out),
        );
        for e in errs {
            assert!(e.contains("outside the project"), "{e}");
        }
        assert_eq!(left, 0, "nothing written outside");
    }
}
