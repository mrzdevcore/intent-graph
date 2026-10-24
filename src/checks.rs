//! Rules: arch, complexity, intents, SARIF checkers, baseline.

use crate::config::{Config, Severity};
use crate::diff::{GraphDiff, Status};
use crate::extract::innermost_at;
use crate::intents::Intent;
use crate::model::{Finding, Graph, Node};
use crate::workspace::{self, Package};
use globset::GlobMatcher;
use std::collections::{BTreeSet, HashMap, HashSet};
use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// Layer assignment: the most specific entry wins (longest literal prefix), ties go to the first declared.
pub struct Layers<'c> {
    entries: Vec<(&'c str, GlobMatcher, usize)>,
}

fn literal_prefix(g: &str) -> usize {
    g.find(['*', '?', '[', '{']).unwrap_or(g.len())
}

impl<'c> Layers<'c> {
    pub fn new(config: &'c Config, packages: &[Package]) -> Layers<'c> {
        let mut entries = vec![];
        for (name, globs) in &config.layers {
            for g in globs {
                let pattern = match packages.iter().find(|p| &p.name == g) {
                    Some(p) if p.dir == "." => "**".to_string(),
                    Some(p) => format!("{}/**", p.dir),
                    None => g.trim_start_matches("./").to_string(),
                };
                if let Ok(m) = workspace::glob(&pattern) {
                    entries.push((name.as_str(), m.compile_matcher(), literal_prefix(&pattern)));
                }
            }
        }
        Layers { entries }
    }

    pub fn of(&self, file: &str) -> Option<&'c str> {
        let mut best: Option<(&str, usize)> = None;
        for (name, m, spec) in &self.entries {
            if m.is_match(file) && best.is_none_or(|(_, s)| *spec > s) {
                best = Some((name, *spec));
            }
        }
        best.map(|(n, _)| n)
    }
}

pub fn arch(graph: &Graph, config: &Config, packages: &[Package]) -> Vec<Finding> {
    if config.rules.is_empty() {
        return vec![];
    }
    let layers = Layers::new(config, packages);
    let files: HashMap<&str, &str> = graph
        .nodes
        .iter()
        .map(|n| (n.id.as_str(), n.file.as_str()))
        .collect();
    let layer_of: HashMap<&str, Option<&str>> =
        files.values().map(|f| (*f, layers.of(f))).collect();
    let layer = |f: &str| layer_of.get(f).copied().flatten();
    let mut out = vec![];
    for e in graph
        .edges
        .iter()
        .filter(|e| e.resolved() && matches!(e.kind.as_str(), "imports" | "calls" | "renders"))
    {
        let (Some(ff), Some(tf)) = (files.get(e.from.as_str()), files.get(e.to.as_str())) else {
            continue;
        };
        let (Some(a), Some(b)) = (layer(ff), layer(tf)) else {
            continue;
        };
        if a == b || !config.rules.iter().any(|(x, y)| x == a && y == b) {
            continue;
        }
        out.push(
            Finding::new(
                "ig/arch",
                Severity::Error,
                "ig",
                &e.file,
                e.line,
                format!(
                    "layer {a} must not depend on {b}: {} {} {}",
                    e.from, e.kind, e.to
                ),
            )
            .node(&e.from)
            .edge(&e.from, &e.to)
            .fix(format!(
                "Remove this dependency, or move the code so {a} no longer {} {b}.",
                e.kind
            )),
        );
    }
    out
}

pub fn complexity(
    head: &Graph,
    base: Option<&Graph>,
    diff: &GraphDiff,
    config: &Config,
) -> Vec<Finding> {
    let before: HashMap<&str, &Node> = base
        .map(|b| b.nodes.iter().map(|n| (n.id.as_str(), n)).collect())
        .unwrap_or_default();
    let (max, inc) = (config.complexity_max, config.complexity_max_increase);
    let mut out = vec![];
    for n in head.nodes.iter().filter(|n| !n.is_module()) {
        let c = n.metrics.complexity;
        let msg = match (diff.status(&n.id), before.get(n.id.as_str())) {
            (Status::Added, _) if c > max => format!("{} has complexity {c} (max {max})", n.name),
            (Status::Changed, Some(b)) if c > max && c > b.metrics.complexity => {
                format!(
                    "{} has complexity {c} (max {max}, was {})",
                    n.name, b.metrics.complexity
                )
            }
            (Status::Changed, Some(b)) if c > b.metrics.complexity + inc => {
                format!(
                    "{} complexity rose from {} to {c} (max increase {inc})",
                    n.name, b.metrics.complexity
                )
            }
            (Status::Unchanged, _) if c > max => {
                format!("{} has complexity {c} (max {max})", n.name)
            }
            _ => continue,
        };
        out.push(
            Finding::new(
                "ig/complexity",
                Severity::Error,
                "ig",
                &n.file,
                n.range.start_line,
                msg,
            )
            .node(&n.id)
            .fix("Split it into smaller functions or flatten the branching."),
        );
    }
    out
}

