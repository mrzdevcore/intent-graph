//! Graph and finding data model. JSON is camelCase; absent fields are omitted.

use crate::config::Severity;
use serde::{Deserialize, Serialize};
use sha1::{Digest, Sha1};

pub const TOOL_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Range {
    pub start_line: u32,
    pub end_line: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Metrics {
    pub loc: u32,
    pub complexity: u32,
}

fn is_false(b: &bool) -> bool {
    !*b
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Node {
    pub id: String,
    pub kind: String,
    pub name: String,
    pub file: String,
    pub range: Range,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub parent: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub package: Option<String>,
    #[serde(skip_serializing_if = "is_false", default)]
    pub exported: bool,
    pub hash: String,
    pub metrics: Metrics,
    pub intents: Vec<String>,
}

impl Node {
    pub fn is_module(&self) -> bool {
        self.kind == "module" || self.kind == "component"
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Edge {
    pub from: String,
    pub to: String,
    pub kind: String,
    pub confidence: String,
    pub file: String,
    pub line: u32,
    #[serde(skip_serializing_if = "is_false", default)]
    pub namespace: bool,
    #[serde(skip_serializing_if = "is_false", default)]
    pub reexport: bool,
}

impl Edge {
    pub fn key(&self) -> String {
        format!("{}|{}|{}", self.from, self.kind, self.to)
    }
    pub fn resolved(&self) -> bool {
        self.confidence == "resolved"
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ParseError {
    pub file: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Graph {
    pub schema_version: u32,
    pub tool_version: String,
    pub commit: Option<String>,
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
    pub parse_errors: Vec<ParseError>,
}

impl Graph {
    pub fn empty() -> Graph {
        Graph { schema_version: 1, tool_version: TOOL_VERSION.into(), commit: None, nodes: vec![], edges: vec![], parse_errors: vec![] }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EdgeRef {
    pub from: String,
    pub to: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Finding {
    pub rule_id: String,
    pub severity: Severity,
    pub message: String,
    pub tool: String,
    pub file: String,
    pub line: u32,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub node_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub edge: Option<EdgeRef>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub snippet: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub fix: Option<String>,
    pub fingerprint: String,
}

impl Finding {
    pub fn new(rule_id: &str, severity: Severity, tool: &str, file: &str, line: u32, message: String) -> Finding {
        Finding {
            rule_id: rule_id.into(),
            severity,
            message,
            tool: tool.into(),
            file: file.into(),
            line,
            node_id: None,
            edge: None,
            snippet: None,
            fix: None,
            fingerprint: String::new(),
        }
    }
    pub fn node(mut self, id: &str) -> Finding {
        self.node_id = Some(id.into());
        self
    }
    pub fn edge(mut self, from: &str, to: &str) -> Finding {
        self.edge = Some(EdgeRef { from: from.into(), to: to.into() });
        self
    }
    pub fn fix(mut self, fix: impl Into<String>) -> Finding {
        self.fix = Some(fix.into());
        self
    }
    /// sha1 of `ruleId|nodeId ?? file|edge.from|edge.to|message`. The line is deliberately excluded.
    pub fn fingerprinted(mut self) -> Finding {
        let (ef, et) = self.edge.as_ref().map(|e| (e.from.as_str(), e.to.as_str())).unwrap_or(("", ""));
        let subject = self.node_id.as_deref().unwrap_or(&self.file);
        self.fingerprint = sha1_hex(&[self.rule_id.as_str(), subject, ef, et, self.message.as_str()].join("|"));
        self
    }
}

pub fn sha1_hex(s: &str) -> String {
    sha1_bytes(s.as_bytes())
}

pub fn sha1_bytes(b: &[u8]) -> String {
    let d = Sha1::digest(b);
    d.iter().map(|x| format!("{x:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fingerprint_ignores_line() {
        let a = Finding::new("ig/arch", Severity::Error, "ig", "a.ts", 3, "m".into()).node("a.ts#f").fingerprinted();
        let b = Finding::new("ig/arch", Severity::Error, "ig", "a.ts", 30, "m".into()).node("a.ts#f").fingerprinted();
        assert_eq!(a.fingerprint, b.fingerprint);
        assert_eq!(a.fingerprint, sha1_hex("ig/arch|a.ts#f|||m"));
    }
}
