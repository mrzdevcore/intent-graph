//! The order migrations apply in, and which files are down migrations.

use regex::Regex;
use std::cmp::Ordering;
use std::sync::LazyLock;

pub(super) static DOWN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^u\d+(?:[._]\d+)*__").unwrap());
pub(super) static FLYWAY: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[vV](\d+(?:[._]\d+)*)__").unwrap());

/// A down or rollback migration: `*.down.sql`, `*_down.sql`, `down.sql`, Flyway's `U<n>__*.sql`,
/// any file in a `down/`, `undo/` or `rollback/` folder.
pub fn is_down(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    let mut parts = lower.rsplit('/');
    let name = parts.next().unwrap_or_default();
    parts.any(|d| ["down", "undo", "rollback", "rollbacks"].contains(&d))
        || name == "down.sql"
        || [".down.sql", "_down.sql", "-down.sql"]
            .iter()
            .any(|s| name.ends_with(s))
        || DOWN.is_match(name)
}

#[derive(PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Chunk {
    Num(u128),
    Text(String),
}

/// `s` as runs of digits (as numbers) and of other characters (lower case).
pub(super) fn chunks(s: &str) -> Vec<Chunk> {
    let mut out = vec![];
    let mut rest = s;
    while let Some(ch) = rest.chars().next() {
        let digit = ch.is_ascii_digit();
        let end = rest
            .find(|c: char| c.is_ascii_digit() != digit)
            .unwrap_or(rest.len());
        let run = &rest[..end];
        out.push(match digit {
            true => Chunk::Num(run.parse().unwrap_or(u128::MAX)),
            false => Chunk::Text(run.to_ascii_lowercase()),
        });
        rest = &rest[end..];
    }
    out
}

/// Natural order: `V2__` before `V10__`; Flyway versions compared part by part; a Flyway
/// repeatable `R__` after every versioned one in its folder.
pub fn natural(a: &str, b: &str) -> Ordering {
    let split = |p: &str| -> (String, String) {
        let (dir, name) = p.rsplit_once('/').unwrap_or(("", p));
        (dir.to_string(), name.to_string())
    };
    let ((da, na), (db, nb)) = (split(a), split(b));
    let key = |n: &str| -> (bool, Option<Vec<u64>>) {
        let version = FLYWAY.captures(n).map(|c| {
            (c[1].split(['.', '_']))
                .map(|v| v.parse().unwrap_or(u64::MAX))
                .collect()
        });
        (n.starts_with("R__") || n.starts_with("r__"), version)
    };
    chunks(&da)
        .cmp(&chunks(&db))
        .then_with(|| key(&na).cmp(&key(&nb)))
        .then_with(|| chunks(&na).cmp(&chunks(&nb)))
        .then_with(|| a.cmp(b))
}
