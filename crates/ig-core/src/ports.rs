//! The four boundaries ig talks to the world through. Adapters live in their own crates; fakes
//! for tests live in `crate::testing` (feature `testing`).

use crate::config::Config;
use crate::http::HttpCall;
use crate::model::{Anatomy, Edge, Node, ParseError};
use crate::workspace::{Layout, Package, SourceFile, WorkspaceInfo};
use std::collections::{BTreeMap, HashMap};
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
/// that writes `ig: <msg>` to stderr when it is a terminal.
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
    /// descends into `workspace::NEVER_WALKED`, nor follows symlinks. Err on a bad pattern.
    fn glob(&self, patterns: &[String]) -> Result<Vec<String>, PortError>;
    /// The same kind of `Files`, rooted at `root` (a checkout).
    fn at(&self, root: &Path) -> Box<dyn Files>;
    /// The tree's packages (`layout`'s, else the root) and their source files.
    fn discover(&self, config: &Config, layout: &Layout) -> Result<WorkspaceInfo, PortError>;
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
    /// The files directly in `dir` at `rev` (paths relative to the ig root) with their text,
    /// read in one batch; empty when there are none or `rev` is not a commit.
    fn show_dir(&self, rev: &str, dir: &str) -> Vec<(String, String)>;
    /// Untracked files plus, with a base, files changed against it; may include deleted ones
    /// (callers filter with `Files::exists`). Failures become notices.
    fn changed_files(&self, base: Option<&str>, notices: &mut Vec<String>) -> Vec<String>;
    /// The files changed between `base` and `head` (the working tree, untracked files
    /// included, when None), deleted ones too, relative to the ig root, sorted.
    fn changed_between(&self, base: &str, head: Option<&str>) -> Result<Vec<String>, PortError>;
    /// Tracked and untracked files that are not gitignored and exist on disk (tracked-but-deleted
    /// ones are left out), relative to the ig root, sorted.
    /// Empty outside a repository.
    fn list_files(&self) -> Vec<String>;
    /// No uncommitted changes.
    fn is_clean(&self) -> bool;
    /// The files each of the last `n` commits changed, newest first; empty without history.
    fn history(&self, n: usize) -> Vec<Vec<String>>;
    /// A temporary checkout of `rev`.
    fn checkout(&self, rev: &str) -> Result<Checkout, PortError>;
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
    /// The packages this language declares in the tree (a workspace's, a `pyproject.toml`'s).
    fn packages(&self, _files: &dyn Files) -> Vec<Package> {
        vec![]
    }
    /// Manifest files whose folder, when no frontend declares it, is another package's and
    /// left out.
    fn fences(&self) -> &'static [&'static str] {
        &[]
    }
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
    /// The UI components declared in `source`, the file at `path`; none when the language has
    /// no such thing or the file holds none.
    fn anatomy(&self, _path: &str, _source: &str, _config: &Config) -> Vec<Anatomy> {
        vec![]
    }
}

/// A connector's `ig.yaml` keys other than `kind`.
pub type Settings = BTreeMap<String, String>;

/// Where a resource lives: which connector, which remote, and its name there.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ResourceId {
    pub connector: String,
    pub remote: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResourceKind {
    Service,
    Route,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum State {
    Running,
    Stopped,
    Unknown,
}

/// Something running that the map can draw.
#[derive(Debug, Clone, PartialEq)]
pub struct Resource {
    pub id: ResourceId,
    pub title: String,
    pub kind: ResourceKind,
    pub parent: Option<ResourceId>,
    pub state: State,
    pub addresses: Vec<String>,
}

/// What a link points at: a known resource, or an address or exact names to look up.
#[derive(Debug, Clone, PartialEq)]
pub enum Target {
    Resource(ResourceId),
    Lookup {
        address: Option<String>,
        names: Vec<String>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Link {
    pub from: ResourceId,
    pub to: Target,
    pub label: Option<String>,
}

/// Everything one connector read.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Snapshot {
    pub resources: Vec<Resource>,
    pub links: Vec<Link>,
}

/// Reads a running system. Read-only: it never writes to `ig/`.
pub trait Connector: Send + Sync {
    fn kind(&self) -> &'static str;
    fn read(&self, settings: &Settings, files: &dyn Files) -> Result<Snapshot, PortError>;
}
