//! `ig mcp`: hand-rolled MCP server (JSON-RPC 2.0 over stdio, one message per line).

mod schema;

use crate::model::{Graph, TOOL_VERSION};
use ig_app::map::edit::Edit;
use ig_app::map::edit::journal::{self, By};
use ig_app::map::kinds::Kinds;
use ig_app::map::{origin, spec, view};
use ig_wire::Wired;
use serde_json::{Value, json};
use std::io::{BufRead, Write};
use std::path::Path;

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
        "tools/list" => {
            // a team kind file that does not read is reported by ig_plan
            let w = Wired::new(root);
            let k = Kinds::load(w.env().files).unwrap_or_else(|_| Kinds::builtin().clone());
            Ok(json!({"tools": schema::tools(&k)}))
        }
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

fn restores(e: &Edit) -> bool {
    match e {
        Edit::Restore { .. } => true,
        Edit::Batch { edits } => edits.iter().any(restores),
        _ => false,
    }
}

fn head_graph(root: &Path) -> anyhow::Result<Graph> {
    let w = Wired::new(root);
    let env = w.env();
    let cfg = ig_app::config::load(env.files)?;
    Ok(ig_app::graph(&env, env.files, &cfg, None)?.graph)
}

fn has_spec(w: &Wired) -> anyhow::Result<bool> {
    Ok(spec::exists(w.files())?)
}

fn plan_text(w: &Wired) -> anyhow::Result<(String, Value)> {
    let v = view::run(&w.env())?;
    if v.broken {
        anyhow::bail!("ig/ does not read: {}", v.notices.join("; "));
    }
    let unread = view::unread(&w.env(), &v, &ig_fs::BUILD_DIRS)?;
    let p = origin::plan_of(&v.boxes, &v.arrows, &unread);
    Ok((p.text("plan"), crate::plan::json(&p)))
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
            let w = Wired::new(root);
            let targets = match ig_wire::landscape(root) {
                Some(v) => ig_app::show::Targets::new(&Graph::empty(), Some(&v)),
                None => ig_app::show::Targets::new(&head_graph(root)?, Some(&view::run(&w.env())?)),
            };
            let show = ig_app::show::validate(&targets, args, seq + 1)?;
            std::fs::create_dir_all(root.join(".ig"))?;
            ig_fs::write_atomic(&path, ig_app::show::canonical(&show).as_bytes())?;
            let text = if std::env::var(ig_app::show::IN_APP).as_deref() == Ok("1") {
                "shown in app"
            } else {
                "no viewer is open; answer with a diagram too"
            };
            Ok((text.into(), None))
        }
        "ig_plan" => {
            let w = Wired::new(root);
            if !has_spec(&w)? {
                return Ok((spec::MISSING.into(), None));
            }
            let (text, p) = plan_text(&w)?;
            Ok((text, Some(p)))
        }
        "ig_edit" => {
            let w = Wired::new(root);
            if !has_spec(&w)? {
                anyhow::bail!(spec::MISSING);
            }
            let edits: Vec<Value> = match args.get("edits") {
                Some(Value::Array(a)) => a.clone(),
                Some(_) => anyhow::bail!("edits must be an array"),
                None => vec![args.clone()],
            };
            let mut recorded = vec![];
            for (i, raw) in edits.iter().enumerate() {
                let ns: Vec<String> = recorded.iter().map(|n: &u64| n.to_string()).collect();
                let kept = match ns.len() {
                    0 => String::new(),
                    1 => format!("; journal entry {} stays recorded", ns[0]),
                    _ => format!("; journal entries {} stay recorded", ns.join(", ")),
                };
                let e: Edit = serde_json::from_value(raw.clone())
                    .map_err(|e| anyhow::anyhow!("edit {}: {e}{kept}", i + 1))?;
                if restores(&e) {
                    anyhow::bail!("edit {}: restore is not an edit you can make{kept}", i + 1);
                }
                match journal::record(w.files(), By::Claude, &e) {
                    Ok(en) => recorded.push(en.n),
                    Err(m) => anyhow::bail!("edit {}: {}{kept}", i + 1, m.join("; ")),
                }
            }
            let (plan, mut p) = plan_text(&w)?;
            p["recorded"] = json!(recorded);
            Ok((format!("recorded {recorded:?}\n{plan}"), Some(p)))
        }
        _ => anyhow::bail!("unknown tool {name}"),
    }
}

#[cfg(test)]
mod tests;
