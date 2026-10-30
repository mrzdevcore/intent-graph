//! `ig fmt`: the spec files in the writer's canonical shape. Comments and order stay.

use super::super::spec::{self, DIR, HEADER, quote};
use ig_core::ports::Files;
use kdl::KdlDocument;

/// Formats every `ig/*.kdl` and writes the ones that change (`write` false: writes nothing);
/// returns their paths. A file that does not read, or a format that would change the map, is an
/// error and nothing is written.
pub fn fmt(files: &dyn Files, write: bool) -> Result<Vec<String>, Vec<String>> {
    let paths = files
        .glob(&[format!("{DIR}/*.kdl")])
        .map_err(|e| vec![format!("{DIR}: {e}")])?;
    let old: Vec<(String, Option<String>)> =
        paths.iter().map(|p| (p.clone(), files.read(p))).collect();
    let before = spec::parse(old.clone())?;
    let mut new = vec![];
    for (p, t) in &old {
        let mut doc: KdlDocument = t
            .clone()
            .unwrap_or_default()
            .parse()
            .map_err(|_| vec![format!("{p}: cannot be read")])?;
        let headed = t.as_deref().is_some_and(|t| t.starts_with(HEADER));
        doc.autoformat();
        quote(doc.nodes_mut());
        let mut text = doc.to_string();
        if headed && !text.starts_with(HEADER) {
            text.insert_str(0, HEADER);
        }
        new.push((p.clone(), Some(text)));
    }
    if spec::parse(new.clone())? != before {
        return Err(vec![format!(
            "{DIR}: formatting would change the spec; nothing written"
        )]);
    }
    let mut changed: Vec<&(String, Option<String>)> = new
        .iter()
        .zip(&old)
        .filter(|(n, o)| n.1 != o.1)
        .map(|(n, _)| n)
        .collect();
    // system.kdl last, as every write to ig/
    changed.sort_by_key(|(p, _)| p.ends_with("/system.kdl"));
    if write {
        for (p, t) in &changed {
            let t = t.as_deref().unwrap_or_default();
            files
                .write_atomic(p, t.as_bytes())
                .map_err(|e| vec![format!("{p}: {e}")])?;
        }
    }
    Ok(changed.into_iter().map(|(p, _)| p.clone()).collect())
}
