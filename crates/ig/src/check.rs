//! `ig check` / `ig diff`: orchestration, scoping, post-processing and output.

use crate::checks;
use crate::config::{Config, Severity};
use crate::diff::{GraphDiff, Status};
use crate::git::{self, Repo, Worktree};
use crate::graph;
use crate::intents::{self, IntentChange};
use crate::model::{Finding, Graph, TOOL_VERSION};
use crate::review;
use crate::workspace::{self, Package};
use anyhow::{Result, bail};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};

#[derive(Default, Clone)]
pub struct Opts {
    pub base: Option<String>,
    pub head: Option<String>,
    pub all: bool,
    /// Viewer builds: reuse the checker findings of the last `ig check` instead of running them.
    pub skip_checkers: bool,
}

/// A progress line on stderr, only when it is a terminal (hooks and JSON consumers stay clean).
pub fn progress(msg: &str) {
    use std::io::IsTerminal;
    if std::io::stderr().is_terminal() {
        eprintln!("ig: {msg}");
    }
}

/// Files the change touches: those with added or changed nodes, plus files git sees as changed
/// against the base or untracked. Repo-relative to the ig root, existing ones only.
fn changed_files(t: &Trees, diff: &GraphDiff, notices: &mut Vec<String>) -> Vec<String> {
    let mut set: BTreeSet<String> = diff.touched_files.iter().cloned().collect();
    if t.repo.is_some() {
        let mut runs = vec![vec!["ls-files", "-z", "--others", "--exclude-standard"]];
        if let Some(b) = &t.base {
            runs.push(vec!["diff", "-z", "--name-only", "--relative", b]);
        }
        for args in runs {
            match git::git(&t.head_root, &args) {
                Ok(out) => set.extend(
                    out.split('\0')
                        .filter(|f| !f.is_empty() && t.head_root.join(f).is_file())
                        .map(String::from),
                ),
                Err(e) => notices.push(format!("changed files for checkers: {e}")),
            }
        }
    }
    set.into_iter().collect()
}

