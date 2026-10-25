//! `ig apply`: apply state (`.ig/apply.json`), plan conformance, tamper detection, exceptions.

use crate::check::{self, Run};
use crate::checks::{self, Layers};
use crate::config::{Config, Severity};
use crate::diff::Status;
use crate::git::Repo;
use crate::model::{Edge, Finding, Node, sha1_bytes, sha1_hex};
use crate::plan::{self, Plan, PlanStatus};
use crate::view::LaneMap;
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApplyState {
    pub plan: String,
    pub base: String,
    pub hashes: BTreeMap<String, String>,
    /// The baseline's fingerprints at start, so finish can list the exceptions added since.
    #[serde(default)]
    pub baseline: Vec<String>,
}

const STATE: &str = ".ig/apply.json";

pub fn load(root: &Path) -> Option<ApplyState> {
    serde_json::from_str(&std::fs::read_to_string(root.join(STATE)).ok()?).ok()
}

/// The apply in progress, when it is usable and no explicit `base` overrides it.
pub fn active(root: &Path, base: Option<&str>) -> Option<ApplyState> {
    load(root).filter(|st| base.is_none() && stale(root, st).is_none())
}

/// Why `st` can't be used (its plan or base commit is gone), or None when it is usable.
pub fn stale(root: &Path, st: &ApplyState) -> Option<String> {
    if !root.join(&st.plan).is_file() {
        Some(format!("plan {} is missing", st.plan))
    } else if crate::git::git(
        root,
        &["cat-file", "-e", &format!("{}^{{commit}}", st.base)],
    )
    .is_err()
    {
        Some(format!("base commit {} is missing", st.base))
    } else {
        None
    }
}

fn save(root: &Path, st: &ApplyState) -> Result<()> {
    std::fs::create_dir_all(root.join(".ig"))?;
    check::write_atomic(&root.join(STATE), &serde_json::to_vec_pretty(st)?)
}

fn hash_file(root: &Path, rel: &str) -> String {
    std::fs::read(root.join(rel))
        .map(|b| sha1_bytes(&b))
        .unwrap_or_else(|_| "absent".into())
}

/// Files under `root` (posix, root-relative) matching `globs`; skips node_modules, .git and .ig.
fn matching(root: &Path, globs: &[String]) -> Vec<String> {
    let mut out = vec![];
    let Ok(set) = crate::workspace::globset(globs) else {
        return out;
    };
    if globs.is_empty() {
        return out;
    }
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&dir) else {
            continue;
        };
        for e in rd.flatten() {
            let p = e.path();
            let Some(r) = crate::workspace::rel(root, &p) else {
                continue;
            };
            let Ok(ft) = e.file_type() else { continue };
            if ft.is_dir() {
                let name = e.file_name();
                if !matches!(name.to_str(), Some("node_modules" | ".git" | ".ig")) {
                    stack.push(p);
                }
            } else if ft.is_file() && set.is_match(&r) {
                out.push(r);
            }
        }
    }
    out
}

/// Guarded files: path → sha1 of the contents, or `"absent"`.
fn hashes(root: &Path, plan: &str, config: &Config) -> BTreeMap<String, String> {
    let globs: Vec<String> = config
        .checkers
        .iter()
        .flat_map(|c| c.configs.iter().cloned())
        .collect();
    [plan, "ig.yaml", "rules.md", ".ig/baseline.json"]
        .into_iter()
        .map(String::from)
        .chain(matching(root, &globs))
        .map(|p| {
            let h = hash_file(root, &p);
            (p, h)
        })
        .collect()
}

pub fn start(root: &Path, plan_path: &Path) -> Result<()> {
    let (rel, text) = plan::read(root, plan_path)?;
    plan::parse(&rel, &text)?;
    let Some(base) = Repo::open(root).and_then(|r| r.head()) else {
        bail!("apply start: needs a git repository with at least one commit");
    };
    let hashes = hashes(root, &rel, &Config::load(root)?);
    let baseline = checks::load_baseline(root).into_iter().collect();
    save(
        root,
        &ApplyState {
            plan: rel,
            base,
            hashes,
            baseline,
        },
    )
}

