//! `ig_core::rules` plus the checker orchestration over `ig_checkers` and the baseline file.

pub use ig_core::rules::*;

use crate::config::{Checker, Config};
use crate::model::{Finding, Graph, Node};
use ig_checkers::ProcessRunner;
use ig_core::ports::CheckerRunner;
use ig_lang_ts::extract::innermost_at;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::time::Instant;

pub use ig_core::ports::CheckerOutput;

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
pub(crate) fn attach_nodes(head: &Graph, findings: Vec<Finding>) -> Vec<Finding> {
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
pub fn checkers(
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
            crate::check::progress(&format!("running {}…", c.name));
            let start = Instant::now();
            let o = ProcessRunner.run(root, c, files.as_deref());
            let secs = start.elapsed().as_secs_f32();
            crate::check::progress(&format!("{} done ({secs:.1} s)", c.name));
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

/// Baseline entries: fingerprint strings, or `{"fingerprint", "reason"}` exceptions. Empty when
/// the file is absent; an error when it exists but doesn't parse.
pub(crate) fn read_baseline(root: &Path) -> anyhow::Result<Vec<serde_json::Value>> {
    let path = root.join(".ig/baseline.json");
    match std::fs::read_to_string(&path) {
        Ok(t) => baseline::parse(&t).map_err(|e| anyhow::anyhow!("{}: {e}", path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(vec![]),
        Err(e) => Err(anyhow::anyhow!("{}: {e}", path.display())),
    }
}

fn baseline_items(root: &Path) -> Vec<serde_json::Value> {
    read_baseline(root).unwrap_or_default()
}

pub fn load_baseline(root: &Path) -> HashSet<String> {
    baseline::fingerprints(&baseline_items(root))
}

/// Writes `fingerprints` as strings, keeping existing exception objects.
pub fn write_baseline(
    root: &Path,
    fingerprints: impl IntoIterator<Item = String>,
) -> anyhow::Result<()> {
    let items = baseline::with_fingerprints(fingerprints, baseline_items(root));
    std::fs::create_dir_all(root.join(".ig"))?;
    std::fs::write(root.join(".ig/baseline.json"), baseline::render(&items)?)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

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

        // Nothing to check: skipped, not an error.
        let dir = std::env::temp_dir().join(format!("ig-files-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let cfg = Config::parse(
            "checkers:\n  - name: t\n    format: lines\n    run: \"exit 1 {files}\"\n",
        )
        .unwrap();
        let out = checkers(&dir, &cfg, &Graph::empty(), Some(&[]));
        std::fs::remove_dir_all(&dir).ok();
        assert!(
            out.errors.is_empty() && out.findings.is_empty(),
            "{:?}",
            out.errors
        );
    }
}