/// `scope`: the intents in scope (changed in intents.md, or all with `--all`).
pub fn intents(
    head: &Graph,
    declared: &[Intent],
    scope: &BTreeSet<String>,
    tests: &globset::GlobSet,
) -> Vec<Finding> {
    let mut implemented: HashSet<&str> = HashSet::new();
    let mut tested: HashSet<&str> = HashSet::new();
    let ids: HashSet<&str> = declared.iter().map(|i| i.id.as_str()).collect();
    let mut out = vec![];
    for n in &head.nodes {
        let in_test = n.kind == "test" || tests.is_match(&n.file);
        for t in &n.intents {
            if in_test {
                tested.insert(t);
            } else {
                implemented.insert(t);
            }
            if !ids.contains(t.as_str()) {
                out.push(
                    Finding::new(
                        "ig/intent-unknown",
                        Severity::Error,
                        "ig",
                        &n.file,
                        n.range.start_line,
                        format!("@intent {t} is not declared in intents.md"),
                    )
                    .node(&n.id)
                    .fix(format!(
                        "Add `- {t}: <one line>` to intents.md, or fix the tag."
                    )),
                );
            }
        }
    }
    for i in declared.iter().filter(|i| scope.contains(&i.id)) {
        if !implemented.contains(i.id.as_str()) {
            out.push(
                Finding::new(
                    "ig/intent-unimplemented",
                    Severity::Error,
                    "ig",
                    "intents.md",
                    i.line,
                    format!("intent {} has no implementation: {}", i.id, i.text),
                )
                .fix(format!(
                    "Tag the implementing code with `// @intent {}`.",
                    i.id
                )),
            );
        }
        if !tested.contains(i.id.as_str()) {
            out.push(
                Finding::new(
                    "ig/intent-untested",
                    Severity::Warning,
                    "ig",
                    "intents.md",
                    i.line,
                    format!("intent {} has no test: {}", i.id, i.text),
                )
                .fix(format!("Tag a test with `// @intent {}`.", i.id)),
            );
        }
    }
    out
}

pub struct CheckerOutput {
    pub findings: Vec<Finding>,
    pub errors: Vec<Finding>,
}

const TIMEOUT: Duration = Duration::from_secs(120);
const CAP: usize = 64 << 20;

fn read_capped(mut r: impl Read + Send + 'static) -> std::thread::JoinHandle<(Vec<u8>, bool)> {
    std::thread::spawn(move || {
        let mut buf = vec![];
        let mut chunk = [0u8; 65536];
        let mut over = false;
        loop {
            match r.read(&mut chunk) {
                Ok(0) | Err(_) => break,
                Ok(n) if buf.len() + n > CAP => {
                    over = true;
                    buf.extend_from_slice(&chunk[..CAP - buf.len()]);
                }
                Ok(n) if !over => buf.extend_from_slice(&chunk[..n]),
                Ok(_) => {}
            }
        }
        (buf, over)
    })
}

/// Runs one checker as `sh -c <run>` in `root`.
fn run_checker(root: &Path, run: &str) -> Result<Vec<u8>, String> {
    let mut child = Command::new("sh")
        .arg("-c")
        .arg(run)
        .current_dir(root)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| e.to_string())?;
    let out = read_capped(child.stdout.take().unwrap());
    let err = read_capped(child.stderr.take().unwrap());
    let start = Instant::now();
    let timed_out = loop {
        match child.try_wait() {
            Ok(Some(_)) => break false,
            Ok(None) if start.elapsed() > TIMEOUT => {
                let _ = child.kill();
                let _ = child.wait();
                break true;
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(20)),
            Err(e) => return Err(e.to_string()),
        }
    };
    let (stdout, over) = out.join().unwrap_or_default();
    let (stderr, _) = err.join().unwrap_or_default();
    let stderr: String = String::from_utf8_lossy(&stderr).chars().take(300).collect();
    if timed_out {
        return Err(format!("timed out after 120 s; stderr: {stderr}"));
    }
    if over {
        return Err(format!("output exceeded 64 MB; stderr: {stderr}"));
    }
    if serde_json::from_slice::<serde_json::Value>(&stdout).is_err() {
        return Err(format!("did not print SARIF JSON; stderr: {stderr}"));
    }
    Ok(stdout)
}

fn repo_path(root: &Path, uri: &str) -> String {
    let mut p = uri.strip_prefix("file://").unwrap_or(uri).to_string();
    if let Ok(d) = urlish_decode(&p) {
        p = d;
    }
    let path = Path::new(&p);
    if path.is_absolute() {
        let canon = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        if let Some(r) = workspace::rel(root, &canon).or_else(|| workspace::rel(root, path)) {
            return r;
        }
    }
    workspace::normalize(p.trim_start_matches("./"))
}

