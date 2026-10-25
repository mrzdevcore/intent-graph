//! Plan conformance: unplanned and missing `ig/plan` findings, `ig/tamper`, and the reviewer's
//! findings for the current change.

use super::{ApplyState, REVIEW};
use crate::Env;
use crate::check::Run;
use crate::lanes::LaneMap;
use crate::plan::{self, Plan, PlanStatus};
use ig_core::config::Severity;
use ig_core::diff::Status;
use ig_core::model::{Edge, Finding, Node, sha1_hex};
use ig_core::rules::Layers;
use ig_core::workspace::globset;
use serde_json::Value;
use std::collections::{HashMap, HashSet};

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
    let tests = globset(&run.config.tests).unwrap_or_default();
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

/// The stored review, when it matches the current change.
pub(super) fn stored_review(env: &Env, run: &Run) -> Option<Value> {
    crate::read_json::<Value>(env.files, REVIEW)
        .filter(|v| v["change"].as_str() == Some(change_hash(run).as_str()))
}

/// Reviewer findings for the current change, minus excepted (baselined) ones.
fn reviewed(env: &Env, run: &Run) -> Vec<Finding> {
    let Some(v) = stored_review(env, run) else {
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
    let baseline = crate::baseline::load(env);
    crate::check::attach_nodes(&run.result.graph, found)
        .into_iter()
        .map(Finding::fingerprinted)
        .filter(|f| !baseline.contains(&f.fingerprint))
        .collect()
}

/// Apply findings: unplanned (and, when `final_`, missing) `ig/plan`, plus `ig/tamper`.
pub fn findings(env: &Env, st: &ApplyState, run: &Run, final_: bool) -> Vec<Finding> {
    let mut out = vec![];
    let text = crate::read_text(env.files, &st.plan);
    match plan::parse(&st.plan, &text) {
        Ok(p) => {
            out.extend(unplanned(&p, run));
            if final_ {
                out.extend(missing(&p, run));
            }
        }
        Err(e) => out.push(err("ig/plan", &st.plan, 0, format!("{e:#}"))),
    }
    let mut cur = super::hashes(env, &st.plan, &run.config);
    for k in st.hashes.keys() {
        cur.entry(k.clone())
            .or_insert_with(|| super::hash_file(env, k));
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
    out.extend(reviewed(env, run));
    out.into_iter().map(Finding::fingerprinted).collect()
}
