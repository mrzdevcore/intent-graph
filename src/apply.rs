//! `ig apply`: apply state (`.ig/apply.json`), plan conformance, tamper detection, exceptions.

use crate::check::{self, Run};
use crate::checks::Layers;
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
}

const STATE: &str = ".ig/apply.json";

pub fn load(root: &Path) -> Option<ApplyState> {
    serde_json::from_str(&std::fs::read_to_string(root.join(STATE)).ok()?).ok()
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
    save(
        root,
        &ApplyState {
            plan: rel,
            base,
            hashes,
        },
    )
}

pub fn accept(root: &Path) -> Result<()> {
    let Some(mut st) = load(root) else {
        bail!("no apply in progress");
    };
    st.hashes = hashes(root, &st.plan, &Config::load(root)?);
    save(root, &st)
}

/// `ig except`: records an exception with its reason in the baseline.
pub fn except(root: &Path, fingerprint: &str, reason: &str) -> Result<()> {
    let path = root.join(".ig/baseline.json");
    let mut items: Vec<Value> = std::fs::read_to_string(&path)
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default();
    items.push(json!({"fingerprint": fingerprint, "reason": reason}));
    std::fs::create_dir_all(root.join(".ig"))?;
    std::fs::write(&path, serde_json::to_string_pretty(&items)? + "\n")?;
    if load(root).is_some() {
        accept(root)?;
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
    let mut out = vec![];
    for n in &g.nodes {
        if run.diff.status(&n.id) == Status::Added
            && !planned.contains(n.id.as_str())
            && (n.is_module() || n.exported)
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
    out.into_iter().map(Finding::fingerprinted).collect()
}

/// The final check: 0 when nothing blocks (apply is then cleared and the plan marked done).
pub fn finish(root: &Path) -> Result<u8> {
    let Some(st) = load(root) else {
        bail!("no apply in progress");
    };
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
    let reviewed = std::fs::read_to_string(root.join(".ig/agent-review.json"))
        .ok()
        .and_then(|t| serde_json::from_str::<Value>(&t).ok())
        .is_some_and(|v| v["change"].as_str() == Some(change_hash(&run).as_str()));
    if !reviewed {
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
    if run.result.blocking {
        print!("{}", check::agent_text(&run.result, block_on));
        return Ok(1);
    }
    std::fs::remove_file(root.join(STATE))?;
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
    println!("apply finished");
    Ok(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::checks;
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
        dir
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
        let _ = std::fs::remove_dir_all(dir);
    }
}
