//! `ig mcp`: hand-rolled MCP server (JSON-RPC 2.0 over stdio, one message per line).

use crate::model::{Graph, TOOL_VERSION};
use ig_wire::Wired;
use serde_json::{Value, json};
use std::io::{BufRead, Write};
use std::path::Path;

fn tools() -> Value {
    json!([
        {
            "name": "ig_state",
            "description": "What the ig viewer is showing: the last answer written with ig_show (title, nodes, edges, steps), or none.",
            "inputSchema": {"type": "object", "properties": {}}
        },
        {
            "name": "ig_show",
            "description": "Record an answer for the ig viewer: the nodes and edges of a path or subgraph, highlighted, with ordered steps. Every id is checked against the graph. Writes only .ig/show.json; the system map does not draw it yet (shown in a later version).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "title": {"type": "string", "description": "What is shown, like 'How the export reaches the database'"},
                    "nodes": {"type": "array", "items": {"type": "string"}, "maxItems": 60, "description": "Node ids; the first starts a chain"},
                    "edges": {"type": "array", "items": {"anyOf": [
                        {"type": "array", "items": {"type": "string"}, "minItems": 2, "maxItems": 2},
                        {"type": "string", "description": "from|kind|to"}]}},
                    "steps": {"type": "array", "maxItems": 40, "items": {"type": "object", "properties": {
                        "target": {"type": "string", "description": "A node id, or from|to for an edge"},
                        "text": {"type": "string"}}, "required": ["target", "text"]}},
                    "mode": {"type": "string", "enum": ["focus", "chain"]}
                },
                "required": ["title", "nodes"]
            }
        }
    ])
}

pub fn serve(root: &Path) -> anyhow::Result<()> {
    let mut out = std::io::stdout();
    for line in std::io::stdin().lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let reply = match serde_json::from_str::<Value>(&line) {
            Ok(msg) => handle(root, &msg),
            Err(e) => Some(
                json!({"jsonrpc": "2.0", "id": null, "error": {"code": -32700, "message": e.to_string()}}),
            ),
        };
        if let Some(r) = reply {
            writeln!(out, "{r}").and_then(|_| out.flush())?;
        }
    }
    Ok(())
}

fn handle(root: &Path, msg: &Value) -> Option<Value> {
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
    let w = Wired::new(root);
    let env = w.env();
    let cfg = ig_app::config::load(env.files)?;
    Ok(ig_app::graph(&env, env.files, &cfg, None)?.graph)
}

fn call(root: &Path, name: &str, args: &Value) -> anyhow::Result<(String, Option<Value>)> {
    match name {
        "ig_state" => {
            let show = std::fs::read_to_string(root.join(ig_app::show::SHOW))
                .ok()
                .and_then(|t| serde_json::from_str::<Value>(&t).ok());
            let s = json!({ "show": show });
            Ok((serde_json::to_string_pretty(&s)?, Some(s)))
        }
        "ig_show" => {
            let path = root.join(ig_app::show::SHOW);
            let seq = std::fs::read_to_string(&path)
                .ok()
                .and_then(|t| serde_json::from_str::<Value>(&t).ok())
                .and_then(|v| v["seq"].as_u64())
                .unwrap_or(0);
            let show = ig_app::show::validate(&head_graph(root)?, args, seq + 1)?;
            std::fs::create_dir_all(root.join(".ig"))?;
            ig_fs::write_atomic(&path, ig_app::show::canonical(&show).as_bytes())?;
            let text = if std::env::var("IG_APP").as_deref() == Ok("1") {
                "shown in app"
            } else {
                "no viewer is open; answer with a diagram too"
            };
            Ok((text.into(), None))
        }
        _ => anyhow::bail!("unknown tool {name}"),
    }
}

#[cfg(test)]
mod tests;
