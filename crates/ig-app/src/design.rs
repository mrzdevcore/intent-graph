//! Features: a folder `plans/<slug>/` (design.json, plan.md, notes.md, screens/, assets/) or a
//! flat plan `plans/<slug>.md`. Design files are user content: written only on the user's
//! action, atomically and never over a file this ig cannot read.

use crate::{Result, bail, read_text};
use ig_core::design::{self, Design, Scope};
use ig_core::ports::Files;
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Serialize, Debug, Clone, PartialEq)]
pub struct Feature {
    pub slug: String,
    pub title: String,
    pub design: bool,
    /// The plan file, when there is one.
    pub plan: Option<String>,
    pub done: bool,
    /// A flat plan (`plans/<slug>.md`), no folder.
    pub flat: bool,
}

/// `^[a-z0-9][a-z0-9-]{0,63}$`: feature slugs and screen ids.
pub fn valid_slug(s: &str) -> bool {
    let b = s.as_bytes();
    !b.is_empty()
        && b.len() <= 64
        && (b[0].is_ascii_lowercase() || b[0].is_ascii_digit())
        && b.iter()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == b'-')
}

/// "Patient export!" -> "patient-export"; empty when the title has no letter or digit.
pub fn slug_of(title: &str) -> String {
    let s = title
        .to_ascii_lowercase()
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    s.chars()
        .take(64)
        .collect::<String>()
        .trim_end_matches('-')
        .into()
}

pub fn design_path(slug: &str) -> String {
    format!("plans/{slug}/design.json")
}

fn check_slug(slug: &str) -> Result<()> {
    if !valid_slug(slug) {
        bail!("bad feature name {slug:?}");
    }
    Ok(())
}

/// Every feature, by slug. A folder wins over a flat plan of the same name.
pub fn list(files: &dyn Files) -> Vec<Feature> {
    let pats = ["plans/*/design.json", "plans/*/plan.md", "plans/*.md"].map(String::from);
    let mut by: BTreeMap<String, Feature> = BTreeMap::new();
    for p in files.glob(&pats).unwrap_or_default() {
        let parts: Vec<&str> = p.split('/').collect();
        let (slug, flat) = match parts[..] {
            [_, s, _] => (s, false),
            [_, f] => (f.trim_end_matches(".md"), true),
            _ => continue,
        };
        if !valid_slug(slug) {
            continue;
        }
        let f = by.entry(slug.into()).or_insert_with(|| Feature {
            slug: slug.into(),
            title: slug.into(),
            design: false,
            plan: None,
            done: false,
            flat,
        });
        if f.flat && !flat {
            f.flat = false;
            f.plan = None;
            f.done = false;
        }
        if flat && !f.flat {
            continue;
        }
        if parts.last() == Some(&"design.json") {
            f.design = true;
            if let Ok(d) = design::parse(&read_text(files, &p)) {
                f.title = d.title;
            }
        } else {
            // Read by line, not parsed: a plan with a broken block still is a feature.
            let text = read_text(files, &p);
            let title = text
                .lines()
                .find_map(|l| l.strip_prefix("# "))
                .map(str::trim);
            f.done = text.lines().any(|l| l.trim() == "Status: done");
            f.plan = Some(p);
            if let Some(t) = title.filter(|t| !t.is_empty() && !f.design) {
                f.title = t.into();
            }
        }
    }
    by.into_values().collect()
}

/// Creates `plans/<slug>/design.json` for a title; refuses a slug that is taken.
pub fn create_as(files: &dyn Files, slug: &str, title: &str, scope: Scope) -> Result<Feature> {
    check_slug(slug)?;
    let title = title.trim();
    if title.is_empty() {
        bail!("a feature needs a title");
    }
    if list(files).iter().any(|f| f.slug == slug) || files.exists(&design_path(slug)) {
        bail!("{slug} already exists");
    }
    files.write_atomic(
        &design_path(slug),
        design::to_text(&design::empty(title, scope)).as_bytes(),
    )?;
    Ok(Feature {
        slug: slug.into(),
        title: title.into(),
        design: true,
        plan: None,
        done: false,
        flat: false,
    })
}

pub fn create(files: &dyn Files, title: &str, scope: Scope) -> Result<Feature> {
    let slug = slug_of(title);
    if slug.is_empty() {
        bail!("the title needs a letter or a digit");
    }
    create_as(files, &slug, title, scope)
}

/// `slug_of(title)`, or the first of `-2`, `-3`, ... that no feature has.
pub fn free_slug(files: &dyn Files, title: &str) -> Result<String> {
    let base = slug_of(title);
    if base.is_empty() {
        bail!("the title needs a letter or a digit");
    }
    let taken: Vec<String> = list(files).into_iter().map(|f| f.slug).collect();
    let fits = |s: &String| !taken.contains(s) && !files.exists(&design_path(s));
    let mut cands = std::iter::once(base.clone()).chain((2..).map(|n| {
        let tail = format!("-{n}");
        let head: String = base.chars().take(64 - tail.len()).collect();
        format!("{}{tail}", head.trim_end_matches('-'))
    }));
    Ok(cands.find(fits).expect("endless"))
}