/// `ig apply abort`: drops the apply state; returns the plan it was for.
pub fn abort(root: &Path) -> Result<String> {
    let Some(st) = load(root) else {
        bail!("no apply in progress");
    };
    std::fs::remove_file(root.join(STATE))?;
    Ok(st.plan)
}

/// Exceptions (baseline entries with a reason) added since `ig apply start`: (fingerprint, reason).
pub fn exceptions_since(root: &Path, st: &ApplyState) -> Vec<(String, String)> {
    checks::read_baseline(root)
        .unwrap_or_default()
        .iter()
        .filter_map(|v| Some((v["fingerprint"].as_str()?, v["reason"].as_str()?)))
        .filter(|(fp, _)| !st.baseline.iter().any(|b| b == fp))
        .map(|(fp, r)| (fp.to_string(), r.to_string()))
        .collect()
}

pub fn accept(root: &Path) -> Result<()> {
    let Some(mut st) = load(root) else {
        bail!("no apply in progress");
    };
    st.hashes = hashes(root, &st.plan, &Config::load(root)?);
    save(root, &st)
}

/// `ig except`: records an exception with its reason in the baseline. During apply only the
/// baseline's hash is re-recorded, so other pending tamper stays visible. Refuses `ig/plan` and
/// `ig/tamper` findings of the last check: those are never baselined.
pub fn except(root: &Path, fingerprint: &str, reason: &str) -> Result<()> {
    const BASELINE: &str = ".ig/baseline.json";
    let last: Value = std::fs::read_to_string(root.join(".ig/findings.json"))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default();
    let rule = last["findings"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|f| f["fingerprint"].as_str() == Some(fingerprint))
        .and_then(|f| f["ruleId"].as_str());
    if let Some(rule @ ("ig/plan" | "ig/tamper")) = rule {
        bail!(
            "except: {fingerprint} is an {rule} finding; exceptions are for tool and review findings only. Update the plan (with the user's yes), then run `ig apply accept`"
        );
    }
    let mut items = checks::read_baseline(root)?;
    items.retain(|v| v.as_str().or_else(|| v["fingerprint"].as_str()) != Some(fingerprint));
    items.push(json!({"fingerprint": fingerprint, "reason": reason}));
    std::fs::create_dir_all(root.join(".ig"))?;
    std::fs::write(
        root.join(BASELINE),
        serde_json::to_string_pretty(&items)? + "\n",
    )?;
    if let Some(mut st) = load(root) {
        st.hashes.insert(BASELINE.into(), hash_file(root, BASELINE));
        save(root, &st)?;
    }
    Ok(())
}

/// sha1 of the sorted `id:hash` lines of the added and changed head nodes.
pub fn change_hash(run: &Run) -> String {
    let mut lines: Vec<String> = run
        .result
        .graph
        .nodes
        .iter()
        .filter(|n| run.diff.is_touched(&n.id))
        .map(|n| format!("{}:{}", n.id, n.hash))
        .collect();
    lines.sort();
    sha1_hex(&lines.join("\n"))
}

fn err(rule: &str, file: &str, line: u32, msg: String) -> Finding {
    Finding::new(rule, Severity::Error, "ig", file, line, msg)
}

fn file_of(id: &str) -> &str {
    id.split('#').next().unwrap_or(id)
}

/// `planned` (a plan edge end) matches `actual`: equal, or `planned` is a module and `actual` is
/// in its file.
fn end_matches(planned: &str, actual: &str) -> bool {
    planned == actual
        || (!planned.contains('#') && !planned.starts_with("system:") && file_of(actual) == planned)
}

fn edge_planned(plan: &Plan, e: &Edge) -> bool {
    plan.edges
        .iter()
        .any(|p| end_matches(&p.from, &e.from) && end_matches(&p.to, &e.to))
}

