//! Running the configured checkers (in parallel), and the findings the last check saved.

use crate::Env;
use ig_core::config::{Checker, Config};
use ig_core::model::{Finding, Graph, Node, innermost_at};
use ig_core::ports::CheckerOutput;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::time::Instant;

/// `{files}` keeps these when a checker sets no `extensions` (plus files in the graph).
const SCRIPT_STYLE: &[&str] = &[
    ".ts", ".tsx", ".js", ".jsx", ".mjs", ".cjs", ".vue", ".css", ".scss", ".less",
];

/// The changed files checker `c` cares about.
fn filter_files(c: &Checker, changed: &[String], known: &HashSet<&str>) -> Vec<String> {
    changed
        .iter()
        .filter(|f| match c.extensions.is_empty() {
            true => known.contains(f.as_str()) || SCRIPT_STYLE.iter().any(|e| f.ends_with(e)),
            false => c.extensions.iter().any(|e| f.ends_with(e.as_str())),
        })
        .cloned()
        .collect()
}

/// Attaches the innermost graph node to each finding (module-level files only).
pub fn attach_nodes(head: &Graph, findings: Vec<Finding>) -> Vec<Finding> {
    let mut by_file: HashMap<&str, Vec<Node>> = HashMap::new();
    for n in &head.nodes {
        by_file.entry(n.file.as_str()).or_default().push(n.clone());
    }
    for v in by_file.values_mut() {
        v.sort_by_key(|n| !n.is_module());
    }
    findings
        .into_iter()
        .map(|f| match by_file.get(f.file.as_str()) {
            Some(nodes) if nodes.first().is_some_and(Node::is_module) => {
                let id = innermost_at(nodes, f.line).to_string();
                f.node(&id)
            }
            _ => f,
        })
        .collect()
}

/// Runs all checkers (in parallel) against the tree at `root`. `changed` fills `{files}`.
pub fn run(
    env: &Env,
    root: &Path,
    config: &Config,
    head: &Graph,
    changed: Option<&[String]>,
) -> CheckerOutput {
    use rayon::prelude::*;
    let known: HashSet<&str> = head.nodes.iter().map(|n| n.file.as_str()).collect();
    let runs: Vec<(&Checker, Option<Vec<String>>)> = config
        .checkers
        .iter()
        .map(|c| (c, changed.map(|ch| filter_files(c, ch, &known))))
        // Nothing to check: skipped, without a progress line.
        .filter(|(c, files)| {
            !(c.run.contains("{files}") && files.as_ref().is_some_and(Vec::is_empty))
        })
        .collect();
    let outs: Vec<CheckerOutput> = runs
        .par_iter()
        .map(|(c, files)| {
            (env.progress)(&format!("running {}…", c.name));
            let start = Instant::now();
            let o = env.runner.run(root, c, files.as_deref());
            let secs = start.elapsed().as_secs_f32();
            (env.progress)(&format!("{} done ({secs:.1} s)", c.name));
            o
        })
        .collect();
    let mut out = CheckerOutput::default();
    for o in outs {
        out.notices.extend(o.notices);
        out.findings.extend(attach_nodes(head, o.findings));
        out.errors.extend(o.errors);
    }
    out
}

/// The checker findings (and checker errors) saved by the last check, for configured checkers.
pub fn saved(env: &Env, config: &Config) -> CheckerOutput {
    let names: HashSet<&str> = config.checkers.iter().map(|c| c.name.as_str()).collect();
    let saved: Vec<serde_json::Value> =
        crate::read_json::<serde_json::Value>(env.files, ".ig/findings.json")
            .and_then(|mut v| serde_json::from_value(v["findings"].take()).ok())
            .unwrap_or_default();
    let (errors, findings) = saved
        .into_iter()
        .filter_map(|v| serde_json::from_value::<Finding>(v).ok())
        .filter(|f| names.contains(f.tool.as_str()))
        .partition(|f| f.rule_id == "ig/checker-error");
    CheckerOutput {
        findings,
        errors,
        notices: vec![],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{Fake, env};
    use ig_core::testing::{FakeRunner, MemFiles};

    #[test]
    fn files_filter() {
        let c = |exts: &str| {
            Config::parse(&format!(
                "checkers:\n  - name: t\n    run: \"lint {{files}}\"\n{exts}"
            ))
            .unwrap()
            .checkers
            .remove(0)
        };
        let changed: Vec<String> = ["src/it's a.ts", "b.vue", "c.md", "d.rs", "-x.ts"]
            .map(String::from)
            .into();
        let known: HashSet<&str> = ["d.rs"].into();
        assert_eq!(
            filter_files(&c(""), &changed, &known),
            ["src/it's a.ts", "b.vue", "d.rs", "-x.ts"]
        );
        assert_eq!(
            filter_files(&c("    extensions: ['.vue']\n"), &changed, &known),
            ["b.vue"]
        );
        assert!(filter_files(&c("    extensions: ['.css']\n"), &changed, &known).is_empty());

        // Nothing to check: skipped, not run.
        let cfg = Config::parse(
            "checkers:\n  - name: t\n    format: lines\n    run: \"exit 1 {files}\"\n",
        )
        .unwrap();
        let (files, runner, fe) = (MemFiles::default(), FakeRunner::default(), Fake::default());
        let e = env(&files, None, &runner, &fe);
        let out = run(&e, Path::new("."), &cfg, &Graph::empty(), Some(&[]));
        assert!(out.errors.is_empty() && out.findings.is_empty());
        assert!(runner.runs.lock().unwrap().is_empty());
    }
}
