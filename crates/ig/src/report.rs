//! `ig report --gitlab`: Mermaid diagram and one MR note, created or updated in place.

use crate::render;
use anyhow::{Context, Result, bail};
use ig_app::check::{self, Run};
use ig_app::view_data as view;
use ig_core::config::Severity;
use ig_core::diff::Status;
use ig_wire::Wired;
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;

pub const MARKER: &str = "<!-- ig-report -->";
const MAX_NODES: usize = 40;
const MAX_ROWS: usize = 50;

fn label(s: &str) -> String {
    s.replace('"', "#quot;")
        .replace('<', "#lt;")
        .replace('>', "#gt;")
}

fn class(s: Status) -> Option<&'static str> {
    match s {
        Status::Added => Some("added"),
        Status::Changed => Some("changed"),
        Status::Removed => Some("removed"),
        Status::Unchanged => None,
    }
}

/// A flowchart of the changed nodes and their neighbours, collapsed to modules above 40 nodes.
pub fn mermaid(run: &Run) -> String {
    let nodes = view::all_nodes(run);
    let edges = view::all_edges(run);
    let viol = view::violations(run);
    let pairs: Vec<(&str, &str)> = edges
        .iter()
        .map(|(e, _)| (e.from.as_str(), e.to.as_str()))
        .collect();
    let set = view::changed_set(run, &pairs);
    let by_id: HashMap<&str, (&crate::model::Node, Status)> = nodes
        .iter()
        .map(|(n, s)| (n.id.as_str(), (*n, *s)))
        .collect();

    // (id, label, status) and (from, to, violating)
    let mut ns: BTreeMap<String, (String, Status)> = BTreeMap::new();
    let mut es: BTreeMap<(String, String), bool> = BTreeMap::new();
    if set.len() <= MAX_NODES {
        for id in &set {
            if let Some((n, s)) = by_id.get(id) {
                let l = if n.is_module() {
                    n.file.clone()
                } else {
                    format!("{}<br/>{}", n.name, n.file)
                };
                ns.insert(id.to_string(), (l, *s));
            }
        }
        for (e, _) in &edges {
            if ns.contains_key(&e.from) && ns.contains_key(&e.to) && e.from != e.to {
                *es.entry((e.from.clone(), e.to.clone())).or_default() |=
                    viol.contains(&(e.from.clone(), e.to.clone()));
            }
        }
    } else {
        let file_of = |id: &str| by_id.get(id).map(|(n, _)| n.file.clone());
        for id in &set {
            let Some((n, s)) = by_id.get(id) else {
                continue;
            };
            let entry = ns
                .entry(n.file.clone())
                .or_insert((n.file.clone(), Status::Unchanged));
            if n.is_module() && *s != Status::Unchanged {
                entry.1 = *s;
            } else if *s != Status::Unchanged && entry.1 == Status::Unchanged {
                entry.1 = Status::Changed;
            }
        }
        for (e, _) in &edges {
            let (Some(a), Some(b)) = (file_of(&e.from), file_of(&e.to)) else {
                continue;
            };
            if a != b && ns.contains_key(&a) && ns.contains_key(&b) {
                *es.entry((a, b)).or_default() |= viol.contains(&(e.from.clone(), e.to.clone()));
            }
        }
    }
    let idx: HashMap<&str, usize> = ns
        .keys()
        .enumerate()
        .map(|(i, k)| (k.as_str(), i))
        .collect();
    let mut s = String::from("flowchart LR\n");
    s.push_str("  classDef added fill:#d3f9d8,stroke:#2f9e44\n  classDef changed fill:#fff3bf,stroke:#d98a00\n  classDef removed fill:#ffdeeb,stroke:#d6336c,stroke-dasharray:4\n");
    for (id, (l, st)) in &ns {
        s.push_str(&format!(
            "  n{}[\"{}\"]",
            idx[id.as_str()],
            label(l).replace("#lt;br/#gt;", "<br/>")
        ));
        if let Some(c) = class(*st) {
            s.push_str(&format!(":::{c}"));
        }
        s.push('\n');
    }
    let mut red = vec![];
    for (i, ((a, b), v)) in es.iter().enumerate() {
        s.push_str(&format!(
            "  n{} --> n{}\n",
            idx[a.as_str()],
            idx[b.as_str()]
        ));
        if *v {
            red.push(i.to_string());
        }
    }
    if !red.is_empty() {
        s.push_str(&format!(
            "  linkStyle {} stroke:#c92a2a,stroke-width:2px\n",
            red.join(",")
        ));
    }
    s
}