fn unplanned(plan: &Plan, run: &Run) -> Vec<Finding> {
    let g = &run.result.graph;
    let msg = |what: &str| {
        format!("not in the plan: {what}; update the plan with the user's agreement or remove it")
    };
    let planned: HashSet<&str> = plan
        .nodes
        .iter()
        .flat_map(|n| [n.id.as_str(), file_of(&n.id)])
        .collect();
    let tests = crate::workspace::globset(&run.config.tests).unwrap_or_default();
    let is_test = |n: &Node| n.kind == "test" || tests.is_match(&n.file);
    // Pattern nodes (routes, ...) are API surface even though they are never `exported`.
    let api = |n: &Node| {
        n.is_module() || n.exported || run.config.patterns.iter().any(|p| p.kind == n.kind)
    };
    let mut out = vec![];
    for n in &g.nodes {
        if run.diff.status(&n.id) == Status::Added
            && !planned.contains(n.id.as_str())
            && !is_test(n)
            && api(n)
        {
            out.push(err("ig/plan", &n.file, n.range.start_line, msg(&n.id)).node(&n.id));
        }
    }
    let nodes: HashMap<&str, &Node> = g.nodes.iter().map(|n| (n.id.as_str(), n)).collect();
    let layers = Layers::new(&run.config, &run.packages);
    let lanes = LaneMap::new(&run.config, &layers);
    for e in &g.edges {
        if run.diff.edges.get(&e.key()) != Some(&Status::Added) {
            continue;
        }
        let bad = match e.to.strip_prefix("system:") {
            Some(name) => !plan.systems.iter().any(|s| s == name),
            None => {
                let (Some(a), Some(b)) = (nodes.get(e.from.as_str()), nodes.get(e.to.as_str()))
                else {
                    continue;
                };
                if is_test(a) || is_test(b) {
                    continue;
                }
                (layers.of(&a.file) != layers.of(&b.file) || lanes.of(a) != lanes.of(b))
                    && !edge_planned(plan, e)
            }
        };
        if bad {
            let what = format!("{} -> {}", e.from, e.to);
            out.push(err("ig/plan", &e.file, e.line, msg(&what)).edge(&e.from, &e.to));
        }
    }
    out
}

fn missing(plan: &Plan, run: &Run) -> Vec<Finding> {
    let g = &run.result.graph;
    let ids: HashSet<&str> = g.nodes.iter().map(|n| n.id.as_str()).collect();
    let mut out = vec![];
    for n in &plan.nodes {
        let gone = match n.status {
            PlanStatus::New => !ids.contains(n.id.as_str()),
            PlanStatus::Changed => run.diff.nodes.get(&n.id) == Some(&Status::Unchanged),
            PlanStatus::Removed => false,
        };
        if gone {
            out.push(
                err(
                    "ig/plan",
                    file_of(&n.id),
                    0,
                    format!("planned but not built: {}", n.id),
                )
                .node(&n.id),
            );
        }
    }
    for p in &plan.edges {
        let built = g
            .edges
            .iter()
            .any(|e| end_matches(&p.from, &e.from) && end_matches(&p.to, &e.to));
        if !built {
            let id = format!("{} -> {}", p.from, p.to);
            out.push(
                err(
                    "ig/plan",
                    file_of(&p.from),
                    0,
                    format!("planned but not built: {id}"),
                )
                .edge(&p.from, &p.to),
            );
        }
    }
    out
}

const REVIEW: &str = ".ig/agent-review.json";

/// `ig apply review`: stores the reviewer agent's JSON array (prose around it is ignored) for
/// the current change; returns the finding count.
pub fn review_ingest(root: &Path, json: &str) -> Result<usize> {
    let (Some(a), Some(b)) = (json.find('['), json.rfind(']')) else {
        bail!("apply review: no JSON array in the input");
    };
    let items: Vec<Value> = serde_json::from_str(&json[a..=b.max(a)])
        .context("apply review: the reviewer output is not a JSON array")?;
    for (i, it) in items.iter().enumerate() {
        if !it["rule"].is_string() || !it["message"].is_string() {
            bail!("apply review: item {i} needs a string `rule` and `message`: {it}");
        }
    }
    let run = check::run(root, &check::Opts::default())?;
    std::fs::create_dir_all(root.join(".ig"))?;
    check::write_atomic(
        &root.join(REVIEW),
        &serde_json::to_vec_pretty(&json!({"change": change_hash(&run), "findings": items}))?,
    )?;
    Ok(items.len())
}

