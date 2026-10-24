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
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};

#[derive(Default, Clone)]
pub struct Opts {
    pub base: Option<String>,
    pub head: Option<String>,
    pub all: bool,
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
    let work_cfg = Config::load(root)?;
    let mut notices = vec![];
    let repo = Repo::open(root);
    let Some(repo) = repo else {
        if opts.head.is_some() || opts.base.is_some() {
            bail!("--base/--head need a git repository");
        }
        notices.push("not a git repository: every node counts as added".into());
        return Ok(Trees { head_root: root.into(), head_commit: None, base: None, repo: None, config: work_cfg, notices, _wt: None });
    };
    let base = repo.resolve_base(opts.base.as_deref(), &mut notices)?;
    let head_sha = repo.head();
    let (head_root, head_commit, wt, config) = match &opts.head {
        None => (root.to_path_buf(), head_sha, None, work_cfg),
        Some(h) => {
            let Some(sha) = git::verify(&repo.top, h) else { bail!("--head {h}: not a commit") };
            if Some(&sha) == head_sha.as_ref() && repo.is_clean() {
                (root.to_path_buf(), Some(sha), None, work_cfg)
            } else {
                let wt = Worktree::add(&repo, &sha, &git::node_modules_sources(root, &work_cfg))?;
                let r = wt.root(&repo);
                // Trust boundary: checkers always come from the working tree's ig.yaml.
                let mut cfg = if r.is_dir() { Config::load(&r)? } else { Config::default() };
                cfg.checkers = work_cfg.checkers.clone();
                (r, Some(sha), Some(wt), cfg)
            }
        }
    };
    Ok(Trees { head_root, head_commit, base, repo: Some(repo), config, notices, _wt: wt })
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
            .map(|i| format!("{} {:>w$} | {}", if i == line { ">" } else { " " }, i, lines[i as usize - 1]))
            .collect::<Vec<_>>()
            .join("\n"),
    )
}

