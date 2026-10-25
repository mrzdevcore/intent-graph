//! A fake frontend over a tiny line language, and an `Env` of fakes, for the use-case tests.
//!
//! Each file is a module node. `fn NAME` adds `file#NAME` (a function spanning that line),
//! `use PATH` an `imports` edge to module PATH, `call ID` a `calls` edge from the last function.

use crate::Env;
use ig_core::config::Config;
use ig_core::model::{Edge, Metrics, Node, Range, sha1_hex};
use ig_core::ports::{
    CheckerRunner, FileFacts, Files, FrontendOutput, LanguageFrontend, PortError, Vcs,
};
use ig_core::workspace::{SourceFile, WorkspaceInfo};
use std::path::Path;

#[derive(Default)]
pub struct Fake {
    inputs: Vec<String>,
}

impl Fake {
    pub fn with_inputs(inputs: &[&str]) -> Fake {
        Fake {
            inputs: inputs.iter().map(|s| s.to_string()).collect(),
        }
    }
}

fn node(id: &str, kind: &str, file: &str, lines: (u32, u32), hash: &str) -> Node {
    Node {
        id: id.into(),
        kind: kind.into(),
        name: id.rsplit('#').next().unwrap_or(id).into(),
        file: file.into(),
        range: Range {
            start_line: lines.0,
            end_line: lines.1,
        },
        parent: (kind != "module").then(|| file.to_string()),
        package: None,
        exported: true,
        hash: sha1_hex(hash),
        metrics: Metrics {
            loc: lines.1 - lines.0 + 1,
            complexity: 1,
        },
        intents: vec![],
    }
}

fn edge(from: &str, kind: &str, to: &str, file: &str, line: u32) -> Edge {
    Edge {
        from: from.into(),
        to: to.into(),
        kind: kind.into(),
        confidence: "resolved".into(),
        file: file.into(),
        line,
        namespace: false,
        reexport: false,
    }
}

impl LanguageFrontend for Fake {
    fn handles(&self, path: &str, _: &Config) -> bool {
        path.ends_with(".ts")
    }
    fn build(
        &self,
        files: &[(SourceFile, String)],
        _: &WorkspaceInfo,
        _: &Config,
    ) -> FrontendOutput {
        let mut out = FrontendOutput::default();
        for (f, src) in files {
            let p = f.path.as_str();
            let n = src.lines().count().max(1) as u32;
            let mut nodes = vec![node(p, "module", p, (1, n), src)];
            let mut from = p.to_string();
            for (i, l) in src.lines().enumerate() {
                let line = i as u32 + 1;
                if let Some(name) = l.strip_prefix("fn ") {
                    from = format!("{p}#{name}");
                    nodes.push(node(&from, "function", p, (line, line), l));
                } else if let Some(to) = l.strip_prefix("use ") {
                    out.edges.push(edge(p, "imports", to, p, line));
                } else if let Some(to) = l.strip_prefix("call ") {
                    out.edges.push(edge(&from, "calls", to, p, line));
                }
            }
            out.facts.push(FileFacts {
                file: p.into(),
                nodes,
                ..FileFacts::default()
            });
        }
        out
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
    fn cache_inputs(&self, _: &Path, _: &Config) -> Vec<String> {
        self.inputs.clone()
    }
}

fn quiet(_: &str) {}

/// An `Env` rooted at "." over fakes; `fe` is leaked (tests only).
pub fn env<'a>(
    files: &'a dyn Files,
    vcs: Option<&'a dyn Vcs>,
    runner: &'a dyn CheckerRunner,
    fe: &'a Fake,
) -> Env<'a> {
    let fes: &'a [&'a dyn LanguageFrontend] = Box::leak(Box::new([fe as &dyn LanguageFrontend]));
    Env {
        root: Path::new("."),
        files,
        vcs,
        runner,
        frontends: fes,
        progress: &quiet,
        notice: &quiet,
    }
}
