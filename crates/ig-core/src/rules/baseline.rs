//! `.ig/baseline.json` contents: fingerprint strings, or `{"fingerprint", "reason"}` exceptions.
//! Reading and writing the file stays with the caller.

use serde_json::Value;
use std::collections::{BTreeSet, HashSet};

/// The baseline file's text as entries.
pub fn parse(text: &str) -> Result<Vec<Value>, serde_json::Error> {
    serde_json::from_str(text)
}

/// Every baselined fingerprint, plain or exception.
pub fn fingerprints(items: &[Value]) -> HashSet<String> {
    items
        .iter()
        .filter_map(|v| v.as_str().or_else(|| v["fingerprint"].as_str()))
        .map(String::from)
        .collect()
}

/// `fingerprints` as sorted strings, then the exception objects of `existing`.
pub fn with_fingerprints(
    fingerprints: impl IntoIterator<Item = String>,
    existing: Vec<Value>,
) -> Vec<Value> {
    let set: BTreeSet<String> = fingerprints.into_iter().collect();
    let mut items: Vec<Value> = set.into_iter().map(Into::into).collect();
    items.extend(existing.into_iter().filter(|v| v.is_object()));
    items
}

/// The file text for `items`.
pub fn render(items: &[Value]) -> Result<String, serde_json::Error> {
    Ok(serde_json::to_string_pretty(items)? + "\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_exceptions_and_sorts() {
        let old = parse(r#"["z", {"fingerprint": "e", "reason": "ok"}]"#).unwrap();
        let items = with_fingerprints(["b".to_string(), "a".to_string(), "a".to_string()], old);
        assert_eq!(
            render(&items).unwrap(),
            "[\n  \"a\",\n  \"b\",\n  {\n    \"fingerprint\": \"e\",\n    \"reason\": \"ok\"\n  }\n]\n"
        );
        assert_eq!(
            fingerprints(&items),
            ["a", "b", "e"].map(String::from).into()
        );
        assert!(parse("{oops").is_err());
    }
}
