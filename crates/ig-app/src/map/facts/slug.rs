//! Ids: slugs of names, and repeats told apart.

/// Longest id (or component id part after the container's `/`).
const MAX_ID: usize = 64;

/// `[a-z0-9-]`: lowercase ASCII, runs of anything else as one `-`, trimmed, at most 64 chars;
/// `fallback` when nothing is left.
pub(crate) fn slug(s: &str, fallback: &str) -> String {
    let s: String = s
        .to_ascii_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    let mut s = s
        .split('-')
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    s.truncate(MAX_ID);
    let s = s.trim_end_matches('-');
    if s.is_empty() { fallback } else { s }.to_string()
}

/// `ids` (sorted) with repeats (and ids in `taken`) suffixed `-2`, `-3`… in order, each still at
/// most 64 chars.
pub(in crate::map) fn dedupe(ids: Vec<String>, taken: &[&str]) -> Vec<String> {
    let mut seen: std::collections::HashSet<String> = taken.iter().map(|t| t.to_string()).collect();
    ids.into_iter()
        .map(|base| {
            let mut id = base.clone();
            let mut n = 1;
            while !seen.insert(id.clone()) {
                n += 1;
                let suffix = format!("-{n}");
                let keep = base.len().min(MAX_ID - suffix.len());
                id = format!("{}{suffix}", base[..keep].trim_end_matches('-'));
            }
            id
        })
        .collect()
}