fn urlish_decode(s: &str) -> Result<String, ()> {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            let h = std::str::from_utf8(&b[i + 1..i + 3]).map_err(|_| ())?;
            out.push(u8::from_str_radix(h, 16).map_err(|_| ())?);
            i += 3;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8(out).map_err(|_| ())
}

pub fn parse_sarif(
    root: &Path,
    tool: &str,
    bytes: &[u8],
    head: &Graph,
) -> Result<Vec<Finding>, String> {
    let v: serde_json::Value = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
    let runs = v
        .get("runs")
        .and_then(|r| r.as_array())
        .ok_or("no runs[]")?;
    let mut by_file: HashMap<&str, Vec<Node>> = HashMap::new();
    for n in &head.nodes {
        by_file.entry(n.file.as_str()).or_default().push(n.clone());
    }
    for v in by_file.values_mut() {
        v.sort_by_key(|n| !n.is_module());
    }
    let mut out = vec![];
    for run in runs {
        for r in run
            .get("results")
            .and_then(|r| r.as_array())
            .into_iter()
            .flatten()
        {
            let loc = r.pointer("/locations/0/physicalLocation");
            let uri = loc
                .and_then(|l| l.pointer("/artifactLocation/uri"))
                .and_then(|u| u.as_str())
                .unwrap_or("");
            let file = repo_path(root, uri);
            let line = loc
                .and_then(|l| l.pointer("/region/startLine"))
                .and_then(|x| x.as_u64())
                .unwrap_or(1) as u32;
            let sev = match r.get("level").and_then(|l| l.as_str()) {
                Some("error") => Severity::Error,
                Some("note") | Some("none") => Severity::Note,
                _ => Severity::Warning,
            };
            let rule = r.get("ruleId").and_then(|x| x.as_str()).unwrap_or(tool);
            let msg = r
                .pointer("/message/text")
                .and_then(|x| x.as_str())
                .unwrap_or("")
                .to_string();
            let mut f = Finding::new(rule, sev, tool, &file, line, msg);
            if let Some(fix) = r
                .pointer("/fixes/0/description/text")
                .and_then(|x| x.as_str())
            {
                f = f.fix(fix);
            }
            if let Some(nodes) = by_file.get(file.as_str())
                && !nodes.is_empty()
                && nodes[0].is_module()
            {
                f = f.node(innermost_at(nodes, line));
            }
            out.push(f);
        }
    }
    Ok(out)
}

/// Runs all checkers (in parallel) against the tree at `root`.
pub fn checkers(root: &Path, config: &Config, head: &Graph) -> CheckerOutput {
    use rayon::prelude::*;
    let results: Vec<(String, Result<Vec<Finding>, String>)> = config
        .checkers
        .par_iter()
        .map(|c| {
            (
                c.name.clone(),
                run_checker(root, &c.run).and_then(|b| parse_sarif(root, &c.name, &b, head)),
            )
        })
        .collect();
    let mut out = CheckerOutput {
        findings: vec![],
        errors: vec![],
    };
    for (name, r) in results {
        match r {
            Ok(f) => out.findings.extend(f),
            Err(e) => out.errors.push(Finding::new(
                "ig/checker-error",
                Severity::Warning,
                &name,
                "ig.yaml",
                1,
                format!("checker {name}: {e}"),
            )),
        }
    }
    out
}

pub fn load_baseline(root: &Path) -> HashSet<String> {
    std::fs::read_to_string(root.join(".ig/baseline.json"))
        .ok()
        .and_then(|t| serde_json::from_str::<Vec<String>>(&t).ok())
        .unwrap_or_default()
        .into_iter()
        .collect()
}

pub fn write_baseline(
    root: &Path,
    fingerprints: impl IntoIterator<Item = String>,
) -> anyhow::Result<()> {
    let set: BTreeSet<String> = fingerprints.into_iter().collect();
    std::fs::create_dir_all(root.join(".ig"))?;
    std::fs::write(
        root.join(".ig/baseline.json"),
        serde_json::to_string_pretty(&set)? + "\n",
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layers_most_specific_wins() {
        let c = Config::parse(
            "layers:\n  a: ['packages/**']\n  b: ['packages/core/**']\n  c: ['packages/core/**']\n",
        )
        .unwrap();
        let l = Layers::new(&c, &[]);
        assert_eq!(l.of("packages/core/x.ts"), Some("b"));
        assert_eq!(l.of("packages/ui/x.ts"), Some("a"));
        assert_eq!(l.of("apps/x.ts"), None);
    }

    #[test]
    fn sarif_paths() {
        let root = Path::new("/repo");
        assert_eq!(repo_path(root, "file:///repo/src/a%20b.ts"), "src/a b.ts");
        assert_eq!(repo_path(root, "./src/a.ts"), "src/a.ts");
    }
}
