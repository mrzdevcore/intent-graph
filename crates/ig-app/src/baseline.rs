//! `.ig/baseline.json`: accepted fingerprints and exceptions.

use crate::Env;
use ig_core::rules::baseline;
use std::collections::HashSet;

pub const PATH: &str = ".ig/baseline.json";

/// Baseline entries: fingerprint strings, or `{"fingerprint", "reason"}` exceptions. Empty when
/// the file is absent; an error when it exists but doesn't parse.
pub fn read(env: &Env) -> anyhow::Result<Vec<serde_json::Value>> {
    let shown = env.root.join(PATH);
    match env.files.read_bytes(PATH) {
        None => Ok(vec![]),
        Some(b) => {
            let text = String::from_utf8(b).map_err(|_| {
                anyhow::anyhow!("{}: stream did not contain valid UTF-8", shown.display())
            })?;
            baseline::parse(&text).map_err(|e| anyhow::anyhow!("{}: {e}", shown.display()))
        }
    }
}

/// The baselined fingerprints; none when the file is bad.
pub fn load(env: &Env) -> HashSet<String> {
    baseline::fingerprints(&read(env).unwrap_or_default())
}

/// Writes `fingerprints` as strings, keeping existing exception objects.
pub fn write(env: &Env, fingerprints: impl IntoIterator<Item = String>) -> anyhow::Result<()> {
    let items = baseline::with_fingerprints(fingerprints, read(env).unwrap_or_default());
    env.files
        .write_atomic(PATH, baseline::render(&items)?.as_bytes())?;
    Ok(())
}

/// Exceptions (entries with a reason): (fingerprint, reason).
pub fn exceptions(items: &[serde_json::Value]) -> Vec<(String, String)> {
    items
        .iter()
        .filter_map(|v| Some((v["fingerprint"].as_str()?, v["reason"].as_str()?)))
        .map(|(fp, r)| (fp.to_string(), r.to_string()))
        .collect()
}