fn cell(s: &str) -> String {
    s.replace('|', "\\|").replace('\n', " ")
}

pub fn note(run: &Run, artifact: Option<&str>) -> String {
    let r = &run.result;
    let count = |sev| r.findings.iter().filter(|f| f.severity == sev).count();
    let mut s = format!("{MARKER}\n");
    s.push_str(&if r.blocking {
        format!(
            "### ig check: ❌ blocking ({} errors, {} warnings)\n\n",
            count(Severity::Error),
            count(Severity::Warning)
        )
    } else {
        format!(
            "### ig check: ✅ not blocking ({} warnings)\n\n",
            count(Severity::Warning)
        )
    });
    for n in &r.notices {
        s.push_str(&format!("> {n}\n"));
    }
    if !r.notices.is_empty() {
        s.push('\n');
    }
    if !r.intent_changes.is_empty() {
        s.push_str("**Intents**\n\n```diff\n");
        for c in &r.intent_changes {
            let m = match c.status {
                Status::Added => "+",
                Status::Removed => "-",
                _ => "!",
            };
            s.push_str(&format!("{m} {}: {}\n", c.id, c.text));
        }
        s.push_str("```\n\n");
    }
    if !r.changes.is_empty() {
        s.push_str(&format!(
            "**Changes** ({} added, {} changed, {} removed)\n\n```mermaid\n{}```\n\n",
            r.counts.added,
            r.counts.changed,
            r.counts.removed,
            mermaid(run)
        ));
    }
    if !r.findings.is_empty() {
        s.push_str("| Severity | Rule | Location | Message |\n|---|---|---|---|\n");
        for f in r.findings.iter().take(MAX_ROWS) {
            s.push_str(&format!(
                "| {} | `{}` | `{}:{}` | {} |\n",
                f.severity.as_str(),
                f.rule_id,
                cell(&f.file),
                f.line,
                cell(&f.message)
            ));
        }
        if r.findings.len() > MAX_ROWS {
            s.push_str(&format!("\n…and {} more.\n", r.findings.len() - MAX_ROWS));
        }
        s.push('\n');
    }
    if !run.intents.is_empty() {
        let tests = ig_core::workspace::globset(&run.config.tests)
            .unwrap_or_else(|_| globset::GlobSet::empty());
        let (mut imp, mut tst): (HashSet<&str>, HashSet<&str>) = (HashSet::new(), HashSet::new());
        for n in &r.graph.nodes {
            for t in &n.intents {
                if n.kind == "test" || tests.is_match(&n.file) {
                    tst.insert(t);
                } else {
                    imp.insert(t);
                }
            }
        }
        s.push_str("| Intent | Implemented | Tested |\n|---|---|---|\n");
        let yes = |b: bool| if b { "✅" } else { "—" };
        for i in &run.intents {
            s.push_str(&format!(
                "| **{}** {} | {} | {} |\n",
                i.id,
                cell(&i.text),
                yes(imp.contains(i.id.as_str())),
                yes(tst.contains(i.id.as_str()))
            ));
        }
        s.push('\n');
    }
    if let Some(a) = artifact {
        s.push_str(&format!("[Open the graph viewer]({a}) · "));
    }
    let base = r
        .base
        .as_deref()
        .map(|b| format!(" --base {b}"))
        .unwrap_or_default();
    s.push_str(&format!("Reproduce: `ig check{base}`\n"));
    s
}

fn env(k: &str) -> Option<String> {
    std::env::var(k).ok().filter(|v| !v.is_empty())
}

