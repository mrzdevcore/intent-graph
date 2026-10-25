//! In-memory fakes of the ports, and the contract every implementation must meet. Adapter crates
//! call the `*_contract` functions on their real implementation.

use crate::config::{Checker, Config, Format};
use crate::ports::*;
use crate::workspace::{Package, SourceFile, WorkspaceInfo, globset, is_source_ext};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// Files in a shared map keyed by path; `at(dir)` views the keys under `dir/`.
#[derive(Default, Clone)]
pub struct MemFiles {
    pub files: Arc<Mutex<BTreeMap<String, Vec<u8>>>>,
    /// "" at the top, "dir/" in a view made by `at`.
    prefix: String,
}

impl MemFiles {
    pub fn with(files: &[(&str, &str)]) -> MemFiles {
        let m = MemFiles::default();
        for (p, t) in files {
            m.write_atomic(p, t.as_bytes()).unwrap();
        }
        m
    }

    /// This view's paths, relative to it.
    fn paths(&self) -> Vec<String> {
        let files = self.files.lock().unwrap();
        files
            .keys()
            .filter_map(|k| k.strip_prefix(&self.prefix))
            .map(String::from)
            .collect()
    }

    fn key(&self, path: &str) -> String {
        format!("{}{path}", self.prefix)
    }
}

/// `path`'s directories, outermost first.
fn dirs(path: &str) -> impl Iterator<Item = &str> {
    let n = path.matches('/').count();
    path.split('/').take(n)
}

impl Files for MemFiles {
    fn read_bytes(&self, path: &str) -> Option<Vec<u8>> {
        self.files.lock().unwrap().get(&self.key(path)).cloned()
    }
    fn write_atomic(&self, path: &str, bytes: &[u8]) -> Result<(), PortError> {
        let key = self.key(path);
        self.files.lock().unwrap().insert(key, bytes.to_vec());
        Ok(())
    }
    fn remove(&self, path: &str) -> Result<(), PortError> {
        self.files.lock().unwrap().remove(&self.key(path));
        Ok(())
    }
    fn exists(&self, path: &str) -> bool {
        self.files.lock().unwrap().contains_key(&self.key(path))
    }
    fn glob(&self, patterns: &[String]) -> Result<Vec<String>, PortError> {
        let set = globset(patterns).map_err(|e| PortError::Io(e.to_string()))?;
        Ok(self
            .paths()
            .into_iter()
            .filter(|p| !dirs(p).any(|d| matches!(d, "node_modules" | ".git" | ".ig")))
            .filter(|p| set.is_match(p))
            .collect())
    }
    fn at(&self, root: &Path) -> Box<dyn Files> {
        let dir = root.to_string_lossy().trim_matches('/').to_string();
        let prefix = match dir.as_str() {
            "" | "." => self.prefix.clone(),
            d => format!("{}{d}/", self.prefix),
        };
        Box::new(MemFiles {
            files: self.files.clone(),
            prefix,
        })
    }
    /// One root package (nested `package.json` dirs are not split out), and its source files
    /// outside `node_modules`, `dist`, dot dirs and `config.ignore`.
    fn discover(&self, config: &Config) -> Result<WorkspaceInfo, PortError> {
        let ignore = globset(&config.ignore).map_err(|e| PortError::Io(e.to_string()))?;
        let name = self
            .read("package.json")
            .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
            .and_then(|v| v["name"].as_str().map(String::from))
            .unwrap_or_else(|| ".".into());
        let files = self
            .paths()
            .into_iter()
            .filter(|p| {
                let mut at = String::new();
                !dirs(p).any(|d| {
                    at.push_str(d);
                    let skip = d == "node_modules" || d == "dist" || d.starts_with('.');
                    let skip = skip || ignore.is_match(&at);
                    at.push('/');
                    skip
                })
            })
            .filter(|p| {
                let base = p.rsplit('/').next().unwrap_or(p);
                is_source_ext(base, &config.embedded) && !ignore.is_match(p)
            })
            .map(|path| SourceFile { path, package: 0 })
            .collect();
        Ok(WorkspaceInfo {
            root: PathBuf::from(match self.prefix.as_str() {
                "" => ".",
                p => p.trim_end_matches('/'),
            }),
            packages: vec![Package {
                name,
                dir: ".".into(),
                tsconfig: None,
                source_entry: None,
            }],
            files,
        })
    }
}

