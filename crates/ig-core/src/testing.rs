//! In-memory fakes of the ports, and the contract every implementation must meet. Adapter crates
//! call the `*_contract` functions on their real implementation.

use crate::config::{Checker, Config, Format};
use crate::model::Graph;
use crate::ports::*;
use crate::workspace::{SourceFile, WorkspaceInfo, globset};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;
use std::sync::Mutex;

/// Files in a map.
#[derive(Default)]
pub struct MemFiles {
    pub files: Mutex<BTreeMap<String, Vec<u8>>>,
}

impl MemFiles {
    pub fn with(files: &[(&str, &str)]) -> MemFiles {
        let m = MemFiles::default();
        for (p, t) in files {
            m.write_atomic(p, t.as_bytes()).unwrap();
        }
        m
    }
}

impl Files for MemFiles {
    fn read(&self, path: &str) -> Option<String> {
        let files = self.files.lock().unwrap();
        files
            .get(path)
            .map(|b| String::from_utf8_lossy(b).into_owned())
    }
    fn write_atomic(&self, path: &str, bytes: &[u8]) -> Result<(), PortError> {
        self.files
            .lock()
            .unwrap()
            .insert(path.into(), bytes.to_vec());
        Ok(())
    }
    fn remove(&self, path: &str) -> Result<(), PortError> {
        self.files.lock().unwrap().remove(path);
        Ok(())
    }
    fn exists(&self, path: &str) -> bool {
        self.files.lock().unwrap().contains_key(path)
    }
    fn glob(&self, patterns: &[String]) -> Result<Vec<String>, PortError> {
        let set = globset(patterns).map_err(|e| PortError::Io(e.to_string()))?;
        let files = self.files.lock().unwrap();
        Ok(files.keys().filter(|p| set.is_match(p)).cloned().collect())
    }
}

/// A scripted repository. `resolve_base(None)` returns `base`; an explicit rev must be in
/// `commits`. `base_graph` returns the scripted graph of the rev.
#[derive(Default)]
pub struct FakeVcs {
    pub head: Option<String>,
    pub base: Option<String>,
    pub commits: HashSet<String>,
    /// (rev, path) → contents.
    pub shows: HashMap<(String, String), String>,
    pub changed: Vec<String>,
    pub clean: bool,
    pub history: Vec<Vec<String>>,
    pub graphs: HashMap<String, Graph>,
}

impl Vcs for FakeVcs {
    fn head(&self) -> Option<String> {
        self.head.clone()
    }
    fn resolve_base(
        &self,
        explicit: Option<&str>,
        _notices: &mut Vec<String>,
    ) -> Result<Option<String>, PortError> {
        match explicit {
            Some(b) if self.commits.contains(b) => Ok(Some(b.into())),
            Some(b) => Err(PortError::Vcs(format!("--base {b}: not a commit"))),
            None => Ok(self.base.clone()),
        }
    }
    fn show(&self, rev: &str, path: &str) -> Option<String> {
        self.shows.get(&(rev.into(), path.into())).cloned()
    }
    fn changed_files(&self, _base: Option<&str>, _notices: &mut Vec<String>) -> Vec<String> {
        self.changed.clone()
    }
    fn commit_exists(&self, rev: &str) -> bool {
        self.commits.contains(rev)
    }
    fn is_clean(&self) -> bool {
        self.clean
    }
    fn history(&self, n: usize) -> Vec<Vec<String>> {
        self.history.iter().take(n).cloned().collect()
    }
    fn checkout(&self, rev: &str) -> Result<Checkout, PortError> {
        Err(PortError::Vcs(format!("FakeVcs cannot check out {rev}")))
    }
    fn base_graph(
        &self,
        rev: &str,
        _build: &dyn Fn(&Path) -> Result<Graph, PortError>,
    ) -> Result<Graph, PortError> {
        self.graphs
            .get(rev)
            .cloned()
            .ok_or_else(|| PortError::Vcs(format!("no graph scripted for {rev}")))
    }
}

/// Canned output per checker name; records each run as (checker, files).
#[derive(Default)]
pub struct FakeRunner {
    pub outputs: HashMap<String, CheckerOutput>,
    pub runs: Mutex<Vec<(String, Option<Vec<String>>)>>,
}

impl CheckerRunner for FakeRunner {
    fn run(&self, _root: &Path, checker: &Checker, files: Option<&[String]>) -> CheckerOutput {
        if checker.run.contains("{files}") && files.is_some_and(<[String]>::is_empty) {
            return CheckerOutput::default();
        }
        self.runs
            .lock()
            .unwrap()
            .push((checker.name.clone(), files.map(<[String]>::to_vec)));
        match self.outputs.get(&checker.name) {
            Some(o) => CheckerOutput {
                findings: o.findings.clone(),
                errors: o.errors.clone(),
                notices: o.notices.clone(),
            },
            None => CheckerOutput::default(),
        }
    }
}

