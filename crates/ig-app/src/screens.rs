//! plans/<slug>/screens/<id>.json and plans/<slug>/assets/<hash>.png|jpg. Like design.json,
//! screens are user content: written only on the user's action (or `design_edit`), atomically,
//! and never over a file this ig cannot read. Images are never executed, only stored and read.

use crate::design::valid_slug;
use crate::{Result, bail};
use ig_core::design::{self, Screen, is_asset_name};
use ig_core::ports::Files;
use std::collections::BTreeMap;

pub const MAX_ASSET: usize = 10 * 1024 * 1024;
const REFUSED: &str = "Only PNG or JPEG up to 10 MB";

fn check_slug(slug: &str) -> Result<()> {
    if !valid_slug(slug) {
        bail!("bad feature name {slug:?}");
    }
    Ok(())
}

pub fn screen_path(slug: &str, id: &str) -> String {
    format!("plans/{slug}/screens/{id}.json")
}

fn asset_path(slug: &str, name: &str) -> String {
    format!("plans/{slug}/assets/{name}")
}

/// Every screen with the exact bytes it was read from, by id. An invalid file, or one whose id
/// is not its file name, or one naming a missing screen, is an error naming that file.
pub fn read_all(files: &dyn Files, slug: &str) -> Result<BTreeMap<String, (Screen, Vec<u8>)>> {
    check_slug(slug)?;
    let pat = format!("plans/{slug}/screens/*.json");
    let mut out = BTreeMap::new();
    for path in files.glob(&[pat]).unwrap_or_default() {
        let Some(bytes) = files.read_bytes(&path) else {
            continue;
        };
        let parsed = std::str::from_utf8(&bytes)
            .map_err(|_| "not UTF-8".to_string())
            .and_then(|t| design::parse_screen(t).map_err(|e| e.to_string()));
        let s = match parsed {
            Ok(s) => s,
            Err(e) => bail!("{path}: {e}"),
        };
        if path != screen_path(slug, &s.id) {
            bail!("{path}: the id {} must match the file name", s.id);
        }
        out.insert(s.id.clone(), (s, bytes));
    }
    let all: BTreeMap<String, Screen> = out.iter().map(|(k, v)| (k.clone(), v.0.clone())).collect();
    for (id, (s, _)) in &out {
        if let Err(e) = design::check_refs(s, &all) {
            bail!("{}: {e}", screen_path(slug, id));
        }
    }
    Ok(out)
}

pub fn list(files: &dyn Files, slug: &str) -> Result<Vec<Screen>> {
    Ok(read_all(files, slug)?.into_values().map(|v| v.0).collect())
}

/// Writes `s` unless the disk differs from `expect` (the bytes last read or written; None: no
/// file yet): then false, nothing written. A file that cannot be read is never overwritten.
// ponytail: check-then-rename is not atomic across processes; a lock file if two writers appear.
pub fn write(files: &dyn Files, slug: &str, s: &Screen, expect: Option<&[u8]>) -> Result<bool> {
    check_slug(slug)?;
    let path = screen_path(slug, &s.id);
    let now = files.read_bytes(&path);
    if let Some(b) = &now
        && !std::str::from_utf8(b).is_ok_and(|t| design::parse_screen(t).is_ok())
    {
        bail!("{path} unreadable; fix or delete it first");
    }
    if now.as_deref() != expect {
        return Ok(false);
    }
    let text = design::screen_to_text(s);
    if let Err(e) = design::parse_screen(&text) {
        bail!("{path}: {e}");
    }
    files.write_atomic(&path, text.as_bytes())?;
    Ok(true)
}

pub fn remove(files: &dyn Files, slug: &str, id: &str) -> Result<()> {
    check_slug(slug)?;
    if !design::is_element_id(id) {
        bail!("bad screen id {id:?}");
    }
    files.remove(&screen_path(slug, id))?;
    Ok(())
}

/// The extension of what `bytes` is by its signature (never by a name): PNG or JPEG.
fn sniff(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("png")
    } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some("jpg")
    } else {
        None
    }
}

/// Stores a PNG or JPEG under `<hash>.<ext>`, where `hash` is the first 12 hex digits of its
/// SHA-256 (the host computes it: this crate has no hashing). Returns the name; the same bytes
/// again change nothing.
pub fn put_asset(files: &dyn Files, slug: &str, bytes: &[u8], hash: &str) -> Result<String> {
    check_slug(slug)?;
    let Some(ext) = sniff(bytes).filter(|_| bytes.len() <= MAX_ASSET) else {
        bail!("{REFUSED}");
    };
    let name = format!("{hash}.{ext}");
    if !is_asset_name(&name) {
        bail!("bad asset hash {hash:?}");
    }
    let path = asset_path(slug, &name);
    match files.read_bytes(&path) {
        Some(b) if b == bytes => {}
        Some(_) => bail!("{name} is taken by other bytes"),
        None => files.write_atomic(&path, bytes)?,
    }
    Ok(name)
}