/// The design and the exact bytes it was read from.
pub fn read_raw(files: &dyn Files, slug: &str) -> Result<(Design, Vec<u8>)> {
    check_slug(slug)?;
    let path = design_path(slug);
    let Some(bytes) = files.read_bytes(&path) else {
        bail!("{path}: not found");
    };
    let text = std::str::from_utf8(&bytes)
        .map_err(|_| crate::Error::Other(anyhow::anyhow!("{path}: not UTF-8")))?;
    match design::parse(text) {
        Ok(d) => Ok((d, bytes)),
        Err(e) => bail!("{path}: {e}"),
    }
}

pub fn read(files: &dyn Files, slug: &str) -> Result<Design> {
    read_raw(files, slug).map(|r| r.0)
}

/// Writes `d` unless the disk differs from `expect` (the bytes last read or written; None: no
/// file yet): then false, nothing written. A file that cannot be read is never overwritten.
// ponytail: check-then-rename is not atomic across processes; a lock file if two writers appear.
pub fn write(files: &dyn Files, slug: &str, d: &Design, expect: Option<&[u8]>) -> Result<bool> {
    check_slug(slug)?;
    let path = design_path(slug);
    let now = files.read_bytes(&path);
    if let Some(b) = &now
        && !std::str::from_utf8(b).is_ok_and(|t| design::parse(t).is_ok())
    {
        bail!("design.json unreadable; fix or delete it first");
    }
    if now.as_deref() != expect {
        return Ok(false);
    }
    let text = design::to_text(d);
    if let Err(e) = design::parse(&text) {
        bail!("{e}");
    }
    files.write_atomic(&path, text.as_bytes())?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ig_core::testing::MemFiles;

    const EMPTY: &str = r#"{"version":1,"title":"A","scope":{"kind":"layer","name":"x"}}"#;
    fn scope() -> Scope {
        Scope::Layer { name: "x".into() }
    }

    #[test]
    fn list_has_folder_and_flat_plans() {
        let f = MemFiles::with(&[
            ("plans/a/design.json", EMPTY),
            ("plans/b/plan.md", "# B\nStatus: done\n"),
            ("plans/c.md", "# C\n"),
            ("plans/c.notes.md", ""),
        ]);
        let l = list(&f);
        let slugs: Vec<_> = l.iter().map(|f| f.slug.as_str()).collect();
        assert_eq!(slugs, ["a", "b", "c"]);
        assert!(l[0].design && l[1].done && l[2].flat && !l[1].flat);
        assert_eq!(l[0].title, "A");
    }

    #[test]
    fn create_writes_an_empty_design_and_refuses_a_second_time() {
        let f = MemFiles::default();
        let c = create(&f, "Patient export!", scope()).unwrap();
        assert_eq!(c.slug, "patient-export");
        assert_eq!(read(&f, "patient-export").unwrap().title, "Patient export!");
        let e = create(&f, "Patient export!", scope()).unwrap_err();
        assert!(e.to_string().contains("already exists"));
        assert_eq!(
            free_slug(&f, "Patient export!").unwrap(),
            "patient-export-2"
        );
        assert!(create(&f, "!!", scope()).is_err());
    }

    #[test]
    fn write_with_stale_expect_writes_nothing() {
        let f = MemFiles::with(&[("plans/a/design.json", EMPTY)]);
        let d = read(&f, "a").unwrap();
        assert!(!write(&f, "a", &d, Some(b"old")).unwrap());
        assert_eq!(
            f.read_bytes("plans/a/design.json").unwrap(),
            EMPTY.as_bytes()
        );
        assert!(!write(&f, "a", &d, None).unwrap());
        assert!(write(&f, "a", &d, Some(EMPTY.as_bytes())).unwrap());
    }

    #[test]
    fn read_reports_invalid_json_and_never_writes() {
        let bad = "<<<<<<< HEAD\n{}\n";
        let f = MemFiles::with(&[("plans/a/design.json", bad)]);
        let e = read(&f, "a").unwrap_err().to_string();
        assert!(e.starts_with("plans/a/design.json: "), "{e}");
        let d = design::empty("A", scope());
        let e = write(&f, "a", &d, Some(bad.as_bytes())).unwrap_err();
        assert_eq!(
            e.to_string(),
            "design.json unreadable; fix or delete it first"
        );
        assert_eq!(f.read_bytes("plans/a/design.json").unwrap(), bad.as_bytes());
    }

    #[test]
    fn bad_slugs_are_refused() {
        let f = MemFiles::default();
        let d = design::empty("A", scope());
        for s in ["../x", "A", "", "a/b", "-a"] {
            assert!(
                read(&f, s).is_err() && write(&f, s, &d, None).is_err(),
                "{s}"
            );
        }
        assert!(f.files.lock().unwrap().is_empty());
    }
}
