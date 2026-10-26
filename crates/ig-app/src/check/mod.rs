//! `ig check` / `ig diff`: base and head trees, the diff, scoped and always-on findings,
//! post-processing, and `.ig/graph.json` + `.ig/findings.json`.

mod checkers;
mod text;

pub use checkers::attach_nodes;
pub use text::{agent_text, diff_json, diff_text, findings_text, summary_text};

use crate::{Env, apply};
use ig_core::config::{Config, Severity};
use ig_core::diff::{GraphDiff, Status};
use ig_core::intents::{self, IntentChange};
use ig_core::model::{Finding, Graph, TOOL_VERSION};
use ig_core::ports::{Checkout, Files};
use ig_core::review;
use ig_core::rules;
use ig_core::workspace::{self, Package};
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

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Counts {
    pub added: usize,
    pub changed: usize,
    pub removed: usize,
    pub unchanged: usize,
}

impl Counts {
    pub fn of(diff: &GraphDiff) -> Counts {
        Counts {
            added: diff.count(Status::Added),
            changed: diff.count(Status::Changed),
            removed: diff.count(Status::Removed),
            unchanged: diff.count(Status::Unchanged),
        }
    }
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
    pub intents: Vec<intents::Intent>,
    /// Ids and edge keys added from a plan; empty without one.
    pub planned: HashSet<String>,
    /// The overlaid plan for the viewer: {title, steps, done, volume, findings}.
    pub plan: Option<serde_json::Value>,
}

/// The head tree (the working tree, or a checkout of `--head`) and the base commit.
struct Trees {
    head_root: PathBuf,
    /// The head tree when it is a checkout; the working tree otherwise.
    head_files: Option<Box<dyn Files>>,
    head_commit: Option<String>,
    base: Option<String>,
    config: Config,
    notices: Vec<String>,
    _wt: Option<Checkout>,
}

impl Trees {
    fn files<'a>(&'a self, env: &'a Env) -> &'a dyn Files {
        self.head_files.as_deref().unwrap_or(env.files)
    }
}

/// Resolves base and head; materialises `--head` when needed.
fn trees(env: &Env, opts: &Opts) -> crate::Result<Trees> {
    let work_cfg = crate::config::load(env.files)?;
    let mut notices = vec![];
    let Some(repo) = env.vcs else {
        if opts.head.is_some() || opts.base.is_some() {
            crate::bail!("--base/--head need a git repository");
        }
        notices.push("not a git repository: every node counts as added".into());
        return Ok(Trees {
            head_root: env.root.into(),
            head_files: None,
            head_commit: None,
            base: None,
            config: work_cfg,
            notices,
            _wt: None,
        });
    };
    let base = repo.resolve_base(opts.base.as_deref(), &mut notices)?;
    let head_sha = repo.head();
    let (head_root, head_files, head_commit, wt, config) = match &opts.head {
        None => (env.root.to_path_buf(), None, head_sha, None, work_cfg),
        Some(h) => {
            let Some(sha) = repo.verify(h) else {
                crate::bail!("--head {h}: not a commit")
            };
            if Some(&sha) == head_sha.as_ref() && repo.is_clean() {
                (env.root.to_path_buf(), None, Some(sha), None, work_cfg)
            } else {
                let wt = repo.checkout(&sha)?;
                let Some(r) = wt.root().map(Path::to_path_buf) else {
                    return Err(crate::Error::HeadRootMissing);
                };
                let work = crate::base::work_info(env, &work_cfg);
                for fe in env.frontends {
                    fe.prepare_checkout(&work, &r, &work_cfg, false)?;
                }
                let files = env.files.at(&r);
                // Trust boundary: checkers always come from the working tree's ig.yaml.
                let mut cfg = crate::config::load(&*files)?;
                cfg.checkers = work_cfg.checkers.clone();
                (r, Some(files), Some(sha), Some(wt), cfg)
            }
        }
    };
    Ok(Trees {
        head_root,
        head_files,
        head_commit,
        base,
        config,
        notices,
        _wt: wt,
    })
}

