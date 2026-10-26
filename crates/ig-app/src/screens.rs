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

/// Writes an edit's files in this order: new or changed screens, design.json, screen removals.
/// Every write is compare-and-swap against the bytes the edit was made on. Ok(false): something
/// changed before the first write, nothing was written, the caller may retry. Once a file is
/// written an error is final (the edit is never applied again) and lists what was written.
pub fn commit(
    files: &dyn Files,
    slug: &str,
    design: &design::Design,
    design_bytes: &[u8],
    old: &BTreeMap<String, (Screen, Vec<u8>)>,
    next: &design::Board,
    touched: &[String],
) -> Result<bool> {
    let changed: Vec<&String> = touched
        .iter()
        .filter(|id| next.screens.get(*id) != old.get(*id).map(|o| &o.0))
        .collect();
    let was = |id: &str| old.get(id).map(|o| o.1.as_slice());
    let dpath = crate::design::design_path(slug);
    let stale = |path: &str, bytes: Option<&[u8]>| files.read_bytes(path).as_deref() != bytes;
    if stale(&dpath, Some(design_bytes))
        || changed
            .iter()
            .any(|id| stale(&screen_path(slug, id), was(id)))
    {
        return Ok(false);
    }
    let mut wrote: Vec<String> = vec![];
    let lost = |wrote: &[String], path: &str| -> Result<bool> {
        if wrote.is_empty() {
            return Ok(false);
        }
        bail!(
            "design_edit: partial: wrote [{}]; {path} changed meanwhile, re-read and retry",
            wrote.join(", ")
        )
    };
    for id in &changed {
        if let Some(s) = next.screens.get(*id) {
            let path = screen_path(slug, id);
            if !write(files, slug, s, was(id))? {
                return lost(&wrote, &path);
            }
            wrote.push(path);
        }
    }
    if next.design != *design {
        if !crate::design::write(files, slug, &next.design, Some(design_bytes))? {
            return lost(&wrote, &dpath);
        }
        wrote.push(dpath);
    }
    for id in changed.iter().filter(|id| !next.screens.contains_key(**id)) {
        let path = screen_path(slug, id);
        if stale(&path, was(id)) {
            return lost(&wrote, &path);
        }
        remove(files, slug, id)?;
        wrote.push(path);
    }
    Ok(true)
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
    let Some(b) = files.read_bytes(&asset_path(slug, name)) else {
        bail!("{name}: not found");
    };
    // what is on disk is checked again: it may have been replaced since it was stored
    if b.len() > MAX_ASSET || sniff(&b) != name.rsplit('.').next() {
        bail!("{name}: {REFUSED}");
    }
    Ok(b)
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

    /// Runs `hook` just before the first write to `at`: another writer sneaking in.
    struct Racy {
        inner: MemFiles,
        at: String,
        hook: std::sync::Mutex<Option<(String, Vec<u8>)>>,
    }
    impl Files for Racy {
        fn read_bytes(&self, p: &str) -> Option<Vec<u8>> {
            self.inner.read_bytes(p)
        }
        fn write_atomic(
            &self,
            p: &str,
            b: &[u8],
        ) -> std::result::Result<(), ig_core::ports::PortError> {
            if p == self.at
                && let Some((v, bytes)) = self.hook.lock().unwrap().take()
            {
                self.inner.write_atomic(&v, &bytes)?;
            }
            self.inner.write_atomic(p, b)
        }
        fn remove(&self, p: &str) -> std::result::Result<(), ig_core::ports::PortError> {
            self.inner.remove(p)
        }
        fn exists(&self, p: &str) -> bool {
            self.inner.exists(p)
        }
        fn glob(
            &self,
            p: &[String],
        ) -> std::result::Result<Vec<String>, ig_core::ports::PortError> {
            self.inner.glob(p)
        }
        fn at(&self, r: &std::path::Path) -> Box<dyn Files> {
            self.inner.at(r)
        }
        fn discover(
            &self,
            c: &ig_core::config::Config,
        ) -> std::result::Result<ig_core::workspace::WorkspaceInfo, ig_core::ports::PortError>
        {
            self.inner.discover(c)
        }
    }

    fn titled(id: &str, title: &str) -> String {
        screen(id, "").replace("\"T\"", &format!("\"{title}\""))
    }

    const DESIGN: &str = r#"{"version":1,"title":"A","scope":{"kind":"layer","name":"x"}}"#;

    #[test]
    fn commit_stops_for_good_after_a_partial_write() {
        let f = Racy {
            inner: MemFiles::with(&[("plans/a/design.json", DESIGN)]),
            at: "plans/a/design.json".into(),
            hook: Default::default(),
        };
        put(&f.inner, "a", &screen("a", ""));
        put(&f.inner, "b", &screen("b", ""));
        let (d, dbytes) = crate::design::read_raw(&f, "a").unwrap();
        let old = read_all(&f, "a").unwrap();
        let mut next = design::Board {
            design: d.clone(),
            screens: old.iter().map(|(k, v)| (k.clone(), v.0.clone())).collect(),
        };
        next.design.title = "New".into();
        next.screens.get_mut("a").unwrap().title = "A2".into();
        next.screens.remove("b");
        let t = ["a".to_string(), "b".to_string()];
        // before any write: stale -> Ok(false), nothing written
        put(&f.inner, "b", &titled("b", "U"));
        assert!(!commit(&f, "a", &d, &dbytes, &old, &next, &t).unwrap());
        assert_eq!(
            f.read_bytes("plans/a/design.json").unwrap(),
            DESIGN.as_bytes()
        );
        // b changes right when design.json is written: a and design are done, b is not removed
        let old = read_all(&f, "a").unwrap();
        let theirs = titled("b", "V").into_bytes();
        *f.hook.lock().unwrap() = Some((screen_path("a", "b"), theirs.clone()));
        let e = commit(&f, "a", &d, &dbytes, &old, &next, &t)
            .unwrap_err()
            .to_string();
        assert!(
            e.starts_with("design_edit: partial: wrote [plans/a/screens/a.json, plans/a/design.json]; plans/a/screens/b.json changed meanwhile"),
            "{e}"
        );
        assert_eq!(f.read_bytes(&screen_path("a", "b")).unwrap(), theirs);
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
