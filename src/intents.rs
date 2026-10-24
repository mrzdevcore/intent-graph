//! `intents.md` parsing and changed intents.

use regex::Regex;
use serde::Serialize;
use std::sync::LazyLock;

static LINE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*(?:[-*]\s+)?([A-Z]+-\d+):\s*(.+)$").unwrap());

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Intent {
    pub id: String,
    pub text: String,
    pub line: u32,
}

pub fn parse(text: &str) -> Vec<Intent> {
    text.lines()
        .enumerate()
        .filter_map(|(i, l)| {
            let c = LINE.captures(l)?;
            Some(Intent {
                id: c[1].to_string(),
                text: c[2].trim().to_string(),
                line: i as u32 + 1,
            })
        })
        .collect()
}

#[derive(Debug, Clone, Serialize)]
pub struct IntentChange {
    pub status: crate::diff::Status,
    pub id: String,
    pub text: String,
}

/// Intents added, changed (different text) or removed between base and head.
pub fn changes(base: &[Intent], head: &[Intent]) -> Vec<IntentChange> {
    use crate::diff::Status;
    let mut out = vec![];
    for h in head {
        match base.iter().find(|b| b.id == h.id) {
            None => out.push(IntentChange {
                status: Status::Added,
                id: h.id.clone(),
                text: h.text.clone(),
            }),
            Some(b) if b.text != h.text => out.push(IntentChange {
                status: Status::Changed,
                id: h.id.clone(),
                text: h.text.clone(),
            }),
            _ => {}
        }
    }
    for b in base {
        if !head.iter().any(|h| h.id == b.id) {
            out.push(IntentChange {
                status: Status::Removed,
                id: b.id.clone(),
                text: b.text.clone(),
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_lines() {
        let i = parse("# Intents\n- CORE-1: add numbers\n* UI-2:  render\nnope\n  AB-3: x\n");
        assert_eq!(
            i.iter().map(|x| x.id.as_str()).collect::<Vec<_>>(),
            ["CORE-1", "UI-2", "AB-3"]
        );
        assert_eq!(i[1].text, "render");
        assert_eq!(i[1].line, 3);
        let c = changes(&i[..2], &parse("- CORE-1: add numbers!\n- NEW-1: x\n"));
        assert_eq!(c.len(), 3);
    }
}
