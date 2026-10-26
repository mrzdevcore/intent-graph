//! The use cases, on top of `ig-core` and its ports (no I/O of its own).

pub mod apply;
pub mod base;
pub mod baseline;
pub mod brief;
pub mod build;
pub mod check;
pub mod config;
mod error;
pub mod lanes;
pub mod notes;
pub mod plan;
pub mod query;
pub mod review;
pub mod setup;
pub mod view_data;

pub(crate) use error::bail;
pub use error::{Error, Result};

use ig_core::config::Config;
use ig_core::ports::{CheckerRunner, Files, LanguageFrontend, Progress, Vcs};
use std::path::Path;

/// The ports a use case runs on. `files` and `vcs` are rooted at `root`, the ig root.
#[derive(Clone, Copy)]
pub struct Env<'a> {
    pub root: &'a Path,
    pub files: &'a dyn Files,
    /// None outside a repository.
    pub vcs: Option<&'a dyn Vcs>,
    pub runner: &'a dyn CheckerRunner,
    pub frontends: &'a [&'a dyn LanguageFrontend],
    /// Progress lines ("building graph…"); the binary shows them only on a terminal.
    pub progress: &'a Progress,
    /// Notices a check found ("not a git repository…"); the binary always shows them.
    pub notice: &'a Progress,
}

/// `path`'s text; empty when it is missing or not UTF-8 (as `read_to_string().unwrap_or_default()`).
pub fn read_text(files: &dyn Files, path: &str) -> String {
    files
        .read_bytes(path)
        .and_then(|b| String::from_utf8(b).ok())
        .unwrap_or_default()
}

/// `path` parsed as JSON; None when missing, not UTF-8 or not valid.
pub fn read_json<T: serde::de::DeserializeOwned>(files: &dyn Files, path: &str) -> Option<T> {
    let text = String::from_utf8(files.read_bytes(path)?).ok()?;
    serde_json::from_str(&text).ok()
}

/// The graph of the tree `files` is rooted at, with `commit` set.
pub fn graph(
    env: &Env,
    files: &dyn Files,
    config: &Config,
    commit: Option<String>,
) -> Result<build::Built> {
    let mut built = build::build_graph(env.frontends, files, config)?;
    built.graph.commit = commit;
    Ok(built)
}

#[cfg(test)]
pub(crate) mod testing;

#[cfg(test)]
#[test]
fn env_is_send_and_sync() {
    fn assert<T: Send + Sync>() {}
    assert::<Env<'static>>();
}