/// The checker findings (and checker errors) saved by the last check, for configured checkers.
fn saved_checkers(root: &Path, config: &Config) -> checks::CheckerOutput {
    let names: HashSet<&str> = config.checkers.iter().map(|c| c.name.as_str()).collect();
    let saved: Vec<serde_json::Value> = std::fs::read_to_string(root.join(".ig/findings.json"))
        .ok()
        .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
        .and_then(|mut v| serde_json::from_value(v["findings"].take()).ok())
        .unwrap_or_default();
    let (errors, findings) = saved
        .into_iter()
        .filter_map(|v| serde_json::from_value::<Finding>(v).ok())
        .filter(|f| names.contains(f.tool.as_str()))
        .partition(|f| f.rule_id == "ig/checker-error");
    checks::CheckerOutput {
        findings,
        errors,
        notices: vec![],
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Counts {
    pub added: usize,
    pub changed: usize,
    pub removed: usize,
    pub unchanged: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckResult {
    pub schema_version: u32,
    pub tool_version: String,
    pub base: Option<String>,
    pub head: Option<String>,
    pub blocking: bool,
    pub notices: Vec<String>,
    pub intent_changes: Vec<IntentChange>,
    pub counts: Counts,
    /// Node id → status, for nodes that are not unchanged.
    pub changes: BTreeMap<String, Status>,
    /// Edge key (`from|kind|to`) → status, for edges that are not unchanged.
    pub edge_changes: BTreeMap<String, Status>,
    pub findings: Vec<Finding>,
    pub graph: Graph,
}

/// Everything a check computed; `base_graph` is kept for the viewer and reports.
pub struct Run {
    pub result: CheckResult,
    pub base_graph: Option<Graph>,
    pub diff: GraphDiff,
    pub config: Config,
    pub packages: Vec<Package>,
    pub repo: Option<Repo>,
    pub intents: Vec<intents::Intent>,
    /// Ids and edge keys added from a plan; empty without one.
    pub planned: HashSet<String>,
    /// The overlaid plan for the viewer: {title, steps, done, volume, findings}.
    pub plan: Option<serde_json::Value>,
}

pub struct Trees {
    pub head_root: PathBuf,
    pub head_commit: Option<String>,
    pub base: Option<String>,
    pub repo: Option<Repo>,
    pub config: Config,
    pub notices: Vec<String>,
    pub _wt: Option<Worktree>,
}

/// Resolves base and head; materialises `--head` when needed.
pub fn trees(root: &Path, opts: &Opts) -> Result<Trees> {
    let work_cfg = crate::config::load(root)?;
    let mut notices = vec![];
    let repo = Repo::open(root);
    let Some(repo) = repo else {
        if opts.head.is_some() || opts.base.is_some() {
            bail!("--base/--head need a git repository");
        }
        notices.push("not a git repository: every node counts as added".into());
        return Ok(Trees {
            head_root: root.into(),
            head_commit: None,
            base: None,
            repo: None,
            config: work_cfg,
            notices,
            _wt: None,
        });
    };
    let base = repo.resolve_base(opts.base.as_deref(), &mut notices)?;
    let head_sha = repo.head();
    let (head_root, head_commit, wt, config) = match &opts.head {
        None => (root.to_path_buf(), head_sha, None, work_cfg),
        Some(h) => {
            let Some(sha) = git::verify(&repo.top, h) else {
                bail!("--head {h}: not a commit")
            };
            if Some(&sha) == head_sha.as_ref() && repo.is_clean() {
                (root.to_path_buf(), Some(sha), None, work_cfg)
            } else {
                let wt = Worktree::add(&repo, &sha, &git::node_modules_sources(root, &work_cfg))?;
                let r = wt.root(&repo);
                // Trust boundary: checkers always come from the working tree's ig.yaml.
                let mut cfg = if r.is_dir() {
                    crate::config::load(&r)?
                } else {
                    Config::default()
                };
                cfg.checkers = work_cfg.checkers.clone();
                (r, Some(sha), Some(wt), cfg)
            }
        }
    };
    Ok(Trees {
        head_root,
        head_commit,
        base,
        repo: Some(repo),
        config,
        notices,
        _wt: wt,
    })
}

/// A scoped finding counts if its edge is new, else if its node is added or changed, else if its
/// file has an added or changed node.
pub fn in_scope(f: &Finding, diff: &GraphDiff) -> bool {
    if let Some(e) = &f.edge {
        return diff.new_pairs.contains(&(e.from.clone(), e.to.clone()));
    }
    if let Some(n) = &f.node_id {
        return diff.is_touched(n);
    }
    diff.touched_files.contains(&f.file)
}

fn snippet(lines: &[&str], line: u32) -> Option<String> {
    if line == 0 || line as usize > lines.len() {
        return None;
    }
    let lo = line.saturating_sub(2).max(1);
    let hi = (line + 2).min(lines.len() as u32);
    let w = hi.to_string().len();
    Some(
        (lo..=hi)
            .map(|i| {
                format!(
                    "{} {:>w$} | {}",
                    if i == line { ">" } else { " " },
                    i,
                    lines[i as usize - 1]
                )
            })
            .collect::<Vec<_>>()
            .join("\n"),
    )
}

pub fn run(root: &Path, opts: &Opts) -> Result<Run> {
    // Apply in progress: compare against its base. Stale state is ignored, never fatal.
    let apply = crate::apply::active(root, opts.base.as_deref());
    let stale = match (&apply, &opts.base) {
        (None, None) => crate::apply::load(root).and_then(|st| crate::apply::stale(root, &st)),
        _ => None,
    };
    let mut opts = opts.clone();
    if let Some(st) = &apply {
        opts.base = Some(st.base.clone());
    }
    let opts = &opts;
    let t = trees(root, opts)?;
    let config = t.config.clone();
    let mut notices = t.notices.clone();
    if let Some(r) = stale {
        notices.push(format!("apply state is stale ({r}); ignoring it"));
    }
    progress("building graph…");
    let built = graph::build(&t.head_root, &config, t.head_commit.clone())?;
    let head = built.graph;
    let packages = built.workspace.packages;
    let base_graph = match (&t.repo, &t.base) {
        (Some(repo), Some(sha)) => Some(git::base_graph(root, repo, sha, &config)?),
        _ => None,
    };
    let diff = GraphDiff::new(base_graph.as_ref(), &head);
    let tests = workspace::globset(&config.tests)?;

    // Intents.
    let head_intents = intents::parse(
        &std::fs::read_to_string(t.head_root.join("intents.md")).unwrap_or_default(),
    );
    let base_intents = match (&t.repo, &t.base) {
        (Some(repo), Some(sha)) => {
            intents::parse(&repo.show(sha, "intents.md").unwrap_or_default())
        }
        _ => vec![],
    };
    let intent_changes = intents::changes(&base_intents, &head_intents);
    let scope: BTreeSet<String> = if opts.all {
        head_intents.iter().map(|i| i.id.clone()).collect()
    } else {
        intent_changes
            .iter()
            .filter(|c| c.status != Status::Removed)
            .map(|c| c.id.clone())
            .collect()
    };

    // Scoped findings.
    let mut scoped = checks::arch(&head, &config, &packages);
    scoped.extend(checks::complexity(
        &head,
        base_graph.as_ref(),
        &diff,
        &config,
    ));
    // None with --all (or no checkers): `{files}` is `.` and nothing is scoped out.
    let changed =
        (!opts.all && !config.checkers.is_empty()).then(|| changed_files(&t, &diff, &mut notices));
    let ch = if opts.skip_checkers {
        saved_checkers(root, &config)
    } else {
        checks::checkers(&t.head_root, &config, &head, changed.as_deref())
    };
    notices.extend(ch.notices);
    let mut findings: Vec<Finding> = scoped
        .into_iter()
        .filter(|f| opts.all || in_scope(f, &diff))
        .collect();
    // Checker findings also count on changed files outside the graph (`.css` and the like).
    let graph_files: HashSet<&str> = head.nodes.iter().map(|n| n.file.as_str()).collect();
    let changed_set: HashSet<&str> = changed.iter().flatten().map(String::as_str).collect();
    let (shown, out_of_scope): (Vec<Finding>, Vec<Finding>) =
        ch.findings.into_iter().partition(|f| {
            opts.all
                || in_scope(f, &diff)
                || (!graph_files.contains(f.file.as_str()) && changed_set.contains(f.file.as_str()))
        });
    findings.extend(shown);
    // A view build writes back the saved checker findings it doesn't show.
    let keep_saved = if opts.skip_checkers {
        out_of_scope
    } else {
        vec![]
    };

    // Always-on findings.
    for e in &head.parse_errors {
        findings.push(Finding::new(
            "ig/parse-error",
            Severity::Warning,
            "ig",
            &e.file,
            1,
            e.message.clone(),
        ));
    }
    findings.extend(ch.errors);
    findings.extend(checks::intents(&head, &head_intents, &scope, &tests));
    let mut fingerprinted: Vec<Finding> =
        findings.into_iter().map(Finding::fingerprinted).collect();
    if let Some(b) = &base_graph {
        let eps = workspace::globset(&config.entry_points)?;
        let ctx = review::Ctx {
            config: &config,
            tests: &tests,
            packages: &packages,
            entry_points: &eps,
        };
        fingerprinted.extend(review::regressions(b, &head, &ctx));
    }

    // Post-processing.
    let baseline = checks::load_baseline(root);
    let mut files: HashMap<String, String> = HashMap::new();
    let mut post = |mut f: Finding| {
        if let Some(s) = config.severity_for(&f.rule_id) {
            f.severity = s;
        }
        let text = files.entry(f.file.clone()).or_insert_with(|| {
            std::fs::read_to_string(t.head_root.join(&f.file)).unwrap_or_default()
        });
        let lines: Vec<&str> = text.lines().collect();
        f.snippet = snippet(&lines, f.line);
        f
    };
    let out: Vec<Finding> = fingerprinted
        .into_iter()
        .filter(|f| !baseline.contains(&f.fingerprint))
        .map(&mut post)
        .collect();
    for n in &notices {
        eprintln!("ig: {n}");
    }
    notices.dedup();

    let result = CheckResult {
        schema_version: 1,
        tool_version: TOOL_VERSION.into(),
        base: t.base.clone(),
        head: t.head_commit.clone(),
        notices,
        intent_changes,
        counts: Counts {
            added: diff.count(Status::Added),
            changed: diff.count(Status::Changed),
            removed: diff.count(Status::Removed),
            unchanged: diff.count(Status::Unchanged),
        },
        changes: diff
            .nodes
            .iter()
            .filter(|(_, s)| **s != Status::Unchanged)
            .map(|(k, v)| (k.clone(), *v))
            .collect(),
        edge_changes: diff
            .edges
            .iter()
            .filter(|(_, s)| **s != Status::Unchanged)
            .map(|(k, v)| (k.clone(), *v))
            .collect(),
        findings: out,
        blocking: false,
        graph: head,
    };
    let mut run = Run {
        result,
        base_graph,
        diff,
        config: config.clone(),
        packages,
        repo: t.repo,
        intents: head_intents,
        planned: HashSet::new(),
        plan: None,
    };
    // Apply findings come after the baseline filter: they cannot be baselined.
    if let Some(st) = &apply {
        let extra: Vec<Finding> = crate::apply::findings(root, st, &run, false)
            .into_iter()
            .map(&mut post)
            .collect();
        run.result.findings.extend(extra);
    }
    run.result.findings.sort_by(|a, b| {
        (
            std::cmp::Reverse(a.severity),
            &a.file,
            a.line,
            &a.rule_id,
            &a.message,
        )
            .cmp(&(
                std::cmp::Reverse(b.severity),
                &b.file,
                b.line,
                &b.rule_id,
                &b.message,
            ))
    });
    let block_on = run.config.block_on;
    run.result.blocking = run.result.findings.iter().any(|f| f.severity >= block_on);
    let shown = run.result.findings.len();
    run.result.findings.extend(keep_saved);
    write_state(root, &run.result)?;
    run.result.findings.truncate(shown);
    Ok(run)
}

pub fn write_state(root: &Path, r: &CheckResult) -> Result<()> {
    let dir = root.join(".ig");
    std::fs::create_dir_all(&dir)?;
    write_atomic(&dir.join("graph.json"), &serde_json::to_vec(&r.graph)?)?;
    write_atomic(&dir.join("findings.json"), &serde_json::to_vec(r)?)?;
    Ok(())
}

pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    let tmp = path.with_extension(format!("tmp-{}", std::process::id()));
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

pub fn summary_text(r: &CheckResult) -> String {
    let mut s = String::new();
    if !r.intent_changes.is_empty() {
        s.push_str("intents:\n");
        for c in &r.intent_changes {
            s.push_str(&format!("  {} {}: {}\n", c.status.mark(), c.id, c.text));
        }
    }
    let c = &r.counts;
    s.push_str(&format!(
        "nodes: {} added, {} changed, {} removed, {} unchanged\n",
        c.added, c.changed, c.removed, c.unchanged
    ));
    s.push_str(&findings_text(&r.findings));
    s
}

/// Findings grouped by severity, as in `ig check`'s summary.
pub fn findings_text(findings: &[Finding]) -> String {
    let mut s = String::new();
    if findings.is_empty() {
        s.push_str("no findings\n");
    }
    for sev in [Severity::Error, Severity::Warning, Severity::Note] {
        let fs: Vec<&Finding> = findings.iter().filter(|f| f.severity == sev).collect();
        if fs.is_empty() {
            continue;
        }
        s.push_str(&format!("{}s ({}):\n", sev.as_str(), fs.len()));
        for f in fs {
            s.push_str(&format!(
                "  {}:{}  {}  {}\n",
                f.file, f.line, f.rule_id, f.message
            ));
        }
    }
    s
}

pub fn agent_text(r: &CheckResult, block_on: Severity) -> String {
    let blocking: Vec<&Finding> = r
        .findings
        .iter()
        .filter(|f| f.severity >= block_on)
        .collect();
    if blocking.is_empty() {
        let w = r
            .findings
            .iter()
            .filter(|f| f.severity == Severity::Warning)
            .count();
        return format!("ig check: no blocking findings ({w} warnings)\n");
    }
    let mut s = format!(
        "ig check: {} blocking finding{}\n\n",
        blocking.len(),
        if blocking.len() == 1 { "" } else { "s" }
    );
    for f in blocking {
        s.push_str(&format!(
            "{}:{} {} [{}] {}\n",
            f.file,
            f.line,
            f.severity.as_str(),
            f.rule_id,
            f.message
        ));
        if let Some(sn) = &f.snippet {
            s.push_str(sn);
            s.push('\n');
        }
        if let Some(fix) = &f.fix {
            s.push_str(&format!("fix: {fix}\n"));
        }
        s.push('\n');
    }
    s.push_str("Fix these, add @intent tags where missing, then run ig check again.\n");
    s
}

pub fn diff_text(r: &CheckResult, base: Option<&Graph>) -> String {
    let mut by_file: BTreeMap<&str, Vec<(Status, &str, &str)>> = BTreeMap::new();
    let lookup: HashMap<&str, &crate::model::Node> = r
        .graph
        .nodes
        .iter()
        .chain(base.map(|b| b.nodes.iter()).into_iter().flatten())
        .map(|n| (n.id.as_str(), n))
        .collect();
    for (id, s) in &r.changes {
        if let Some(n) = lookup.get(id.as_str()) {
            by_file
                .entry(&n.file)
                .or_default()
                .push((*s, &n.name, &n.kind));
        }
    }
    if by_file.is_empty() {
        return "no changed nodes\n".into();
    }
    let mut s = String::new();
    for (file, ns) in by_file {
        s.push_str(file);
        s.push('\n');
        for (st, name, kind) in ns {
            s.push_str(&format!("  {} {name} ({kind})\n", st.mark()));
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scoping() {
        use crate::model::{Edge, Metrics, Node, Range};
        let node = |id: &str, file: &str, hash: &str| Node {
            id: id.into(),
            kind: "function".into(),
            name: id.into(),
            file: file.into(),
            range: Range {
                start_line: 1,
                end_line: 1,
            },
            parent: None,
            package: None,
            exported: false,
            hash: hash.into(),
            metrics: Metrics {
                loc: 1,
                complexity: 1,
            },
            intents: vec![],
        };
        let edge = |f: &str, t: &str| Edge {
            from: f.into(),
            to: t.into(),
            kind: "calls".into(),
            confidence: "resolved".into(),
            file: "a.ts".into(),
            line: 1,
            namespace: false,
            reexport: false,
        };
        let mut base = Graph::empty();
        base.nodes = vec![
            node("a", "a.ts", "1"),
            node("b", "b.ts", "1"),
            node("c", "c.ts", "1"),
        ];
        base.edges = vec![edge("a", "b")];
        let mut head = base.clone();
        head.nodes[1].hash = "2".into();
        head.edges.push(edge("c", "a"));
        let d = GraphDiff::new(Some(&base), &head);
        let f = |file: &str| Finding::new("r", Severity::Error, "t", file, 1, "m".into());
        assert!(in_scope(&f("a.ts").edge("c", "a"), &d), "new edge");
        assert!(
            !in_scope(&f("b.ts").node("b").edge("a", "b"), &d),
            "old edge wins over changed node"
        );
        assert!(in_scope(&f("b.ts").node("b"), &d), "changed node");
        assert!(!in_scope(&f("a.ts").node("a"), &d), "unchanged node");
        assert!(in_scope(&f("b.ts"), &d), "file with a changed node");
        assert!(!in_scope(&f("c.ts"), &d), "untouched file");
    }

    #[test]
    fn snippet_marks_line() {
        let lines = ["a", "b", "c", "d", "e"];
        assert_eq!(snippet(&lines, 1).unwrap(), "> 1 | a\n  2 | b\n  3 | c");
        assert!(snippet(&lines, 9).is_none());
    }
}
