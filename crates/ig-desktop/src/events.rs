//! What the webview hears from the agent, as `agent-event`: each ACP `session/update` mapped to a
//! typed event, plus the app's own (permission requests, auth, a stop, setup).

use serde::Serialize;
use serde_json::Value;

/// One slash command the session offers.
#[derive(Serialize, Debug, Clone, PartialEq)]
pub struct Command {
    pub name: String,
    pub description: String,
}

#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(tag = "type", rename_all_fields = "camelCase")]
pub enum AgentEvent {
    /// Streamed text. `role`: "agent", "thought", or "user" (a loaded session replays the user's turns).
    MessageChunk {
        session_id: String,
        role: &'static str,
        content: Value,
    },
    /// A tool call starts, as the adapter sent it (`toolCallId`, `title`, `kind`, `status`, `content`).
    ToolCall {
        session_id: String,
        call: Value,
    },
    /// A change to a tool call: status, content (`{type: "diff", path, oldText, newText}`), title.
    ToolCallUpdate {
        session_id: String,
        update: Value,
    },
    /// Context use, the cost so far and the rate-limit state, when the adapter reports them.
    Usage {
        session_id: String,
        used: Option<u64>,
        size: Option<u64>,
        cost: Option<Value>,
        rate_limit: Option<Value>,
    },
    /// The session's title.
    Info {
        session_id: String,
        title: Option<String>,
    },
    Commands {
        session_id: String,
        commands: Vec<Command>,
    },
    Plan {
        session_id: String,
        entries: Value,
    },
    /// The agent asks before a tool runs: answer once with one of `options`' `optionId`s.
    Permission {
        request_id: u64,
        session_id: String,
        tool_call: Value,
        options: Value,
    },
    /// `needed`: not logged in; the user runs `claude` and `/login` in a terminal. Never the account.
    Auth {
        needed: bool,
        label: Option<String>,
    },
    /// The adapter exited: `stderr` is its last few lines. The transcript stays; offer a restart.
    Stopped {
        reason: String,
        stderr: Option<String>,
    },
    /// No node or no adapter: what to run (`installable`: `agent_install` can do it).
    Setup {
        instructions: String,
        installable: bool,
    },
}

/// The event for one `session/update` notification's params; None for updates the app ignores.
pub fn from_update(params: &Value) -> Option<AgentEvent> {
    use AgentEvent::*;
    let session_id = params["sessionId"].as_str()?.to_string();
    let u = &params["update"];
    let field = |k: &str| u.get(k).cloned().unwrap_or(Value::Null);
    let chunk = |role| MessageChunk {
        session_id: session_id.clone(),
        role,
        content: field("content"),
    };
    Some(match u["sessionUpdate"].as_str()? {
        "agent_message_chunk" => chunk("agent"),
        "agent_thought_chunk" => chunk("thought"),
        "user_message_chunk" => chunk("user"),
        "tool_call" => ToolCall {
            session_id,
            call: u.clone(),
        },
        "tool_call_update" => ToolCallUpdate {
            session_id,
            update: u.clone(),
        },
        "usage_update" => Usage {
            session_id,
            used: u["used"].as_u64(),
            size: u["size"].as_u64(),
            cost: u.get("cost").cloned(),
            rate_limit: u["_meta"].get("_claude/rateLimit").cloned(),
        },
        "session_info_update" => Info {
            session_id,
            title: u["title"].as_str().map(Into::into),
        },
        "available_commands_update" => Commands {
            session_id,
            commands: u["availableCommands"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|c| {
                    Some(Command {
                        name: c["name"].as_str()?.into(),
                        description: c["description"].as_str().unwrap_or("").into(),
                    })
                })
                .collect(),
        },
        "plan" => Plan {
            session_id,
            entries: field("entries"),
        },
        _ => return None,
    })
}

/// The event for `_auth/status_update`: only whether a login is needed and the plan's label.
pub fn from_auth(params: &Value) -> AgentEvent {
    let s = &params["authStatus"];
    AgentEvent::Auth {
        needed: s["kind"] == "none",
        label: s["label"].as_str().map(Into::into),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn params(line: &str) -> Value {
        serde_json::from_str::<Value>(line).unwrap()["params"].clone()
    }

    // Lines from the ACP spike's trace (adapter 0.95.0).
    #[test]
    fn spike_updates_map_to_events() {
        let e = from_update(&params(
            r#"{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"s1","update":{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"pong"},"messageId":"m"}}}"#,
        ));
        assert_eq!(
            serde_json::to_value(e).unwrap(),
            json!({"type": "MessageChunk", "sessionId": "s1", "role": "agent", "content": {"type": "text", "text": "pong"}})
        );
        let e = from_update(&params(
            r#"{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"s1","update":{"sessionUpdate":"usage_update","used":31639,"size":1000000,"cost":{"amount":0.16,"currency":"USD"},"_meta":{"_claude/rateLimit":{"status":"allowed"},"_claude/model":"m"}}}}"#,
        ));
        assert_eq!(
            e,
            Some(AgentEvent::Usage {
                session_id: "s1".into(),
                used: Some(31639),
                size: Some(1000000),
                cost: Some(json!({"amount": 0.16, "currency": "USD"})),
                rate_limit: Some(json!({"status": "allowed"})),
            })
        );
        let e = from_update(&params(
            r#"{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"s1","update":{"sessionUpdate":"available_commands_update","availableCommands":[{"name":"ig:explore","description":"Explore","input":null}]}}}"#,
        ));
        let Some(AgentEvent::Commands { commands, .. }) = e else {
            panic!("{e:?}")
        };
        assert_eq!(commands[0].name, "ig:explore");
        let diff = json!({"sessionUpdate": "tool_call_update", "toolCallId": "t", "content": [{"type": "diff", "path": "/p/hello.txt", "oldText": null, "newText": "hi\n"}]});
        let e = from_update(&json!({"sessionId": "s1", "update": diff}));
        assert_eq!(
            e,
            Some(AgentEvent::ToolCallUpdate {
                session_id: "s1".into(),
                update: diff
            })
        );
        let e = from_update(
            &json!({"sessionId": "s1", "update": {"sessionUpdate": "session_info_update", "title": "Pong"}}),
        );
        assert!(matches!(e, Some(AgentEvent::Info { title: Some(t), .. }) if t == "Pong"));
        assert_eq!(
            from_update(
                &json!({"sessionId": "s1", "update": {"sessionUpdate": "current_mode_update"}})
            ),
            None
        );
    }

    #[test]
    fn auth_never_carries_the_account() {
        let e = from_auth(&params(
            r#"{"jsonrpc":"2.0","method":"_auth/status_update","params":{"authStatus":{"kind":"account","label":"Claude Max","account":{"email":"a@b.c"}}}}"#,
        ));
        let v = serde_json::to_string(&e).unwrap();
        assert_eq!(v, r#"{"type":"Auth","needed":false,"label":"Claude Max"}"#);
        let e = from_auth(&json!({"authStatus": {"kind": "none"}}));
        assert_eq!(
            e,
            AgentEvent::Auth {
                needed: true,
                label: None
            }
        );
    }
}
