//! `ig/complexity`: functions over the max, or that grew too much.

use crate::config::{Config, Severity};
use crate::diff::{GraphDiff, Status};
use crate::model::{Finding, Graph, Node};
use std::collections::HashMap;

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
