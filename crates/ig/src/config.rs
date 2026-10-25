//! `ig_core::config` plus reading `ig.yaml` from disk.

pub use ig_core::config::*;

use std::path::Path;

/// `<root>/ig.yaml` parsed; the default config when there is none.
pub fn load(root: &Path) -> anyhow::Result<Config> {
    match std::fs::read_to_string(root.join("ig.yaml")) {
        Ok(text) => Ok(Config::parse(&text)?),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Config::default()),
        Err(e) => anyhow::bail!("ig.yaml: {e}"),
    }
}