pub fn gitlab(root: &Path) -> Result<u8> {
    let w = Wired::new(root);
    let e = w.env();
    let run = check::run(&e, &check::Opts::default())?;
    render::write_run(&e, &run, &check::Opts::default())?;
    let prefix = w.vcs().map(|r| r.prefix().to_string()).unwrap_or_default();
    let artifact_path = if prefix.is_empty() {
        ".ig/view.html".to_string()
    } else {
        format!("{prefix}/.ig/view.html")
    };
    let artifact = env("CI_JOB_URL").map(|u| format!("{u}/artifacts/file/{artifact_path}"));
    let body = note(&run, artifact.as_deref());

    let (Some(api), Some(project), Some(mr)) = (
        env("CI_API_V4_URL"),
        env("CI_PROJECT_ID"),
        env("CI_MERGE_REQUEST_IID"),
    ) else {
        bail!(
            "report --gitlab: needs CI_API_V4_URL, CI_PROJECT_ID and CI_MERGE_REQUEST_IID (run in a merge request pipeline)"
        );
    };
    let token = env("IG_GITLAB_TOKEN")
        .or_else(|| env("GITLAB_TOKEN"))
        .context("report --gitlab: set IG_GITLAB_TOKEN or GITLAB_TOKEN")?;
    let notes = format!(
        "{}/projects/{}/merge_requests/{mr}/notes",
        api.trim_end_matches('/'),
        project
    );
    let agent = ureq::Agent::new_with_defaults();
    let mut page = "1".to_string();
    let existing = loop {
        let mut resp = agent
            .get(&format!("{notes}?per_page=100&page={page}"))
            .header("PRIVATE-TOKEN", &token)
            .call()
            .context("listing MR notes")?;
        let next = resp
            .headers()
            .get("x-next-page")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();
        let list: Value = resp.body_mut().read_json().context("parsing MR notes")?;
        let found = list
            .as_array()
            .into_iter()
            .flatten()
            .find(|n| n["body"].as_str().is_some_and(|b| b.contains(MARKER)))
            .and_then(|n| n["id"].as_u64());
        if found.is_some() || next.is_empty() {
            break found;
        }
        page = next;
    };
    match existing {
        Some(id) => {
            agent
                .put(&format!("{notes}/{id}"))
                .header("PRIVATE-TOKEN", &token)
                .send_json(json!({"body": body}))
                .context("updating MR note")?;
            eprintln!("ig: updated MR note {id}");
        }
        None => {
            agent
                .post(&notes)
                .header("PRIVATE-TOKEN", &token)
                .send_json(json!({"body": body}))
                .context("creating MR note")?;
            eprintln!("ig: created MR note");
        }
    }
    Ok(if run.result.blocking { 1 } else { 0 })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mermaid_and_note() {
        let src =
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/monorepo");
        let dir = std::env::temp_dir().join(format!("ig-report-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let cp = std::process::Command::new("cp")
            .arg("-R")
            .arg(&src)
            .arg(&dir)
            .status()
            .unwrap();
        assert!(cp.success());
        let run = crate::tests::check_run(&dir, &check::Opts::default()).unwrap();
        // Outside git everything is added: above 40 nodes, so the diagram collapses to modules.
        let m = mermaid(&run);
        assert!(m.starts_with("flowchart LR\n"));
        assert!(m.contains("[\"packages/core/src/leak.ts\"]:::added"));
        assert!(m.contains("linkStyle") && m.contains("stroke:#c92a2a"));
        let n = note(&run, Some("https://ci/job/1/artifacts/file/.ig/view.html"));
        assert!(n.starts_with(MARKER));
        assert!(n.contains("### ig check: ❌ blocking"));
        assert!(n.contains("| error | `ig/arch` | `packages/core/src/leak.ts:1` |"));
        assert!(n.contains("| **WEB-2** nobody implemented this | — | — |"));
        assert!(
            n.contains("[Open the graph viewer](https://ci/job/1/artifacts/file/.ig/view.html)")
        );
        assert_eq!(label("a\"<b>"), "a#quot;#lt;b#gt;");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
