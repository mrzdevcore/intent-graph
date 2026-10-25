//! The text forms of a check result.

use super::CheckResult;
use ig_core::config::Severity;
use ig_core::diff::Status;
use ig_core::model::{Finding, Graph, Node};
use std::collections::{BTreeMap, HashMap};

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
    let lookup: HashMap<&str, &Node> = r
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