pub fn run(root: &Path, opts: &Opts) -> Result<Run> {
    let t = trees(root, opts)?;
    let config = t.config.clone();
    let mut notices = t.notices.clone();
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
    let head_intents = intents::parse(&std::fs::read_to_string(t.head_root.join("intents.md")).unwrap_or_default());
    let base_intents = match (&t.repo, &t.base) {
        (Some(repo), Some(sha)) => intents::parse(&repo.show(sha, "intents.md").unwrap_or_default()),
        _ => vec![],
    };
    let intent_changes = intents::changes(&base_intents, &head_intents);
    let scope: BTreeSet<String> = if opts.all {
        head_intents.iter().map(|i| i.id.clone()).collect()
    } else {
        intent_changes.iter().filter(|c| c.status != Status::Removed).map(|c| c.id.clone()).collect()
    };

    // Scoped findings.
    let mut scoped = checks::arch(&head, &config, &packages);
    scoped.extend(checks::complexity(&head, base_graph.as_ref(), &diff, &config));
    let ch = checks::checkers(&t.head_root, &config, &head);
    scoped.extend(ch.findings);
    let in_scope = |f: &Finding| -> bool {
        if let Some(e) = &f.edge {
            return diff.new_pairs.contains(&(e.from.clone(), e.to.clone()));
        }
        if let Some(n) = &f.node_id {
            return diff.is_touched(n);
        }
        diff.touched_files.contains(&f.file)
    };
    let mut findings: Vec<Finding> = scoped.into_iter().filter(|f| opts.all || in_scope(f)).collect();

    // Always-on findings.
    for e in &head.parse_errors {
        findings.push(Finding::new("ig/parse-error", Severity::Warning, "ig", &e.file, 1, e.message.clone()));
    }
    findings.extend(ch.errors);
    findings.extend(checks::intents(&head, &head_intents, &scope, &tests));
    let mut fingerprinted: Vec<Finding> = findings.into_iter().map(Finding::fingerprinted).collect();
    if let Some(b) = &base_graph {
        let eps = workspace::globset(&config.entry_points)?;
        let ctx = review::Ctx { config: &config, tests: &tests, packages: &packages, entry_points: &eps };
        fingerprinted.extend(review::regressions(b, &head, &ctx));
    }

    // Post-processing.
    let baseline = checks::load_baseline(root);
    let mut files: HashMap<String, String> = HashMap::new();
    let mut out = vec![];
    for mut f in fingerprinted {
        if let Some(s) = config.severity_for(&f.rule_id) {
            f.severity = s;
        }
        if baseline.contains(&f.fingerprint) {
            continue;
        }
        let text = files
            .entry(f.file.clone())
            .or_insert_with(|| std::fs::read_to_string(t.head_root.join(&f.file)).unwrap_or_default());
        let lines: Vec<&str> = text.lines().collect();
        f.snippet = snippet(&lines, f.line);
        out.push(f);
    }
    out.sort_by(|a, b| {
        (std::cmp::Reverse(a.severity), &a.file, a.line, &a.rule_id, &a.message).cmp(&(std::cmp::Reverse(b.severity), &b.file, b.line, &b.rule_id, &b.message))
    });
    let blocking = out.iter().any(|f| f.severity >= config.block_on);
    for n in &notices {
        eprintln!("ig: {n}");
    }
    notices.dedup();

    let result = CheckResult {
        schema_version: 1,
        tool_version: TOOL_VERSION.into(),
        base: t.base.clone(),
        head: t.head_commit.clone(),
        blocking,
        notices,
        intent_changes,
        counts: Counts {
            added: diff.count(Status::Added),
            changed: diff.count(Status::Changed),
            removed: diff.count(Status::Removed),
            unchanged: diff.count(Status::Unchanged),
        },
        changes: diff.nodes.iter().filter(|(_, s)| **s != Status::Unchanged).map(|(k, v)| (k.clone(), *v)).collect(),
        edge_changes: diff.edges.iter().filter(|(_, s)| **s != Status::Unchanged).map(|(k, v)| (k.clone(), *v)).collect(),
        findings: out,
        graph: head,
    };
    write_state(root, &result)?;
    Ok(Run { result, base_graph, diff, config, packages, repo: t.repo, intents: head_intents })
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
    s.push_str(&format!("nodes: {} added, {} changed, {} removed, {} unchanged\n", c.added, c.changed, c.removed, c.unchanged));
    if r.findings.is_empty() {
        s.push_str("no findings\n");
    }
    for sev in [Severity::Error, Severity::Warning, Severity::Note] {
        let fs: Vec<&Finding> = r.findings.iter().filter(|f| f.severity == sev).collect();
        if fs.is_empty() {
            continue;
        }
        s.push_str(&format!("{}s ({}):\n", sev.as_str(), fs.len()));
        for f in fs {
            s.push_str(&format!("  {}:{}  {}  {}\n", f.file, f.line, f.rule_id, f.message));
        }
    }
    s
}

pub fn agent_text(r: &CheckResult, block_on: Severity) -> String {
    let blocking: Vec<&Finding> = r.findings.iter().filter(|f| f.severity >= block_on).collect();
    if blocking.is_empty() {
        let w = r.findings.iter().filter(|f| f.severity == Severity::Warning).count();
        return format!("ig check: no blocking findings ({w} warnings)\n");
    }
    let mut s = format!("ig check: {} blocking finding{}\n\n", blocking.len(), if blocking.len() == 1 { "" } else { "s" });
    for f in blocking {
        s.push_str(&format!("{}:{} {} [{}] {}\n", f.file, f.line, f.severity.as_str(), f.rule_id, f.message));
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
    let lookup: HashMap<&str, &crate::model::Node> =
        r.graph.nodes.iter().chain(base.map(|b| b.nodes.iter()).into_iter().flatten()).map(|n| (n.id.as_str(), n)).collect();
    for (id, s) in &r.changes {
        if let Some(n) = lookup.get(id.as_str()) {
            by_file.entry(&n.file).or_default().push((*s, &n.name, &n.kind));
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
    fn snippet_marks_line() {
        let lines = ["a", "b", "c", "d", "e"];
        assert_eq!(snippet(&lines, 1).unwrap(), "> 1 | a\n  2 | b\n  3 | c");
        assert!(snippet(&lines, 9).is_none());
    }
}
