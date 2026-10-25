//! Graph diff.

use crate::model::{Edge, Graph, Node};
use serde::Serialize;
use std::collections::{BTreeMap, HashMap, HashSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Added,
    Removed,
    Changed,
    Unchanged,
}

impl Status {
    pub fn mark(self) -> &'static str {
        match self {
            Status::Added => "+",
            Status::Removed => "-",
            Status::Changed => "~",
            Status::Unchanged => " ",
        }
    }
}

pub struct GraphDiff {
    pub nodes: BTreeMap<String, Status>,
    pub edges: BTreeMap<String, Status>,
    /// from|to pairs of added edges.
    pub new_pairs: HashSet<(String, String)>,
    /// Files with an added or changed node.
    pub touched_files: HashSet<String>,
}

impl GraphDiff {
    pub fn new(base: Option<&Graph>, head: &Graph) -> GraphDiff {
        let empty = Graph::empty();
        let base = base.unwrap_or(&empty);
        let bn: HashMap<&str, &Node> = base.nodes.iter().map(|n| (n.id.as_str(), n)).collect();
        let hn: HashSet<&str> = head.nodes.iter().map(|n| n.id.as_str()).collect();
        let mut nodes = BTreeMap::new();
        let mut touched_files = HashSet::new();
        for n in &head.nodes {
            let s = match bn.get(n.id.as_str()) {
                None => Status::Added,
                Some(b) if b.hash != n.hash => Status::Changed,
                Some(_) => Status::Unchanged,
            };
            if s != Status::Unchanged {
                touched_files.insert(n.file.clone());
            }
            nodes.insert(n.id.clone(), s);
        }
        for n in &base.nodes {
            if !hn.contains(n.id.as_str()) {
                nodes.insert(n.id.clone(), Status::Removed);
            }
        }
        let be: HashSet<String> = base.edges.iter().map(Edge::key).collect();
        let he: HashSet<String> = head.edges.iter().map(Edge::key).collect();
        let mut edges = BTreeMap::new();
        let mut new_pairs = HashSet::new();
        for e in &head.edges {
            let k = e.key();
            if be.contains(&k) {
                edges.insert(k, Status::Unchanged);
            } else {
                new_pairs.insert((e.from.clone(), e.to.clone()));
                edges.insert(k, Status::Added);
            }
        }
        for k in be.difference(&he) {
            edges.insert(k.clone(), Status::Removed);
        }
        GraphDiff {
            nodes,
            edges,
            new_pairs,
            touched_files,
        }
    }

    pub fn status(&self, id: &str) -> Status {
        self.nodes.get(id).copied().unwrap_or(Status::Unchanged)
    }

    pub fn is_touched(&self, id: &str) -> bool {
        matches!(self.status(id), Status::Added | Status::Changed)
    }

    pub fn count(&self, s: Status) -> usize {
        self.nodes.values().filter(|x| **x == s).count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Metrics, Range};

    fn node(id: &str, hash: &str) -> Node {
        Node {
            id: id.into(),
            kind: "function".into(),
            name: id.into(),
            file: "a.ts".into(),
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
        }
    }

    #[test]
    fn statuses() {
        let mut b = Graph::empty();
        b.nodes = vec![node("a", "1"), node("b", "1"), node("c", "1")];
        let mut h = Graph::empty();
        h.nodes = vec![node("a", "1"), node("b", "2"), node("d", "1")];
        let d = GraphDiff::new(Some(&b), &h);
        assert_eq!(d.status("a"), Status::Unchanged);
        assert_eq!(d.status("b"), Status::Changed);
        assert_eq!(d.status("c"), Status::Removed);
        assert_eq!(d.status("d"), Status::Added);
    }
}