/// Files the change touches: those with added or changed nodes, plus files git sees as changed
/// against the base or untracked. Repo-relative to the ig root, existing ones only.
fn changed_files(env: &Env, t: &Trees, diff: &GraphDiff, notices: &mut Vec<String>) -> Vec<String> {
    let mut set: BTreeSet<String> = diff.touched_files.iter().cloned().collect();
    if let Some(repo) = env.vcs {
        let head = repo.at(&t.head_root);
        let files = t.files(env);
        set.extend(
            head.changed_files(t.base.as_deref(), notices)
                .into_iter()
                .filter(|f| files.exists(f)),
        );
    }
    set.into_iter().collect()
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

pub fn run(env: &Env, opts: &Opts) -> crate::Result<Run> {
    // Apply in progress: compare against its base. Stale state is ignored, never fatal.
    let apply = apply::active(env, opts.base.as_deref());
    let stale = match (&apply, &opts.base) {
        (None, None) => apply::load(env).and_then(|st| apply::stale(env, &st)),
        _ => None,
    };
    let mut opts = opts.clone();
    if let Some(st) = &apply {
        opts.base = Some(st.base.clone());
    }
    let opts = &opts;
    let t = trees(env, opts)?;
    let head_files = t.files(env);
    let config = t.config.clone();
    let mut notices = t.notices.clone();
    if let Some(r) = stale {
        notices.push(format!("apply state is stale ({r}); ignoring it"));
    }
    (env.progress)("building graph…");
    let built = crate::graph(env, head_files, &config, t.head_commit.clone())?;
    let head = built.graph;
    let packages = built.workspace.packages;
    let base_graph = match (env.vcs, &t.base) {
        (Some(repo), Some(sha)) => Some(crate::base::graph(env, repo, sha, &config)?),
        _ => None,
    };
    let diff = GraphDiff::new(base_graph.as_ref(), &head);
    let tests = workspace::globset(&config.tests)?;

    // Intents.
    let head_intents = intents::parse(&crate::read_text(head_files, "intents.md"));
    let base_intents = match (env.vcs, &t.base) {
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
    let mut scoped = rules::arch(&head, &config, &packages);
    scoped.extend(rules::complexity(
        &head,
        base_graph.as_ref(),
        &diff,
        &config,
    ));
    // None with --all (or no checkers): `{files}` is `.` and nothing is scoped out.
    let changed = (!opts.all && !config.checkers.is_empty())
        .then(|| changed_files(env, &t, &diff, &mut notices));
    let ch = if opts.skip_checkers {
        checkers::saved(env, &config)
    } else {
        checkers::run(env, &t.head_root, &config, &head, changed.as_deref())
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
    findings.extend(rules::intents(&head, &head_intents, &scope, &tests));
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
    let baseline = crate::baseline::load(env);
    let mut files: HashMap<String, String> = HashMap::new();
    let mut post = |mut f: Finding| {
        if let Some(s) = config.severity_for(&f.rule_id) {
            f.severity = s;
        }
        let text = files
            .entry(f.file.clone())
            .or_insert_with(|| crate::read_text(head_files, &f.file));
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
        (env.notice)(n);
    }
    notices.dedup();

    let result = CheckResult {
        schema_version: 1,
        tool_version: TOOL_VERSION.into(),
        base: t.base.clone(),
        head: t.head_commit.clone(),
        notices,
        intent_changes,
        counts: Counts::of(&diff),
        changes: changed_only(&diff.nodes),
        edge_changes: changed_only(&diff.edges),
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
        intents: head_intents,
        planned: HashSet::new(),
        plan: None,
    };
    // Apply findings come after the baseline filter: they cannot be baselined.
    if let Some(st) = &apply {
        let extra: Vec<Finding> = apply::findings(env, st, &run, false)
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
    write_state(env, &run.result)?;
    run.result.findings.truncate(shown);
    Ok(run)
}

/// The entries of `m` that are not unchanged.
pub fn changed_only(m: &BTreeMap<String, Status>) -> BTreeMap<String, Status> {
    m.iter()
        .filter(|(_, s)| **s != Status::Unchanged)
        .map(|(k, v)| (k.clone(), *v))
        .collect()
}

/// `.ig/graph.json` and `.ig/findings.json`.
pub fn write_state(env: &Env, r: &CheckResult) -> crate::Result<()> {
    env.files
        .write_atomic(".ig/graph.json", &serde_json::to_vec(&r.graph)?)?;
    env.files
        .write_atomic(".ig/findings.json", &serde_json::to_vec(r)?)?;
    Ok(())
}

#[cfg(test)]
mod tests;
