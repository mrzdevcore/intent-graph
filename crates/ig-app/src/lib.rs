//! The use cases, on top of `ig-core` and its ports (no I/O of its own).

pub mod build;
pub mod config;
mod error;
pub mod map;
pub mod setup;
pub mod show;

pub use error::{Error, Result};

use ig_core::config::Config;
use ig_core::ports::{Files, LanguageFrontend, Progress, SchemaReader, Vcs};
use std::path::Path;

/// The ports a use case runs on. `files` and `vcs` are rooted at `root`, the ig root.
#[derive(Clone, Copy)]
pub struct Env<'a> {
    pub root: &'a Path,
    pub files: &'a dyn Files,
    /// None outside a repository.
    pub vcs: Option<&'a dyn Vcs>,
    pub frontends: &'a [&'a dyn LanguageFrontend],
    /// Reads the tables the tree declares; None draws no tables.
    pub schema: Option<&'a dyn SchemaReader>,
    /// Progress lines ("building graph…"); the binary shows them only on a terminal.
    pub progress: &'a Progress,
    /// Notices a check found ("not a git repository…"); the binary always shows them.
    pub notice: &'a Progress,
}

/// The graph of the tree `files` is rooted at, with `commit` set.
pub fn graph(
    env: &Env,
    files: &dyn Files,
    config: &Config,
    commit: Option<String>,
) -> Result<build::Built> {
    let mut built = build::build_graph(env.frontends, env.schema, files, config)?;
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