/// A scripted repository. `resolve_base(None)` returns `base`; explicit revs and `verify` accept
/// `commits` only. `checkout` of a commit yields `checkout_root`. `at` returns a copy.
#[derive(Default, Clone)]
pub struct FakeVcs {
    pub head: Option<String>,
    pub base: Option<String>,
    pub commits: HashSet<String>,
    /// (rev, path) → contents.
    pub shows: HashMap<(String, String), String>,
    pub changed: Vec<String>,
    pub clean: bool,
    pub history: Vec<Vec<String>>,
    pub checkout_root: Option<PathBuf>,
}

impl Vcs for FakeVcs {
    fn at(&self, _root: &Path) -> Box<dyn Vcs> {
        Box::new(self.clone())
    }
    fn head(&self) -> Option<String> {
        self.head.clone()
    }
    fn resolve_base(
        &self,
        explicit: Option<&str>,
        _notices: &mut Vec<String>,
    ) -> Result<Option<String>, PortError> {
        match explicit {
            Some(b) => match self.verify(b) {
                Some(s) => Ok(Some(s)),
                None => Err(PortError::Vcs(format!("--base {b}: not a commit"))),
            },
            None => Ok(self.base.clone()),
        }
    }
    fn verify(&self, rev: &str) -> Option<String> {
        self.commits.get(rev).cloned()
    }
    fn show(&self, rev: &str, path: &str) -> Option<String> {
        self.shows.get(&(rev.into(), path.into())).cloned()
    }
    fn changed_files(&self, _base: Option<&str>, _notices: &mut Vec<String>) -> Vec<String> {
        self.changed.clone()
    }
    fn is_clean(&self) -> bool {
        self.clean
    }
    fn history(&self, n: usize) -> Vec<Vec<String>> {
        self.history.iter().take(n).cloned().collect()
    }
    fn checkout(&self, rev: &str) -> Result<Checkout, PortError> {
        match self.verify(rev) {
            Some(_) => Ok(Checkout::new(self.checkout_root.clone(), Box::new(()))),
            None => Err(PortError::Vcs(format!("{rev}: not a commit"))),
        }
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
        self.outputs.get(&checker.name).cloned().unwrap_or_default()
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
    assert!(!f.exists("dir/sub"), "a directory is not a file");
    f.write_atomic("dir/sub/a.txt", b"two").unwrap();
    assert_eq!(
        f.read("dir/sub/a.txt").as_deref(),
        Some("two"),
        "overwrites"
    );
    f.write_atomic("bin", &[0xff, 0]).unwrap();
    assert_eq!(f.read_bytes("bin"), Some(vec![0xff, 0]), "bytes kept");
    assert_eq!(f.read("bin").as_deref(), Some("\u{fffd}\0"), "lossy text");
    let sub = f.at(Path::new("dir"));
    assert_eq!(
        sub.read("sub/a.txt").as_deref(),
        Some("two"),
        "`at` re-roots"
    );

    for p in ["b.ts", "a.ts", "src/c.ts", "src/a.ts", "x.md"] {
        f.write_atomic(p, b"").unwrap();
    }
    for p in [
        "node_modules/m/i.ts",
        ".git/h.ts",
        "src/.ig/s.ts",
        "src/b.d.ts",
    ] {
        f.write_atomic(p, b"").unwrap();
    }
    let glob = |p: &[&str]| f.glob(&p.iter().map(|s| s.to_string()).collect::<Vec<_>>());
    assert_eq!(
        glob(&["*.ts"]).unwrap(),
        ["a.ts", "b.ts"],
        "`*` stops at `/`"
    );
    assert_eq!(
        glob(&["src/*.ts", "*.ts"]).unwrap(),
        ["a.ts", "b.ts", "src/a.ts", "src/b.d.ts", "src/c.ts"],
        "sorted across patterns"
    );
    assert_eq!(
        glob(&["**/*.ts"]).unwrap(),
        ["a.ts", "b.ts", "src/a.ts", "src/b.d.ts", "src/c.ts"],
        "skips node_modules, .git and .ig"
    );
    assert!(glob(&["a{"]).is_err(), "bad pattern");

    let ws = f.discover(&Config::default()).unwrap();
    assert!(!ws.packages.is_empty());
    let found: Vec<&str> = ws.files.iter().map(|s| s.path.as_str()).collect();
    assert_eq!(
        found,
        ["a.ts", "b.ts", "src/a.ts", "src/c.ts"],
        "source files"
    );

    f.remove("b.ts").unwrap();
    assert!(!f.exists("b.ts"));
    assert_eq!(f.read("b.ts"), None);
    f.remove("b.ts").expect("removing a missing file is fine");
}

/// `v` has at least one commit, and `file` holds `content` at HEAD.
pub fn vcs_contract(v: &dyn Vcs, file: &str, content: &str) {
    let head = v.head().expect("a head commit");
    assert_eq!(v.verify(&head).as_ref(), Some(&head));
    assert_eq!(v.verify("0123456789abcdef0123456789abcdef01234567"), None);
    assert_eq!(v.show(&head, file).as_deref(), Some(content));
    assert_eq!(v.show(&head, "no/such/file.txt"), None, "missing path");
    let mut notices = vec![];
    assert_eq!(
        v.resolve_base(Some(&head), &mut notices),
        Ok(Some(head.clone()))
    );
    assert!(v.resolve_base(Some("no-such-rev"), &mut notices).is_err());
    assert!(v.checkout("no-such-rev").is_err());
    let c = v.checkout(&head).expect("checkout of HEAD");
    assert_eq!(
        v.at(c.root().unwrap()).head(),
        Some(head),
        "same repository"
    );
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
    assert!(fe.handles(path, &Config::default()));
    let file = SourceFile {
        path: path.into(),
        package: 0,
    };
    let ws = WorkspaceInfo {
        root: ".".into(),
        packages: vec![Package {
            name: "root".into(),
            dir: ".".into(),
            tsconfig: None,
            source_entry: None,
        }],
        files: vec![file.clone()],
    };
    let out = fe.build(&[(file, source.into())], &ws, &Config::default());
    let [facts] = &out.facts[..] else {
        panic!("one FileFacts per file, got {}", out.facts.len())
    };
    assert_eq!(facts.file, path);
    assert_eq!(facts.parse_error, None);
    assert!(
        facts.nodes.iter().all(|n| n.file == path),
        "nodes of this file"
    );
    assert!(out.edges.iter().all(|e| e.file == path));
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
    fn mem_files_discover_honours_ignore_and_embedded() {
        let f = MemFiles::with(&[
            ("package.json", r#"{"name": "app"}"#),
            ("src/a.ts", ""),
            ("src/App.vue", ""),
            ("gen/x.ts", ""),
            ("dist/d.js", ""),
        ]);
        let config = Config {
            ignore: vec!["gen".into()],
            embedded: vec![".vue".into()],
            ..Config::default()
        };
        let ws = f.discover(&config).unwrap();
        assert_eq!(ws.packages[0].name, "app");
        let found: Vec<&str> = ws.files.iter().map(|s| s.path.as_str()).collect();
        assert_eq!(found, ["src/App.vue", "src/a.ts"]);
    }

    #[test]
    fn fake_vcs_meets_the_contract() {
        let v = FakeVcs {
            head: Some("abc".into()),
            commits: ["abc".to_string()].into(),
            shows: [(("abc".into(), "intents.md".into()), "# I\n".into())].into(),
            checkout_root: Some("wt".into()),
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
        fn handles(&self, path: &str, _: &Config) -> bool {
            path.ends_with(".x")
        }
        fn build(
            &self,
            files: &[(SourceFile, String)],
            _: &WorkspaceInfo,
            _: &Config,
        ) -> FrontendOutput {
            let facts = files
                .iter()
                .map(|(f, _)| FileFacts {
                    file: f.path.clone(),
                    nodes: vec![Node {
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
                    }],
                    ..FileFacts::default()
                })
                .collect();
            FrontendOutput {
                facts,
                ..FrontendOutput::default()
            }
        }
        fn prepare_checkout(
            &self,
            _: &WorkspaceInfo,
            _: &Path,
            _: &Config,
            _: bool,
        ) -> Result<(), PortError> {
            Ok(())
        }
    }

    #[test]
    fn frontend_contract_runs() {
        frontend_contract(&OneNode, "a.x", "");
    }
}