/// The stored review, when it matches the current change.
fn stored_review(root: &Path, run: &Run) -> Option<Value> {
    std::fs::read_to_string(root.join(REVIEW))
        .ok()
        .and_then(|t| serde_json::from_str::<Value>(&t).ok())
        .filter(|v| v["change"].as_str() == Some(change_hash(run).as_str()))
}

/// Reviewer findings for the current change, minus excepted (baselined) ones.
fn reviewed(root: &Path, run: &Run) -> Vec<Finding> {
    let Some(v) = stored_review(root, run) else {
        return vec![];
    };
    let found = v["findings"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|i| {
            let s = |k| i[k].as_str().unwrap_or_default();
            Finding::new(
                &format!("review/{}", s("rule")),
                Severity::parse(s("severity")).unwrap_or(Severity::Error),
                "ig-review",
                s("file"),
                i["line"].as_u64().unwrap_or(0) as u32,
                s("message").into(),
            )
        })
        .collect();
    let baseline = checks::load_baseline(root);
    checks::attach_nodes(&run.result.graph, found)
        .into_iter()
        .map(Finding::fingerprinted)
        .filter(|f| !baseline.contains(&f.fingerprint))
        .collect()
}

/// Apply findings: unplanned (and, when `final_`, missing) `ig/plan`, plus `ig/tamper`.
pub fn findings(root: &Path, st: &ApplyState, run: &Run, final_: bool) -> Vec<Finding> {
    let mut out = vec![];
    let text = std::fs::read_to_string(root.join(&st.plan)).unwrap_or_default();
    match plan::parse(&st.plan, &text) {
        Ok(p) => {
            out.extend(unplanned(&p, run));
            if final_ {
                out.extend(missing(&p, run));
            }
        }
        Err(e) => out.push(err("ig/plan", &st.plan, 0, format!("{e:#}"))),
    }
    let mut cur = hashes(root, &st.plan, &run.config);
    for k in st.hashes.keys() {
        cur.entry(k.clone()).or_insert_with(|| hash_file(root, k));
    }
    for (path, h) in &cur {
        if st.hashes.get(path).map_or("absent", String::as_str) != h {
            out.push(err(
                "ig/tamper",
                path,
                0,
                format!("{path} changed during apply; ask the user, then run `ig apply accept`"),
            ));
        }
    }
    out.extend(reviewed(root, run));
    out.into_iter().map(Finding::fingerprinted).collect()
}

