//! A scripted ACP agent for ig-desktop's tests (a harness-less test target: `cargo test` links it,
//! no app build ships it; it serves only with `--serve`). It speaks
//! the real adapter's message shapes (ACP spike, adapter 0.95.0); each prompt text picks a script:
//! `stream`, `perm` (one permission request), `two` (two at once), `hang` (until cancelled),
//! `crash` (exits mid-turn), `auth` (not logged in), `meta` (echoes session/new's `_meta`),
//! `ask` (an `AskUserQuestion` form, the answer echoed), `tour` (commands, title, usage, text, a
//! permission and a question: what the chat's recorded fixture comes from).
//! `--serve <file>` also appends what happened to `<file>`: `spawn`, then `answer <optionId>`s.

use serde_json::{Value, json};
use std::io::{BufRead, Write};

fn record(line: &str) {
    if let Some(f) = std::env::args().nth(2) {
        let mut f = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(f)
            .unwrap();
        writeln!(f, "{line}").unwrap();
    }
}

struct Agent {
    lines: std::io::Lines<std::io::StdinLock<'static>>,
    next_id: u64,
    meta: Value,
}

fn send(v: Value) {
    let mut out = std::io::stdout().lock();
    writeln!(out, "{v}").unwrap();
    out.flush().unwrap();
}

fn update(sid: &str, u: Value) {
    send(
        json!({"jsonrpc": "2.0", "method": "session/update", "params": {"sessionId": sid, "update": u}}),
    );
}

fn chunk(sid: &str, text: &str) {
    update(
        sid,
        json!({"sessionUpdate": "agent_message_chunk", "content": {"type": "text", "text": text}}),
    );
}

/// The adapter's `configOptions` (spike, trimmed): mode and model.
fn config(mode: &str) -> Value {
    json!([{"id": "mode", "name": "Mode", "category": "mode", "type": "select", "currentValue": mode,
        "options": [{"value": "default", "name": "Manual"}, {"value": "acceptEdits", "name": "Accept edits"},
            {"value": "plan", "name": "Plan"}]},
        {"id": "model", "name": "Model", "category": "model", "type": "select", "currentValue": "default",
        "options": [{"value": "default", "name": "Default (recommended)"}, {"value": "sonnet", "name": "Sonnet"}]}])
}

fn tool_call(sid: &str, id: &str) -> Value {
    let call = json!({"toolCallId": id, "status": "pending", "title": format!("Write {id}.txt"), "kind": "edit",
        "locations": [{"path": format!("/p/{id}.txt")}],
        "content": [{"type": "diff", "path": format!("/p/{id}.txt"), "oldText": null, "newText": "hi\n"}]});
    let mut u = call.clone();
    u["sessionUpdate"] = json!("tool_call");
    update(sid, u);
    call
}

impl Agent {
    fn read(&mut self) -> Value {
        match self.lines.next() {
            Some(Ok(l)) => serde_json::from_str(&l).unwrap(),
            _ => std::process::exit(0),
        }
    }

    fn ask(&mut self, sid: &str, call: Value) -> u64 {
        self.next_id += 1;
        let options = json!([{"optionId": "allow-once", "name": "Yes", "kind": "allow_once"},
            {"optionId": "allow-with-updates", "name": "Yes, allow all edits during this session", "kind": "allow_always"},
            {"optionId": "reject", "name": "No", "kind": "reject_once"}]);
        send(
            json!({"jsonrpc": "2.0", "id": self.next_id, "method": "session/request_permission",
            "params": {"sessionId": sid, "toolCall": call, "options": options}}),
        );
        self.next_id
    }

    /// Waits for the answers to `ids`, in any order; each tool call then ends as the real adapter
    /// ends it (`failed` on reject), with the answer in `rawOutput`.
    fn answers(&mut self, sid: &str, ids: &[(u64, &str)]) {
        let mut left = ids.len();
        while left > 0 {
            let m = self.read();
            let Some((_, call)) = ids.iter().find(|(id, _)| m["id"] == *id) else {
                continue;
            };
            left -= 1;
            let outcome = &m["result"]["outcome"];
            let answer = outcome["optionId"]
                .as_str()
                .unwrap_or("cancelled")
                .to_string();
            record(&format!("answer {answer}"));
            let status = if answer.starts_with("allow") {
                "completed"
            } else {
                "failed"
            };
            update(
                sid,
                json!({"sessionUpdate": "tool_call_update", "toolCallId": call, "status": status, "rawOutput": answer}),
            );
        }
    }

