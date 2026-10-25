//! The four boundaries ig talks to the world through. Adapters live in their own crates; fakes
//! for tests live in `crate::testing` (feature `testing`).

use crate::config::{Checker, Config};
use crate::http::HttpCall;
use crate::model::{Edge, Finding, Graph, Node, ParseError};
use crate::workspace::{SourceFile, WorkspaceInfo};
use std::any::Any;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// A port failure. Messages are shown to the user as they are.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum PortError {
    #[error("{0}")]
    Io(String),
    #[error("{0}")]
    Vcs(String),
    /// A graph build run inside `Vcs::base_graph`.
    #[error("{0}")]
    Build(String),
}

/// Files under a root fixed at construction; paths are posix and relative to that root.
pub trait Files: Sync {
    /// UTF-8 (lossy) contents; None when missing or unreadable.
    fn read(&self, path: &str) -> Option<String>;
    /// Writes through a temporary file and a rename; creates parent directories.
    fn write_atomic(&self, path: &str, bytes: &[u8]) -> Result<(), PortError>;
    /// Removing a missing file is not an error.
    fn remove(&self, path: &str) -> Result<(), PortError>;
    fn exists(&self, path: &str) -> bool;
    /// Files matching any pattern (`crate::workspace::glob` flavour), sorted. Err on a bad pattern.
    fn glob(&self, patterns: &[String]) -> Result<Vec<String>, PortError>;
}

/// A materialised revision, removed when dropped.
pub struct Checkout {
    /// The ig root inside the checkout; None when it did not exist at that revision.
    pub root: Option<PathBuf>,
    /// Cleans the checkout up on drop.
    pub guard: Box<dyn Send>,
}

/// The version control system the ig root lives in.
pub trait Vcs {
    /// The HEAD commit; None without commits.
    fn head(&self) -> Option<String>;
    /// `explicit` (Err when it is not a commit), else the CI base, else the merge base with
    /// origin/HEAD, main or master, else HEAD. Pushes a notice for each fallback. None without
    /// commits.
    fn resolve_base(
        &self,
        explicit: Option<&str>,
        notices: &mut Vec<String>,
    ) -> Result<Option<String>, PortError>;
    /// `path` (relative to the ig root) at `rev`; None when missing.
    fn show(&self, rev: &str, path: &str) -> Option<String>;
    /// Untracked files plus, with a base, files changed against it. Failures become notices.
    fn changed_files(&self, base: Option<&str>, notices: &mut Vec<String>) -> Vec<String>;
    fn commit_exists(&self, rev: &str) -> bool;
    /// No uncommitted changes.
    fn is_clean(&self) -> bool;
    /// The files each of the last `n` commits changed, newest first; empty without history.
    fn history(&self, n: usize) -> Vec<Vec<String>>;
    /// A temporary checkout of `rev`.
    fn checkout(&self, rev: &str) -> Result<Checkout, PortError>;
    /// The graph of `rev`, from the cache or by calling `build` on the ig root of a checkout.
    fn base_graph(
        &self,
        rev: &str,
        build: &dyn Fn(&Path) -> Result<Graph, PortError>,
    ) -> Result<Graph, PortError>;
}

/// What running checkers produced. Findings carry no node ids; callers attach them.
#[derive(Debug, Default)]
pub struct CheckerOutput {
    pub findings: Vec<Finding>,
    pub errors: Vec<Finding>,
    pub notices: Vec<String>,
}

pub trait CheckerRunner: Sync {
    /// Runs `checker` in `root`. `files` fills `{files}`: the files it should check, already
    /// filtered (None: the whole root). An empty list skips a `{files}` checker.
    fn run(&self, root: &Path, checker: &Checker, files: Option<&[String]>) -> CheckerOutput;
}

/// One file's facts. `lang` holds what only the frontend that produced them reads in `resolve`.
pub struct FileFacts {
    pub file: String,
    pub nodes: Vec<Node>,
    pub parse_error: Option<String>,
    pub http: Vec<HttpCall>,
    /// `const NAME = '<url-ish string>'`, for route paths built from constants.
    pub url_consts: HashMap<String, String>,
    /// Callee name → calls whose first argument looks like a URL.
    pub url_callees: HashMap<String, u32>,
    pub lang: Box<dyn Any + Send + Sync>,
}

impl Default for FileFacts {
    fn default() -> FileFacts {
        FileFacts {
            file: String::new(),
            nodes: vec![],
            parse_error: None,
            http: vec![],
            url_consts: HashMap::new(),
            url_callees: HashMap::new(),
            lang: Box::new(()),
        }
    }
}

/// What resolving a frontend's files produced.
#[derive(Debug, Default)]
pub struct Resolved {
    pub edges: Vec<Edge>,
    pub parse_errors: Vec<ParseError>,
    /// Imports of external packages: (file, package, line).
    pub packages: Vec<(String, String, u32)>,
}

pub trait LanguageFrontend: Sync {
    fn handles(&self, path: &str) -> bool;
    /// Per file, no cross-file work; called in parallel.
    fn extract(
        &self,
        file: &SourceFile,
        source: &str,
        ws: &WorkspaceInfo,
        config: &Config,
    ) -> FileFacts;
    /// Cross-file resolution over the facts this frontend extracted.
    fn resolve(&self, facts: &[FileFacts], ws: &WorkspaceInfo, config: &Config) -> Resolved;
}
