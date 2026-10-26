//! Plan files (`plans/*.md` with an `ig-plan` block): parsing, merging into a graph, volume.

mod merge;
mod volume;

pub use merge::merge;
pub use volume::volume;

use serde::Deserialize;

#[derive(Debug, thiserror::Error)]
pub enum PlanError {
    #[error("{0}: no ```ig-plan block")]
    NoBlock(String),
    #[error("{path}: ig-plan block")]
    Block {
        path: String,
        #[source]
        source: serde_yaml_ng::Error,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PlanStatus {
    New,
    Changed,
    Removed,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PlanNode {
    pub id: String,
    #[serde(default)]
    pub kind: Option<String>,
    pub status: PlanStatus,
    #[serde(default)]
    pub exported: bool,
}

fn calls() -> String {
    "calls".into()
}

#[derive(Debug, Clone, Deserialize)]
pub struct PlanEdge {
    pub from: String,
    pub to: String,
    #[serde(default = "calls")]
    pub kind: String,
}

/// A design element, link or bridge the user agreed to leave out of the plan.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Omitted {
    pub id: String,
    pub reason: String,
}

#[derive(Default, Deserialize)]
struct Block {
    #[serde(default)]
    omitted: Vec<Omitted>,
    #[serde(default)]
    nodes: Vec<PlanNode>,
    #[serde(default)]
    edges: Vec<PlanEdge>,
    #[serde(default)]
    systems: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct Plan {
    pub path: String,
    pub title: String,
    pub done: bool,
    pub steps: Vec<String>,
    pub nodes: Vec<PlanNode>,
    pub edges: Vec<PlanEdge>,
    pub systems: Vec<String>,
    pub omitted: Vec<Omitted>,
}

pub fn parse(path: &str, text: &str) -> Result<Plan, PlanError> {
    let lines: Vec<&str> = text.lines().collect();
    let title = lines
        .iter()
        .find_map(|l| l.strip_prefix("# "))
        .unwrap_or_default()
        .trim()
        .to_string();
    let done = lines.iter().any(|l| l.trim() == "Status: done");
    let mut steps = vec![];
    if let Some(i) = lines.iter().position(|l| l.trim() == "## Steps") {
        for l in lines[i + 1..].iter().map(|l| l.trim()) {
            if l.starts_with('#') || l.starts_with("```") {
                break;
            }
            if let Some((n, rest)) = l.split_once(". ")
                && !n.is_empty()
                && n.bytes().all(|b| b.is_ascii_digit())
            {
                steps.push(rest.trim().to_string());
            }
        }
    }
    let open = lines
        .iter()
        .position(|l| l.trim() == "```ig-plan")
        .ok_or_else(|| PlanError::NoBlock(path.into()))?;
    let body: Vec<&str> = lines[open + 1..]
        .iter()
        .take_while(|l| l.trim() != "```")
        .copied()
        .collect();
    let block: Block = if body.iter().all(|l| l.trim().is_empty()) {
        Block::default()
    } else {
        serde_yaml_ng::from_str(&body.join("\n")).map_err(|source| PlanError::Block {
            path: path.into(),
            source,
        })?
    };
    Ok(Plan {
        path: path.into(),
        title,
        done,
        steps,
        nodes: block.nodes,
        edges: block.edges,
        systems: block.systems,
        omitted: block.omitted,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Graph;

    #[test]
    fn parse_errors_and_merge_into_empty_graph() {
        let e = parse("p.md", "# T\n").unwrap_err();
        assert_eq!(e.to_string(), "p.md: no ```ig-plan block");
        let e = parse("p.md", "```ig-plan\nnodes: 3\n```\n").unwrap_err();
        assert_eq!(e.to_string(), "p.md: ig-plan block");
        assert!(std::error::Error::source(&e).is_some());

        let text = "# T\nStatus: done\n## Steps\n1. a\n2. b\n```ig-plan\nnodes:\n  - id: src/a.ts#f\n    status: new\n  - id: src/b.ts\n    status: changed\nedges:\n  - from: src/a.ts#f\n    to: src/z.ts\n```\n";
        let p = parse("p.md", text).unwrap();
        assert_eq!((p.title.as_str(), p.done, p.steps.len()), ("T", true, 2));
        let (g, added, f) = merge(&Graph::empty(), &p);
        assert_eq!(g.nodes.len(), 2); // src/a.ts and src/a.ts#f
        assert!(added.contains("src/a.ts") && added.contains("src/a.ts#f"));
        let rules: Vec<&str> = f.iter().map(|x| x.rule_id.as_str()).collect();
        assert_eq!(rules, ["plan/unknown-node", "plan/unknown-edge-end"]);
    }

    #[test]
    fn omitted_parses_and_defaults_to_empty() {
        let p = parse(
            "p.md",
            "```ig-plan
```
",
        )
        .unwrap();
        assert!(p.omitted.is_empty());
        let t = "```ig-plan
omitted:
  - id: fn-a
    reason: later
```
";
        let p = parse("p.md", t).unwrap();
        assert_eq!(
            p.omitted,
            [Omitted {
                id: "fn-a".into(),
                reason: "later".into()
            }]
        );
    }
}
