//! Graph and finding data model. JSON is camelCase; absent fields are omitted.

use crate::config::Severity;
use regex::Regex;
use serde::{Deserialize, Serialize};
use sha1::{Digest, Sha1};
use std::sync::LazyLock;

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

/// A UI component (a Vue single-file component, a React function component) as its code
/// declares it: what comes in, what it holds, what it uses, does and draws.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Anatomy {
    /// "vue", "react", or the single-file extension it was read from ("svelte").
    pub framework: String,
    pub name: String,
    pub file: String,
    pub range: Range,
    /// Props, emits, slots and models.
    pub inputs: Vec<Part>,
    /// Reactive state, derived values and watchers.
    pub state: Vec<Part>,
    /// Composables, stores, hooks and injections.
    pub uses: Vec<Part>,
    pub functions: Vec<Part>,
    /// The components it draws, by tag.
    pub renders: Vec<Part>,
    /// Its template block, for a single-file component.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub template: Option<Range>,
}

/// One declared part of a component: `kind` says how (prop, emit, ref, computed, useState…).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Part {
    pub name: String,
    pub kind: String,
    pub line: u32,
    /// Its type or argument, as written.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub detail: Option<String>,
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
    /// The call sits in a loop body or an iterating callback.
    #[serde(skip_serializing_if = "is_false", default)]
    pub in_loop: bool,
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
        Graph {
            schema_version: 1,
            tool_version: TOOL_VERSION.into(),
            commit: None,
            nodes: vec![],
            edges: vec![],
            parse_errors: vec![],
        }
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
    pub fn new(
        rule_id: &str,
        severity: Severity,
        tool: &str,
        file: &str,
        line: u32,
        message: String,
    ) -> Finding {
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
        self.edge = Some(EdgeRef {
            from: from.into(),
            to: to.into(),
        });
        self
    }
    pub fn fix(mut self, fix: impl Into<String>) -> Finding {
        self.fix = Some(fix.into());
        self
    }
    /// sha1 of `ruleId|nodeId ?? file|edge.from|edge.to|message`. The line is deliberately excluded.
    pub fn fingerprinted(mut self) -> Finding {
        let (ef, et) = self
            .edge
            .as_ref()
            .map(|e| (e.from.as_str(), e.to.as_str()))
            .unwrap_or(("", ""));
        let subject = self.node_id.as_deref().unwrap_or(&self.file);
        self.fingerprint = sha1_hex(
            &[
                self.rule_id.as_str(),
                subject,
                ef,
                et,
                self.message.as_str(),
            ]
            .join("|"),
        );
        self
    }
}

static INTENT: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"@intent\s+([A-Z]+-\d+(?:\s*,\s*[A-Z]+-\d+)*)").unwrap());

/// A node's text hashed with its whitespace runs as one space: the same code, reformatted, hashes the same.
pub fn normalized_hash(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut ws = false;
    for c in text.trim().chars() {
        if c.is_whitespace() {
            ws = true;
        } else {
            if ws {
                out.push(' ');
                ws = false;
            }
            out.push(c);
        }
    }
    sha1_hex(&out)
}

/// The `@intent ABC-1, ABC-2` tags in `text`, added to `out` once each.
pub fn intents_in(text: &str, out: &mut Vec<String>) {
    for c in INTENT.captures_iter(text) {
        for tag in c[1].split(',') {
            let tag = tag.trim().to_string();
            if !out.contains(&tag) {
                out.push(tag);
            }
        }
    }
}

pub fn sha1_hex(s: &str) -> String {
    sha1_bytes(s.as_bytes())
}

pub fn sha1_bytes(b: &[u8]) -> String {
    let d = Sha1::digest(b);
    d.iter().map(|x| format!("{x:02x}")).collect()
}

/// The innermost node (smallest line range) containing `line`; the module (index 0) otherwise.
pub fn innermost_at(nodes: &[Node], line: u32) -> &str {
    let mut best = &nodes[0];
    for n in &nodes[1..] {
        if n.range.start_line <= line && line <= n.range.end_line {
            let span = n.range.end_line - n.range.start_line;
            if span <= best.range.end_line - best.range.start_line {
                best = n;
            }
        }
    }
    &best.id
}

/// Up to 5 ids close to `id`: those containing it (shortest first), then near misses by edit
/// distance.
pub fn suggest(g: &Graph, id: &str) -> Vec<String> {
    suggest_among(g.nodes.iter().map(|n| n.id.as_str()), id)
}

pub fn suggest_among<'a>(ids: impl Iterator<Item = &'a str>, id: &str) -> Vec<String> {
    let max = (id.chars().count() / 4).max(2);
    let mut sugg: Vec<(bool, usize, &str)> = ids
        .map(|n| (!n.contains(id), edit_distance(id, n), n))
        .filter(|(far, d, _)| !*far || *d <= max)
        .collect();
    sugg.sort();
    sugg.dedup();
    sugg.into_iter()
        .take(5)
        .map(|(_, _, n)| n.to_string())
        .collect()
}

fn edit_distance(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.chars().enumerate() {
        let mut prev = row[0];
        row[0] = i + 1;
        for j in 0..b.len() {
            let cur = row[j + 1];
            row[j + 1] = (prev + (ca != b[j]) as usize).min(row[j] + 1).min(cur + 1);
            prev = cur;
        }
    }
    row[b.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fingerprint_ignores_line() {
        let a = Finding::new("ig/arch", Severity::Error, "ig", "a.ts", 3, "m".into())
            .node("a.ts#f")
            .fingerprinted();
        let b = Finding::new("ig/arch", Severity::Error, "ig", "a.ts", 30, "m".into())
            .node("a.ts#f")
            .fingerprinted();
        assert_eq!(a.fingerprint, b.fingerprint);
        assert_eq!(a.fingerprint, sha1_hex("ig/arch|a.ts#f|||m"));
    }
}