pub fn read_asset(files: &dyn Files, slug: &str, name: &str) -> Result<Vec<u8>> {
    check_slug(slug)?;
    if !is_asset_name(name) {
        bail!("bad asset name {name:?}");
    }
    match files.read_bytes(&asset_path(slug, name)) {
        Some(b) => Ok(b),
        None => bail!("{name}: not found"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ig_core::testing::MemFiles;

    fn screen(id: &str, extra: &str) -> String {
        format!(r#"{{"version":1,"id":"{id}","title":"T","x":0,"y":0,"w":10,"h":10{extra}}}"#)
    }

    fn put(f: &MemFiles, id: &str, text: &str) {
        f.write_atomic(&screen_path("a", id), text.as_bytes())
            .unwrap();
    }

    #[test]
    fn write_list_and_remove() {
        let f = MemFiles::default();
        let s = design::parse_screen(&screen("login", "")).unwrap();
        assert!(!write(&f, "a", &s, Some(b"old")).unwrap());
        assert!(write(&f, "a", &s, None).unwrap());
        assert!(!write(&f, "a", &s, None).unwrap());
        assert_eq!(list(&f, "a").unwrap(), [s]);
        remove(&f, "a", "login").unwrap();
        assert!(list(&f, "a").unwrap().is_empty());
        assert!(
            write(
                &f,
                "../x",
                &design::parse_screen(&screen("l", "")).unwrap(),
                None
            )
            .is_err()
        );
        assert!(remove(&f, "a", "../x").is_err());
    }

    #[test]
    fn variant_needs_state_and_existing_parent() {
        let f = MemFiles::default();
        put(
            &f,
            "empty",
            &screen("empty", r#","variantOf":"home","state":"empty""#),
        );
        let e = list(&f, "a").unwrap_err().to_string();
        assert!(
            e.starts_with("plans/a/screens/empty.json: ") && e.contains("home"),
            "{e}"
        );
        put(&f, "home", &screen("home", ""));
        assert_eq!(list(&f, "a").unwrap().len(), 2);
        put(&f, "empty", &screen("empty", r#","variantOf":"home""#));
        assert!(list(&f, "a").is_err());
    }

    #[test]
    fn nav_to_missing_screen_is_reported_by_list() {
        let f = MemFiles::default();
        let nav = r#","elements":[{"id":"btn-a","kind":"button","label":"","x":0,"y":0,"w":1,"h":1}],"navs":[{"from":"btn-a","to":"gone","label":""}]"#;
        put(&f, "home", &screen("home", nav));
        let e = list(&f, "a").unwrap_err().to_string();
        assert!(
            e.starts_with("plans/a/screens/home.json: ") && e.contains("gone"),
            "{e}"
        );
    }

    #[test]
    fn file_name_must_match_id() {
        let f = MemFiles::default();
        put(&f, "other", &screen("home", ""));
        let e = list(&f, "a").unwrap_err().to_string();
        assert!(
            e.contains("plans/a/screens/other.json") && e.contains("file name"),
            "{e}"
        );
        // an invalid file is reported and never overwritten
        put(&f, "home", "<<<<");
        let s = design::parse_screen(&screen("home", "")).unwrap();
        assert!(write(&f, "a", &s, Some(b"<<<<")).is_err());
        assert_eq!(f.read_bytes(&screen_path("a", "home")).unwrap(), b"<<<<");
    }

    const PNG: &[u8] = b"\x89PNG\r\n\x1a\nrest";

    #[test]
    fn png_and_jpeg_are_stored_by_hash() {
        let f = MemFiles::default();
        let n = put_asset(&f, "a", PNG, "0123456789ab").unwrap();
        assert_eq!(n, "0123456789ab.png");
        assert_eq!(put_asset(&f, "a", PNG, "0123456789ab").unwrap(), n);
        assert_eq!(f.files.lock().unwrap().len(), 1);
        assert_eq!(read_asset(&f, "a", &n).unwrap(), PNG);
        let j = put_asset(&f, "a", &[0xFF, 0xD8, 0xFF, 0xE0, 1], "ba9876543210").unwrap();
        assert_eq!(j, "ba9876543210.jpg");
        assert!(put_asset(&f, "a", &[0x89, b'P'], "0123456789ab").is_err());
        assert!(
            put_asset(&f, "a", b"\x89PNG\r\n\x1a\nother", "0123456789ab").is_err(),
            "taken"
        );
        assert!(put_asset(&f, "a", PNG, "../../x").is_err());
    }

    #[test]
    fn svg_gif_and_renamed_files_are_refused() {
        let f = MemFiles::default();
        for b in [&b"<svg xmlns=''/>"[..], b"GIF89a....", b"%PDF-1.4", b""] {
            let e = put_asset(&f, "a", b, "0123456789ab")
                .unwrap_err()
                .to_string();
            assert_eq!(e, "Only PNG or JPEG up to 10 MB");
        }
        assert!(f.files.lock().unwrap().is_empty());
    }

    #[test]
    fn oversize_is_refused() {
        let f = MemFiles::default();
        let mut b = PNG.to_vec();
        b.resize(MAX_ASSET, 0);
        assert!(put_asset(&f, "a", &b, "0123456789ab").is_ok());
        b.push(0);
        let e = put_asset(&f, "a", &b, "ba9876543210")
            .unwrap_err()
            .to_string();
        assert_eq!(e, "Only PNG or JPEG up to 10 MB");
        assert_eq!(f.files.lock().unwrap().len(), 1);
    }

    #[test]
    fn read_asset_rejects_traversal() {
        let f = MemFiles::with(&[("x.png", "secret")]);
        for n in [
            "../x.png",
            "x.png",
            "0123456789ab.svg",
            "0123456789AB.png",
            "a/0123456789ab.png",
        ] {
            assert!(read_asset(&f, "a", n).is_err(), "{n}");
        }
        assert!(read_asset(&f, "../a", "0123456789ab.png").is_err());
    }
}
