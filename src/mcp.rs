//! `ig mcp`: hand-rolled MCP server (JSON-RPC 2.0 over stdio, one message per line).

use crate::check;
use crate::config::Config;
use crate::graph;
use crate::intents;
use crate::model::{Graph, TOOL_VERSION};
use crate::workspace;
use serde_json::{Value, json};
use std::collections::{BTreeSet, HashSet, VecDeque};
use std::io::{BufRead, Write};
use std::path::Path;

fn tools() -> Value {
    json!([
        {
            "name": "check",
            "description": "Run ig check on the working tree against the base. Returns the blocking findings with fixes.",
            "inputSchema": {"type": "object", "properties": {"all": {"type": "boolean", "description": "Report all findings, not only those touching the change"}}}
        },
        {
            "name": "get_findings",
            "description": "The findings of the last ig check (runs a check if there is none).",
            "inputSchema": {"type": "object", "properties": {}}
        },
        {
            "name": "get_intents",
            "description": "Intents from intents.md with implemented/tested coverage.",
            "inputSchema": {"type": "object", "properties": {}}
        },
        {
            "name": "graph_query",
            "description": "The neighbourhood of a graph node (id like 'src/a.ts#fn').",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "id": {"type": "string"},
                    "direction": {"type": "string", "enum": ["in", "out", "both"]},
                    "depth": {"type": "integer", "minimum": 1, "maximum": 3}
                },
                "required": ["id"]
            }
        }
    ])
}

pub fn serve(root: &Path) -> anyhow::Result<()> {
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();
    for line in stdin.lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let resp = match serde_json::from_str::<Value>(&line) {
            Ok(msg) => handle(root, &msg),
            Err(e) => Some(
                json!({"jsonrpc": "2.0", "id": null, "error": {"code": -32700, "message": e.to_string()}}),
            ),
        };
        if let Some(r) = resp {
            writeln!(stdout, "{r}")?;
            stdout.flush()?;
        }
    }
    Ok(())
}

pub fn handle(root: &Path, msg: &Value) -> Option<Value> {
    let id = msg.get("id")?.clone();
    let method = msg.get("method").and_then(Value::as_str).unwrap_or("");
    let params = msg.get("params").cloned().unwrap_or(json!({}));
    let result = match method {
        "initialize" => Ok(json!({
            "protocolVersion": params.get("protocolVersion").cloned().unwrap_or(json!("2025-06-18")),
            "capabilities": {"tools": {}},
            "serverInfo": {"name": "ig", "version": TOOL_VERSION}
        })),
        "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({"tools": tools()})),
        "tools/call" => {
            let name = params.get("name").and_then(Value::as_str).unwrap_or("");
            let args = params.get("arguments").cloned().unwrap_or(json!({}));
            Ok(match call(root, name, &args) {
                Ok((text, structured)) => {
                    let mut r = json!({"content": [{"type": "text", "text": text}]});
                    if let Some(s) = structured {
                        r["structuredContent"] = s;
                    }
                    r
                }
                Err(e) => {
                    json!({"content": [{"type": "text", "text": format!("ig: {e:#}")}], "isError": true})
                }
            })
        }
        _ => Err((-32601, format!("method not found: {method}"))),
    };
    Some(match result {
        Ok(r) => json!({"jsonrpc": "2.0", "id": id, "result": r}),
        Err((code, m)) => {
            json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": m}})
        }
    })
}

fn head_graph(root: &Path) -> anyhow::Result<Graph> {
    let cfg = Config::load(root)?;
    Ok(graph::build(root, &cfg, None)?.graph)
}