    /// An `AskUserQuestion` as the adapter renders it (elicitation.js); the reply echoed as text.
    fn question(&mut self, sid: &str) {
        self.next_id += 1;
        let id = self.next_id;
        let field = json!({"type": "string", "title": "Scope", "oneOf": [
            {"const": "Small", "title": "Small", "description": "One module"},
            {"const": "Large", "title": "Large"}]});
        send(
            json!({"jsonrpc": "2.0", "id": id, "method": "elicitation/create", "params": {"mode": "form",
            "sessionId": sid, "toolCallId": "q", "message": "How big?", "requestedSchema": {"type": "object",
            "properties": {"question_0": field, "question_0_custom": {"type": "string", "title": "Other"}}}}}),
        );
        loop {
            let m = self.read();
            if m["id"] == id {
                record(&format!("reply {}", m["result"]));
                chunk(sid, &format!("answer: {}", m["result"]));
                return;
            }
        }
    }

    fn prompt(&mut self, sid: &str, text: &str) -> &'static str {
        match text {
            "ask" => self.question(sid),
            "tour" => {
                update(
                    sid,
                    json!({"sessionUpdate": "available_commands_update", "availableCommands": [
                    {"name": "compact", "description": "Compact the context", "input": null},
                    {"name": "ig:plan", "description": "Plan a change", "input": {"hint": "what"}},
                    {"name": "ig:explore", "description": "Explore the repo", "input": null}]}),
                );
                update(
                    sid,
                    json!({"sessionUpdate": "session_info_update", "title": "Tour"}),
                );
                for t in ["Here is **the", " plan**:\n\n- one", "\n- two\n"] {
                    chunk(sid, t);
                }
                let call = tool_call(sid, "a");
                let id = self.ask(sid, call);
                self.answers(sid, &[(id, "a")]);
                self.question(sid);
                update(
                    sid,
                    json!({"sessionUpdate": "usage_update", "used": 31639, "size": 1000000,
                    "cost": {"amount": 0.16, "currency": "USD"},
                    "_meta": {"_claude/rateLimit": {"status": "allowed", "utilization": 0.42}}}),
                );
            }
            "stream" => {
                for t in ["Hel", "lo", " world"] {
                    chunk(sid, t);
                }
            }
            "perm" => {
                let call = tool_call(sid, "a");
                let id = self.ask(sid, call);
                self.answers(sid, &[(id, "a")]);
                chunk(sid, "done");
            }
            "two" => {
                let (a, b) = (tool_call(sid, "a"), tool_call(sid, "b"));
                let ia = self.ask(sid, a);
                let ib = self.ask(sid, b);
                self.answers(sid, &[(ia, "a"), (ib, "b")]);
            }
            "hang" => {
                chunk(sid, "thinking");
                loop {
                    let m = self.read();
                    if m["method"] == "session/cancel" {
                        return "cancelled";
                    }
                }
            }
            "crash" => {
                chunk(sid, "about to fall over");
                eprintln!("fake agent: boom");
                std::process::exit(3);
            }
            "auth" => send(
                json!({"jsonrpc": "2.0", "method": "_auth/status_update", "params": {"authStatus": {"kind": "none", "account": "secret"}}}),
            ),
            "meta" => chunk(sid, &self.meta.to_string()),
            other => chunk(sid, &format!("unknown script {other}")),
        }
        "end_turn"
    }
}

fn main() {
    if std::env::args().nth(1).as_deref() != Some("--serve") {
        return;
    }
    record("spawn");
    let mut a = Agent {
        lines: std::io::stdin().lock().lines(),
        next_id: 0,
        meta: Value::Null,
    };
    loop {
        let m = a.read();
        let (id, p) = (m["id"].clone(), &m["params"]);
        let result = match m["method"].as_str() {
            Some("initialize") => {
                json!({"protocolVersion": 1, "agentCapabilities": {"loadSession": true},
                "agentInfo": {"name": "fake", "version": "0"}, "authMethods": []})
            }
            Some("session/new") => {
                a.meta = p["_meta"].clone();
                json!({"sessionId": "fake-1", "configOptions": config("default")})
            }
            Some("session/load") => {
                let sid = p["sessionId"].as_str().unwrap().to_string();
                update(
                    &sid,
                    json!({"sessionUpdate": "user_message_chunk", "content": {"type": "text", "text": "earlier"}}),
                );
                json!({})
            }
            Some("session/set_config_option") => {
                record(&format!("config {} {}", p["configId"], p["value"]));
                json!({"configOptions": config(p["value"].as_str().unwrap_or(""))})
            }
            Some("session/list") => {
                json!({"sessions": [{"sessionId": "fake-1", "cwd": p["cwd"], "title": "Fake"}]})
            }
            Some("session/prompt") => {
                let sid = p["sessionId"].as_str().unwrap().to_string();
                let text = p["prompt"][0]["text"].as_str().unwrap_or("").to_string();
                json!({"stopReason": a.prompt(&sid, &text)})
            }
            _ => continue,
        };
        send(json!({"jsonrpc": "2.0", "id": id, "result": result}));
    }
}