/// `f` must start empty.
pub fn files_contract(f: &dyn Files) {
    assert_eq!(f.read("missing.txt"), None);
    assert!(!f.exists("missing.txt"));
    f.write_atomic("dir/sub/a.txt", b"one").unwrap();
    assert_eq!(
        f.read("dir/sub/a.txt").as_deref(),
        Some("one"),
        "round-trips"
    );
    assert!(f.exists("dir/sub/a.txt"));
    f.write_atomic("dir/sub/a.txt", b"two").unwrap();
    assert_eq!(
        f.read("dir/sub/a.txt").as_deref(),
        Some("two"),
        "overwrites"
    );
    for p in ["b.ts", "a.ts", "src/c.ts", "src/a.ts", "x.md"] {
        f.write_atomic(p, b"").unwrap();
    }
    let glob = |p: &[&str]| f.glob(&p.iter().map(|s| s.to_string()).collect::<Vec<_>>());
    assert_eq!(
        glob(&["*.ts"]).unwrap(),
        ["a.ts", "b.ts"],
        "`*` stops at `/`"
    );
    assert_eq!(
        glob(&["src/**", "*.ts"]).unwrap(),
        ["a.ts", "b.ts", "src/a.ts", "src/c.ts"],
        "sorted across patterns"
    );
    assert!(glob(&["a{"]).is_err(), "bad pattern");
    f.remove("b.ts").unwrap();
    assert!(!f.exists("b.ts"));
    assert_eq!(f.read("b.ts"), None);
    f.remove("b.ts").expect("removing a missing file is fine");
}

/// `v` has at least one commit, and `file` holds `content` at HEAD.
pub fn vcs_contract(v: &dyn Vcs, file: &str, content: &str) {
    let head = v.head().expect("a head commit");
    assert!(v.commit_exists(&head));
    assert!(!v.commit_exists("0123456789abcdef0123456789abcdef01234567"));
    assert_eq!(v.show(&head, file).as_deref(), Some(content));
    assert_eq!(v.show(&head, "no/such/file.txt"), None, "missing path");
    let mut notices = vec![];
    assert_eq!(v.resolve_base(Some(&head), &mut notices), Ok(Some(head)));
    assert!(v.resolve_base(Some("no-such-rev"), &mut notices).is_err());
}

/// `checker` uses `{files}`.
pub fn runner_contract(r: &dyn CheckerRunner, root: &Path, checker: &Checker) {
    assert!(checker.run.contains("{files}"));
    let out = r.run(root, checker, Some(&[]));
    assert!(
        out.findings.is_empty() && out.errors.is_empty(),
        "no files: skipped"
    );
}

/// `fe` handles `path`, whose `source` parses.
pub fn frontend_contract(fe: &dyn LanguageFrontend, path: &str, source: &str) {
    assert!(fe.handles(path));
    let file = SourceFile {
        path: path.into(),
        package: 0,
    };
    let ws = WorkspaceInfo {
        root: ".".into(),
        packages: vec![crate::workspace::Package {
            name: "root".into(),
            dir: ".".into(),
            tsconfig: None,
            source_entry: None,
        }],
        files: vec![file.clone()],
    };
    let config = Config::default();
    let facts = fe.extract(&file, source, &ws, &config);
    assert_eq!(facts.file, path);
    assert_eq!(facts.parse_error, None);
    assert!(
        facts.nodes.iter().all(|n| n.file == path),
        "nodes of this file"
    );
    let r = fe.resolve(&[facts], &ws, &config);
    assert!(r.edges.iter().all(|e| e.file == path));
}

/// A `{files}` checker for `runner_contract`.
pub fn files_checker() -> Checker {
    Checker {
        name: "lint".into(),
        run: "lint {files}".into(),
        format: Format::Lines,
        configs: vec![],
        extensions: vec![],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Metrics, Node, Range};

    #[test]
    fn mem_files_meets_the_contract() {
        files_contract(&MemFiles::default());
    }

    #[test]
    fn fake_vcs_meets_the_contract() {
        let v = FakeVcs {
            head: Some("abc".into()),
            commits: ["abc".to_string()].into(),
            shows: [(("abc".into(), "intents.md".into()), "# I\n".into())].into(),
            ..FakeVcs::default()
        };
        vcs_contract(&v, "intents.md", "# I\n");
    }

    #[test]
    fn fake_runner_meets_the_contract_and_records_runs() {
        let r = FakeRunner::default();
        runner_contract(&r, Path::new("."), &files_checker());
        assert!(r.runs.lock().unwrap().is_empty());
        r.run(Path::new("."), &files_checker(), Some(&["a.ts".into()]));
        assert_eq!(
            r.runs.lock().unwrap()[0],
            ("lint".into(), Some(vec!["a.ts".into()]))
        );
    }

    /// One module node per file; exercises `frontend_contract` itself.
    struct OneNode;
    impl LanguageFrontend for OneNode {
        fn handles(&self, path: &str) -> bool {
            path.ends_with(".x")
        }
        fn extract(&self, f: &SourceFile, _: &str, _: &WorkspaceInfo, _: &Config) -> FileFacts {
            let n = Node {
                id: f.path.clone(),
                kind: "module".into(),
                name: f.path.clone(),
                file: f.path.clone(),
                range: Range {
                    start_line: 1,
                    end_line: 1,
                },
                parent: None,
                package: None,
                exported: false,
                hash: String::new(),
                metrics: Metrics {
                    loc: 1,
                    complexity: 1,
                },
                intents: vec![],
            };
            FileFacts {
                file: f.path.clone(),
                nodes: vec![n],
                ..FileFacts::default()
            }
        }
        fn resolve(&self, _: &[FileFacts], _: &WorkspaceInfo, _: &Config) -> Resolved {
            Resolved::default()
        }
    }

    #[test]
    fn frontend_contract_runs() {
        frontend_contract(&OneNode, "a.x", "");
    }
}
