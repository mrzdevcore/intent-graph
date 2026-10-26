//! The four boundaries ig talks to the world through. Adapters live in their own crates; fakes
//! for tests live in `crate::testing` (feature `testing`).

use crate::config::{Checker, Config};
use crate::http::HttpCall;
use crate::model::{Edge, Finding, Node, ParseError};
use crate::workspace::{SourceFile, WorkspaceInfo};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// A port failure. Messages are shown to the user as they are.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum PortError {
    #[error("{0}")]
    Io(String),
    #[error("{0}")]
    Vcs(String),
    #[error("{0}")]
    Lang(String),
}

/// Progress lines ("building graph…"). Core and app never print; the binary passes a callback
/// that writes `ig: <msg>` to stderr when it is a terminal. Sync: checkers report from threads.
pub type Progress = dyn Fn(&str) + Sync;

/// Files under a root fixed at construction; paths are posix and relative to that root.
pub trait Files: Send + Sync {
    /// None when missing or not a regular file.
    fn read_bytes(&self, path: &str) -> Option<Vec<u8>>;
    /// UTF-8 (lossy) contents.
    fn read(&self, path: &str) -> Option<String> {
        self.read_bytes(path)
            .map(|b| String::from_utf8_lossy(&b).into_owned())
    }
    /// Writes through a temporary file and a rename; creates parent directories.
    fn write_atomic(&self, path: &str, bytes: &[u8]) -> Result<(), PortError>;
    /// Removing a missing file is not an error.
    fn remove(&self, path: &str) -> Result<(), PortError>;
    /// A regular file exists at `path` (not a directory).
    fn exists(&self, path: &str) -> bool;
    /// Regular files matching any pattern (`crate::workspace::glob` flavour), sorted. Never
    /// descends into `node_modules`, `.git` or `.ig`, nor follows symlinks. Err on a bad pattern.
    fn glob(&self, patterns: &[String]) -> Result<Vec<String>, PortError>;
    /// The same kind of `Files`, rooted at `root` (a checkout).
    fn at(&self, root: &Path) -> Box<dyn Files>;
    /// Packages and source files of the tree.
    fn discover(&self, config: &Config) -> Result<WorkspaceInfo, PortError>;
}

/// A materialised revision, removed when dropped. It must outlive every `Files` or `Vcs` made
/// with `at()` on its root.
pub struct Checkout {
    root: Option<PathBuf>,
    _guard: Box<dyn Send>,
}

impl Checkout {
    /// `root`: the ig root inside the checkout, None when it did not exist at that revision.
    /// `guard` removes the checkout when dropped.
    pub fn new(root: Option<PathBuf>, guard: Box<dyn Send>) -> Checkout {
        Checkout {
            root,
            _guard: guard,
        }
    }
    pub fn root(&self) -> Option<&Path> {
        self.root.as_deref()
    }
}

/// The version control system the ig root lives in. Paths are relative to the ig root.
pub trait Vcs: Send + Sync {
    /// The same repository seen from `root` (a checkout's ig root).
    fn at(&self, root: &Path) -> Box<dyn Vcs>;
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
    /// The full sha of commit `rev`; None when it is not a commit.
    fn verify(&self, rev: &str) -> Option<String>;
    /// `path` at `rev`; None when missing.
    fn show(&self, rev: &str, path: &str) -> Option<String>;
    /// Untracked files plus, with a base, files changed against it; may include deleted ones
    /// (callers filter with `Files::exists`). Failures become notices.
    fn changed_files(&self, base: Option<&str>, notices: &mut Vec<String>) -> Vec<String>;
    /// Tracked and untracked files that are not gitignored, relative to the ig root, sorted.
    /// Empty outside a repository.
    fn list_files(&self) -> Vec<String>;
    /// No uncommitted changes.
    fn is_clean(&self) -> bool;
    /// The files each of the last `n` commits changed, newest first; empty without history.
    fn history(&self, n: usize) -> Vec<Vec<String>>;
    /// A temporary checkout of `rev`.
    fn checkout(&self, rev: &str) -> Result<Checkout, PortError>;
}

/// What running checkers produced. Findings carry no node ids; callers attach them.
#[derive(Debug, Default, Clone)]
pub struct CheckerOutput {
    pub findings: Vec<Finding>,
    pub errors: Vec<Finding>,
    pub notices: Vec<String>,
}

pub trait CheckerRunner: Send + Sync {
    /// Runs `checker` in `root`. `files` fills `{files}`: the files it should check, already
    /// filtered (None: the whole root). An empty list skips a `{files}` checker.
    fn run(&self, root: &Path, checker: &Checker, files: Option<&[String]>) -> CheckerOutput;
}

/// One file's language-neutral facts.
#[derive(Debug, Default, Clone)]
pub struct FileFacts {
    pub file: String,
    pub nodes: Vec<Node>,
    pub parse_error: Option<String>,
    pub http: Vec<HttpCall>,
    /// `const NAME = '<url-ish string>'`, for route paths built from constants.
    pub url_consts: HashMap<String, String>,
    /// Callee name → calls whose first argument looks like a URL.
    pub url_callees: HashMap<String, u32>,
}

/// What a frontend built from its files.
#[derive(Debug, Default)]
pub struct FrontendOutput {
    /// Sorted by file.
    pub facts: Vec<FileFacts>,
    pub edges: Vec<Edge>,
    pub parse_errors: Vec<ParseError>,
    /// Imports of external packages: (file, package, line).
    pub packages: Vec<(String, String, u32)>,
    pub notices: Vec<String>,
}

pub trait LanguageFrontend: Send + Sync {
    /// `config` is the one the tree is built with (`embedded` extensions, say).
    fn handles(&self, path: &str, config: &Config) -> bool;
    /// Extracts and resolves `files` (path and source) of `ws`, the ones this frontend handles.
    fn build(
        &self,
        files: &[(SourceFile, String)],
        ws: &WorkspaceInfo,
        config: &Config,
    ) -> FrontendOutput;
    /// Makes a checkout buildable from the working tree `work`: links what the language needs
    /// but git lacks (dependencies, generated config; auto-import declarations when
    /// `link_auto_imports`).
    fn prepare_checkout(
        &self,
        work: &WorkspaceInfo,
        checkout_root: &Path,
        config: &Config,
        link_auto_imports: bool,
    ) -> Result<(), PortError>;
    /// Files under `root` (posix, root-relative, sorted) outside the source files whose contents
    /// change this frontend's output: their contents key the base-graph cache.
    fn cache_inputs(&self, _root: &Path, _config: &Config) -> Vec<String> {
        vec![]
    }
}