fn call(root: &Path, name: &str, args: &Value) -> anyhow::Result<(String, Option<Value>)> {
    match name {
        "check" => {
            let all = args.get("all").and_then(Value::as_bool).unwrap_or(false);
            let run = check::run(
                root,
                &check::Opts {
                    all,
                    ..Default::default()
                },
            )?;
            let text = check::agent_text(&run.result, run.config.block_on);
            Ok((
                text,
                Some(json!({"blocking": run.result.blocking, "findings": run.result.findings})),
            ))
        }
        "get_findings" => {
            let v: Value = match std::fs::read_to_string(root.join(".ig/findings.json"))
                .ok()
                .and_then(|t| serde_json::from_str(&t).ok())
            {
                Some(v) => v,
                None => {
                    let run = check::run(root, &check::Opts::default())?;
                    json!({"blocking": run.result.blocking, "findings": run.result.findings})
                }
            };
            let s = json!({"blocking": v["blocking"], "findings": v["findings"]});
            Ok((serde_json::to_string_pretty(&s)?, Some(s)))
        }
        "get_intents" => {
            let cfg = Config::load(root)?;
            let g = graph::build(root, &cfg, None)?.graph;
            let tests = workspace::globset(&cfg.tests)?;
            let declared = intents::parse(
                &std::fs::read_to_string(root.join("intents.md")).unwrap_or_default(),
            );
            let list: Vec<Value> = declared
                .iter()
                .map(|i| {
                    let tagged = g.nodes.iter().filter(|n| n.intents.contains(&i.id));
                    let (mut implemented, mut tested) = (vec![], vec![]);
                    for n in tagged {
                        if n.kind == "test" || tests.is_match(&n.file) { tested.push(&n.id) } else { implemented.push(&n.id) }
                    }
                    json!({"id": i.id, "text": i.text, "implemented": !implemented.is_empty(), "tested": !tested.is_empty(),
                           "implementedBy": implemented, "testedBy": tested})
                })
                .collect();
            let s = json!({"intents": list});
            Ok((serde_json::to_string_pretty(&s)?, Some(s)))
        }
        "graph_query" => {
            let id = args
                .get("id")
                .and_then(Value::as_str)
                .ok_or_else(|| anyhow::anyhow!("graph_query: id is required"))?;
            let dir = args
                .get("direction")
                .and_then(Value::as_str)
                .unwrap_or("both");
            let depth = args
                .get("depth")
                .and_then(Value::as_u64)
                .unwrap_or(1)
                .clamp(1, 3) as usize;
            let g = match std::fs::read_to_string(root.join(".ig/graph.json"))
                .ok()
                .and_then(|t| serde_json::from_str::<Graph>(&t).ok())
            {
                Some(g) => g,
                None => head_graph(root)?,
            };
            let s = query(&g, id, dir, depth);
            Ok((serde_json::to_string_pretty(&s)?, Some(s)))
        }
        _ => anyhow::bail!("unknown tool {name}"),
    }
}

pub fn query(g: &Graph, id: &str, dir: &str, depth: usize) -> Value {
    if !g.nodes.iter().any(|n| n.id == id) {
        return json!({"error": format!("unknown node {id}"), "suggestions": suggest(g, id)});
    }
    let mut seen: BTreeSet<&str> = BTreeSet::from([id]);
    let mut edges = HashSet::new();
    let mut q = VecDeque::from([(id, 0usize)]);
    while let Some((cur, d)) = q.pop_front() {
        if d == depth {
            continue;
        }
        for (i, e) in g.edges.iter().enumerate() {
            let next = if (dir == "out" || dir == "both") && e.from == cur {
                Some(e.to.as_str())
            } else if (dir == "in" || dir == "both") && e.to == cur {
                Some(e.from.as_str())
            } else {
                None
            };
            if let Some(n) = next {
                edges.insert(i);
                if seen.insert(n) {
                    q.push_back((n, d + 1));
                }
            }
        }
    }
    let nodes: Vec<Value> = g
        .nodes
        .iter()
        .filter(|n| seen.contains(n.id.as_str()))
        .map(|n| json!({"id": n.id, "kind": n.kind, "file": n.file, "line": n.range.start_line, "intents": n.intents}))
        .collect();
    let mut es: Vec<usize> = edges.into_iter().collect();
    es.sort();
    let edges: Vec<Value> = es
        .iter()
        .map(|i| {
            let e = &g.edges[*i];
            json!({"from": e.from, "to": e.to, "kind": e.kind, "confidence": e.confidence})
        })
        .collect();
    json!({"id": id, "nodes": nodes, "edges": edges})
}

/// Up to 5 ids close to `id`: those containing it (shortest first), then near misses by edit
/// distance.
pub fn suggest(g: &Graph, id: &str) -> Vec<String> {
    let max = (id.chars().count() / 4).max(2);
    let mut sugg: Vec<(bool, usize, &str)> = g
        .nodes
        .iter()
        .map(|n| n.id.as_str())
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
    fn protocol_basics() {
        let root = Path::new(".");
        let r = handle(
            root,
            &json!({"jsonrpc": "2.0", "id": 1, "method": "tools/list"}),
        )
        .unwrap();
        assert_eq!(r["result"]["tools"].as_array().unwrap().len(), 4);
        assert!(
            handle(
                root,
                &json!({"jsonrpc": "2.0", "method": "notifications/initialized"})
            )
            .is_none()
        );
        let r = handle(root, &json!({"jsonrpc": "2.0", "id": 2, "method": "nope"})).unwrap();
        assert_eq!(r["error"]["code"], -32601);
    }

    #[test]
    fn query_suggests() {
        let mut g = Graph::empty();
        let n = |id: &str| crate::model::Node {
            id: id.into(),
            kind: "function".into(),
            name: id.into(),
            file: "a.ts".into(),
            range: crate::model::Range {
                start_line: 1,
                end_line: 1,
            },
            parent: None,
            package: None,
            exported: false,
            hash: String::new(),
            metrics: crate::model::Metrics {
                loc: 1,
                complexity: 1,
            },
            intents: vec![],
        };
        g.nodes = vec![n("a.ts#foobar"), n("a.ts#foo")];
        let v = query(&g, "foo", "both", 1);
        assert_eq!(v["suggestions"][0], "a.ts#foo");
    }
}