/// The final check: 0 when nothing blocks (apply is then cleared and the plan marked done).
pub fn finish(root: &Path) -> Result<u8> {
    let Some(st) = load(root) else {
        bail!("no apply in progress");
    };
    if let Some(r) = stale(root, &st) {
        bail!("stale apply state ({r}): run ig apply abort");
    }
    let mut run = check::run(root, &check::Opts::default())?;
    let seen: HashSet<String> = run
        .result
        .findings
        .iter()
        .map(|f| f.fingerprint.clone())
        .collect();
    let extra: Vec<Finding> = findings(root, &st, &run, true)
        .into_iter()
        .filter(|f| !seen.contains(&f.fingerprint))
        .collect();
    run.result.findings.extend(extra);
    let has_review = stored_review(root, &run).is_some();
    if !has_review {
        run.result.findings.push(
            err(
                "ig/review-missing",
                ".ig/agent-review.json",
                0,
                "the reviewer agent has not reviewed this change; run it, then ig apply finish"
                    .into(),
            )
            .fingerprinted(),
        );
    }
    let block_on = run.config.block_on;
    run.result.blocking = run.result.findings.iter().any(|f| f.severity >= block_on);
    // The last check result, so `ig except` sees the final ig/plan findings too.
    check::write_state(root, &run.result)?;
    let excepted = exceptions_since(root, &st);
    if !excepted.is_empty() {
        println!("exceptions added during this apply:");
        for (fp, reason) in &excepted {
            println!("- {fp}: {reason}");
        }
    }
    if run.result.blocking {
        print!("{}", check::agent_text(&run.result, block_on));
        return Ok(1);
    }
    let path = root.join(&st.plan);
    let text = std::fs::read_to_string(&path).with_context(|| st.plan.clone())?;
    if !plan::parse(&st.plan, &text)?.done {
        let mut lines: Vec<&str> = text.lines().collect();
        let at = lines
            .iter()
            .position(|l| l.starts_with("# "))
            .map_or(0, |i| i + 1);
        lines.insert(at, "Status: done");
        std::fs::write(&path, lines.join("\n") + "\n")?;
    }
    std::fs::remove_file(root.join(STATE))?;
    println!("apply finished");
    Ok(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plan::tests::PLAN;
    use std::path::PathBuf;

    fn git_fixture(name: &str) -> PathBuf {
        static N: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let n = N.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let src = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(name);
        let dir = std::env::temp_dir().join(format!("ig-apply-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let sh = |cmd: &mut std::process::Command| assert!(cmd.status().unwrap().success());
        sh(std::process::Command::new("cp")
            .arg("-R")
            .arg(&src)
            .arg(&dir));
        for args in [
            &["init", "-q"][..],
            &["add", "-A"],
            &[
                "-c",
                "commit.gpgsign=false",
                "-c",
                "user.email=t@t",
                "-c",
                "user.name=t",
                "commit",
                "-qm",
                "base",
            ],
        ] {
            sh(std::process::Command::new("git")
                .arg("-C")
                .arg(&dir)
                .args(args));
        }
        dir.canonicalize().unwrap() // like main: plan paths are made root-relative
    }

    fn write(dir: &Path, rel: &str, text: &str) {
        let p = dir.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, text).unwrap();
    }

    #[test]
    fn unplanned_missing_and_tamper() {
        let dir = git_fixture("monorepo");
        write(&dir, "plans/x.md", PLAN);
        start(&dir, &dir.join("plans/x.md")).unwrap();
        // an unplanned exported function in a new file
        write(
            &dir,
            "packages/core/src/extra.ts",
            "export const extra = () => 1;\n",
        );
        let run = check::run(&dir, &Default::default()).unwrap();
        let ids: Vec<_> = run
            .result
            .findings
            .iter()
            .map(|f| (f.rule_id.as_str(), f.message.as_str()))
            .collect();
        assert!(
            ids.iter()
                .any(|(r, m)| *r == "ig/plan" && m.contains("packages/core/src/extra.ts"))
        );
        assert!(
            !ids.iter()
                .any(|(_, m)| m.starts_with("planned but not built"))
        );
        assert_eq!(finish(&dir).unwrap(), 1);
        // the user edits the plan mid-apply (Review Focus 4)
        write(&dir, "plans/x.md", &PLAN.replace("# Export", "# Export v2"));
        let run = check::run(&dir, &Default::default()).unwrap();
        assert!(
            run.result
                .findings
                .iter()
                .any(|f| f.rule_id == "ig/tamper" && f.message.contains("plans/x.md"))
        );
        accept(&dir).unwrap();
        let run = check::run(&dir, &Default::default()).unwrap();
        assert!(!run.result.findings.iter().any(|f| f.rule_id == "ig/tamper"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn finish_clears_state_and_marks_done() {
        let dir = git_fixture("monorepo");
        let plan = "# Tiny\n\n```ig-plan\nnodes:\n  - id: packages/core/src/extra.ts#extra\n    status: new\n    exported: true\n```\n";
        write(&dir, "plans/t.md", plan);
        start(&dir, &dir.join("plans/t.md")).unwrap();
        write(
            &dir,
            "packages/core/src/extra.ts",
            "export const extra = () => 1;\n",
        );
        let run = check::run(&dir, &Default::default()).unwrap();
        assert!(!run.result.blocking);
        let review = json!({"change": change_hash(&run)}).to_string();
        write(&dir, ".ig/agent-review.json", &review);
        assert_eq!(finish(&dir).unwrap(), 0);
        assert!(load(&dir).is_none());
        let text = std::fs::read_to_string(dir.join("plans/t.md")).unwrap();
        assert!(text.starts_with("# Tiny\nStatus: done\n"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn tests_helpers_and_routes() {
        let dir = git_fixture("monorepo");
        let plan = "# R\n\n```ig-plan\nnodes:\n  - id: apps/web/src/routes.ts\n    status: changed\n  - id: packages/core/src/repo.ts\n    status: changed\n```\n";
        write(&dir, "plans/r.md", plan);
        start(&dir, &dir.join("plans/r.md")).unwrap();
        write(
            &dir,
            "packages/core/src/repo2.test.ts",
            "import { Repo } from './repo';\ntest('find', () => new Repo().find('a'));\n",
        );
        let repo = std::fs::read_to_string(dir.join("packages/core/src/repo.ts")).unwrap();
        write(
            &dir,
            "packages/core/src/repo.ts",
            &(repo + "function helper() { return 3; }\n"),
        );
        let routes = std::fs::read_to_string(dir.join("apps/web/src/routes.ts")).unwrap();
        write(
            &dir,
            "apps/web/src/routes.ts",
            &(routes + "app.get('/secret', handler);\n"),
        );
        let run = check::run(&dir, &Default::default()).unwrap();
        let plan_msgs: Vec<&str> = run
            .result
            .findings
            .iter()
            .filter(|f| f.rule_id == "ig/plan")
            .map(|f| f.message.as_str())
            .collect();
        assert!(
            !plan_msgs.iter().any(|m| m.contains("repo2.test.ts")),
            "{plan_msgs:?}"
        );
        assert!(
            !plan_msgs.iter().any(|m| m.contains("helper")),
            "{plan_msgs:?}"
        );
        assert!(
            plan_msgs
                .iter()
                .any(|m| m.contains("apps/web/src/routes.ts#") && m.contains("/secret")),
            "{plan_msgs:?}"
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn except_keeps_other_tamper_visible() {
        let dir = git_fixture("monorepo");
        write(&dir, "plans/x.md", PLAN);
        start(&dir, &dir.join("plans/x.md")).unwrap();
        write(&dir, "plans/x.md", &PLAN.replace("# Export", "# Export v2"));
        except(&dir, "b", "ok").unwrap();
        let run = check::run(&dir, &Default::default()).unwrap();
        let tamper: Vec<&str> = run
            .result
            .findings
            .iter()
            .filter(|f| f.rule_id == "ig/tamper")
            .map(|f| f.file.as_str())
            .collect();
        assert_eq!(tamper, vec!["plans/x.md"]);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn except_refuses_apply_findings_and_finish_lists_exceptions() {
        let dir = git_fixture("monorepo");
        write(&dir, "plans/x.md", PLAN);
        start(&dir, &dir.join("plans/x.md")).unwrap();
        write(
            &dir,
            "packages/core/src/extra.ts",
            "export const extra = () => 1;\n",
        );
        let run = check::run(&dir, &Default::default()).unwrap();
        let fp = run
            .result
            .findings
            .iter()
            .find(|f| f.rule_id == "ig/plan")
            .unwrap()
            .fingerprint
            .clone();
        let e = except(&dir, &fp, "nope").unwrap_err().to_string();
        assert!(e.contains("ig apply accept"), "{e}");
        assert!(!checks::load_baseline(&dir).contains(&fp));
        except(&dir, "tool-fp", "user said fine").unwrap();
        let st = load(&dir).unwrap();
        assert_eq!(
            exceptions_since(&dir, &st),
            vec![("tool-fp".to_string(), "user said fine".to_string())]
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn abort_clears_state_and_finish_on_stale_points_to_it() {
        let dir = git_fixture("monorepo");
        write(&dir, "plans/x.md", PLAN);
        start(&dir, &dir.join("plans/x.md")).unwrap();
        std::fs::remove_file(dir.join("plans/x.md")).unwrap();
        let e = finish(&dir).unwrap_err().to_string();
        assert!(
            e.contains("stale apply state (plan plans/x.md is missing): run ig apply abort"),
            "{e}"
        );
        assert_eq!(abort(&dir).unwrap(), "plans/x.md");
        assert!(load(&dir).is_none());
        assert!(abort(&dir).is_err());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn except_refuses_corrupt_baseline() {
        let dir = git_fixture("monorepo");
        write(&dir, ".ig/baseline.json", "{not json");
        assert!(except(&dir, "b", "r").is_err());
        assert_eq!(
            std::fs::read_to_string(dir.join(".ig/baseline.json")).unwrap(),
            "{not json"
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn stale_state_is_ignored() {
        let dir = git_fixture("monorepo");
        std::fs::create_dir_all(dir.join(".ig")).unwrap();
        std::fs::write(
            dir.join(".ig/apply.json"),
            r#"{"plan":"plans/gone.md","base":"0000000000000000000000000000000000000000","hashes":{}}"#,
        )
        .unwrap();
        let run = check::run(&dir, &Default::default()).unwrap();
        assert!(
            run.result
                .notices
                .iter()
                .any(|n| n.contains("apply state is stale"))
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn exceptions_keep_reason() {
        let dir = git_fixture("monorepo");
        checks::write_baseline(&dir, ["a".to_string()]).unwrap();
        except(&dir, "b", "user accepted: legacy import").unwrap();
        let b = checks::load_baseline(&dir);
        assert!(b.contains("a") && b.contains("b"));
        assert!(
            std::fs::read_to_string(dir.join(".ig/baseline.json"))
                .unwrap()
                .contains("legacy import")
        );
        except(&dir, "b", "again").unwrap();
        let items = checks::read_baseline(&dir).unwrap();
        assert_eq!(items.len(), 2, "{items:?}");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn reviewer_findings_block_until_the_change_moves() {
        let dir = git_fixture("monorepo");
        write(&dir, "plans/x.md", PLAN);
        start(&dir, &dir.join("plans/x.md")).unwrap();
        write(
            &dir,
            "packages/core/src/export.ts",
            "export const exportRows = () => 1;\n",
        );
        let n = review_ingest(&dir, "Here you go:\n[{\"rule\":\"R-PURE\",\"file\":\"packages/core/src/export.ts\",\"line\":1,\"message\":\"side effect\",\"severity\":\"error\"}]").unwrap();
        assert_eq!(n, 1);
        let run = check::run(&dir, &Default::default()).unwrap();
        assert!(
            run.result.blocking
                && run
                    .result
                    .findings
                    .iter()
                    .any(|f| f.rule_id == "review/R-PURE")
        );
        write(
            &dir,
            "packages/core/src/export.ts",
            "export const exportRows = () => 2;\n",
        );
        let run = check::run(&dir, &Default::default()).unwrap();
        assert!(
            !run.result
                .findings
                .iter()
                .any(|f| f.rule_id == "review/R-PURE")
        );
    }

    #[test]
    fn excepted_review_finding_disappears_and_bad_input_errors() {
        let dir = git_fixture("monorepo");
        write(&dir, "plans/x.md", PLAN);
        start(&dir, &dir.join("plans/x.md")).unwrap();
        write(
            &dir,
            "packages/core/src/export.ts",
            "export const exportRows = () => 1;\n",
        );
        review_ingest(&dir, "[{\"rule\":\"R-PURE\",\"file\":\"packages/core/src/export.ts\",\"line\":1,\"message\":\"m\",\"severity\":\"error\"}]").unwrap();
        let run = check::run(&dir, &Default::default()).unwrap();
        let f = run
            .result
            .findings
            .iter()
            .find(|f| f.rule_id == "review/R-PURE")
            .unwrap();
        except(&dir, &f.fingerprint.clone(), "false alarm").unwrap();
        let run = check::run(&dir, &Default::default()).unwrap();
        assert!(
            !run.result
                .findings
                .iter()
                .any(|f| f.rule_id == "review/R-PURE")
        );
        assert_eq!(review_ingest(&dir, "all clean: []").unwrap(), 0);
        assert!(review_ingest(&dir, "no array here").is_err());
        assert!(review_ingest(&dir, "] oops [").is_err());
        assert!(review_ingest(&dir, "[1]").is_err());
        assert!(review_ingest(&dir, "[{\"rule\":\"R\"}]").is_err());
        assert!(review_ingest(&dir, "[{\"rule\":3,\"message\":\"m\"}]").is_err());
    }
}
