//! The agent's permission requests waiting for the user. Each gets an id of its own (concurrent
//! ones, from sub-agents too, stay apart) and is answered exactly once, with one of the adapter's
//! own option ids; when the window closes, every one still waiting is answered `reject`.
//! The agent's questions (`elicitation/create`, how the adapter asks `AskUserQuestion`) wait here
//! too, under the same ids, and are answered once with the user's picks (or declined).

use crate::events::AgentEvent;
use agent_client_protocol::schema::v1::{
    PermissionOption, PermissionOptionKind, RequestPermissionOutcome, RequestPermissionRequest,
    RequestPermissionResponse, SelectedPermissionOutcome,
};
use agent_client_protocol::{JsonRpcRequest, JsonRpcResponse, Responder};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashMap};
use std::sync::Mutex;

/// `elicitation/create`, untyped: only the form mode is shown, its schema passed on as is.
#[derive(Clone, Debug, Serialize, Deserialize, JsonRpcRequest)]
#[request(method = "elicitation/create", response = AskReply)]
pub struct Ask(pub Value);

#[derive(Clone, Debug, Serialize, Deserialize, JsonRpcResponse)]
pub struct AskReply(pub Value);

enum Answer {
    Permission(Vec<PermissionOption>, Responder<RequestPermissionResponse>),
    /// The form's fields: an answer may only fill those, with their own options when they have some.
    Question(Fields, Responder<AskReply>),
}

/// Field name → its option values (`oneOf` / `items.anyOf` consts); None: free text.
type Fields = BTreeMap<String, Option<Vec<String>>>;

/// The fields of a form schema.
fn fields_of(schema: &Value) -> Fields {
    let consts = |xs: &Value| -> Option<Vec<String>> {
        let xs = xs.as_array()?;
        Some(
            xs.iter()
                .filter_map(|o| o["const"].as_str().map(Into::into))
                .collect(),
        )
    };
    schema["properties"]
        .as_object()
        .into_iter()
        .flatten()
        .map(|(k, p)| {
            (
                k.clone(),
                consts(&p["oneOf"]).or_else(|| consts(&p["items"]["anyOf"])),
            )
        })
        .collect()
}

struct Waiting {
    session: String,
    answer: Answer,
}

#[derive(Default)]
pub struct Pending {
    // ponytail: one lock for every request; a handful wait at a time.
    inner: Mutex<(u64, HashMap<u64, Waiting>)>,
}

fn reply(r: Responder<RequestPermissionResponse>, option: Option<&PermissionOption>) {
    let outcome = match option {
        Some(o) => {
            RequestPermissionOutcome::Selected(SelectedPermissionOutcome::new(o.option_id.clone()))
        }
        None => RequestPermissionOutcome::Cancelled,
    };
    // The agent may be gone already: nothing is left to answer then.
    let _ = r.respond(RequestPermissionResponse::new(outcome));
}

fn ask_reply(r: Responder<AskReply>, v: Value) {
    let _ = r.respond(AskReply(v));
}

/// A question's answer: `{field: "label" | ["label", ...]}` over the form's own fields only, each
/// label one of that field's options (a free-text field, like `question_<n>_custom`, takes any).
fn valid(fields: &Fields, content: &Value) -> bool {
    let ok = |allowed: &Option<Vec<String>>, v: &Value| {
        v.as_str()
            .is_some_and(|s| allowed.as_ref().is_none_or(|a| a.iter().any(|x| x == s)))
    };
    content.as_object().is_some_and(|m| {
        m.iter().all(|(k, v)| {
            fields.get(k).is_some_and(|allowed| {
                ok(allowed, v)
                    || v.as_array()
                        .is_some_and(|a| a.iter().all(|x| ok(allowed, x)))
            })
        })
    })
}

impl Pending {
    fn keep(&self, session: &str, answer: Answer) -> u64 {
        let mut g = self.inner.lock().unwrap();
        g.0 += 1;
        let id = g.0;
        let session = session.to_string();
        g.1.insert(id, Waiting { session, answer });
        id
    }

    /// Keeps the request; the event to show it.
    pub fn add(
        &self,
        req: RequestPermissionRequest,
        responder: Responder<RequestPermissionResponse>,
    ) -> AgentEvent {
        let session_id = req.session_id.0.to_string();
        let tool_call = serde_json::to_value(&req.tool_call).unwrap_or_default();
        let options = serde_json::to_value(&req.options).unwrap_or_default();
        let request_id = self.keep(&session_id, Answer::Permission(req.options, responder));
        AgentEvent::Permission {
            request_id,
            session_id,
            tool_call,
            options,
        }
    }

    /// Keeps a form question; the event to show it. Any other mode is declined at once.
    pub fn ask(&self, q: Value, responder: Responder<AskReply>) -> Option<AgentEvent> {
        let schema = q["requestedSchema"].clone();
        let fields = fields_of(&schema);
        let session_id = q["sessionId"].as_str().unwrap_or("").to_string();
        if q["mode"] != "form" || fields.is_empty() {
            ask_reply(responder, json!({"action": "decline"}));
            return None;
        }
        let request_id = self.keep(&session_id, Answer::Question(fields, responder));
        Some(AgentEvent::Question {
            request_id,
            session_id,
            message: q["message"].as_str().unwrap_or("").into(),
            schema,
        })
    }

    fn take(
        &self,
        request_id: u64,
        ok: impl FnOnce(&Answer) -> Result<(), String>,
    ) -> Result<Answer, String> {
        let mut g = self.inner.lock().unwrap();
        let w =
            g.1.get(&request_id)
                .ok_or(format!("request {request_id}: already answered or unknown"))?;
        ok(&w.answer)?;
        Ok(g.1.remove(&request_id).unwrap().answer)
    }

    /// The user's answer: one of the request's own option ids, once.
    pub fn answer(&self, request_id: u64, option_id: &str) -> Result<(), String> {
        let mut at = 0;
        let a = self.take(request_id, |a| match a {
            Answer::Permission(options, _) => {
                at = options
                    .iter()
                    .position(|o| &*o.option_id.0 == option_id)
                    .ok_or(format!(
                        "permission request {request_id}: no option {option_id}"
                    ))?;
                Ok(())
            }
            Answer::Question(..) => Err(format!("request {request_id} is a question")),
        })?;
        if let Answer::Permission(options, r) = a {
            reply(r, Some(&options[at]));
        }
        Ok(())
    }

    /// The user's answer to a question, once: `Some(content)` accepts, `None` declines (skips).
    pub fn reply(&self, request_id: u64, content: Option<Value>) -> Result<(), String> {
        let a = self.take(request_id, |a| match (a, &content) {
            (Answer::Question(fields, _), Some(c)) if !valid(fields, c) => Err(format!(
                "question {request_id}: not an answer to its fields"
            )),
            (Answer::Question(..), _) => Ok(()),
            (Answer::Permission(..), _) => Err(format!("request {request_id} is a permission")),
        })?;
        if let Answer::Question(_, r) = a {
            ask_reply(
                r,
                match content {
                    Some(c) => json!({"action": "accept", "content": c}),
                    None => json!({"action": "decline"}),
                },
            );
        }
        Ok(())
    }

    /// Answers every waiting request with the adapter's reject option (cancelled if it has none);
    /// questions are cancelled.
    pub fn reject_all(&self) {
        let all: Vec<_> = self.inner.lock().unwrap().1.drain().collect();
        for (_, w) in all {
            match w.answer {
                Answer::Permission(options, r) => {
                    let reject = options
                        .iter()
                        .find(|o| o.kind == PermissionOptionKind::RejectOnce)
                        .or_else(|| {
                            options
                                .iter()
                                .find(|o| o.kind == PermissionOptionKind::RejectAlways)
                        });
                    reply(r, reject);
                }
                Answer::Question(_, r) => ask_reply(r, json!({"action": "cancel"})),
            }
        }
    }

    /// A cancelled turn: its requests are answered `cancelled`, as ACP asks.
    pub fn cancel(&self, session: &str) {
        let mut g = self.inner.lock().unwrap();
        let ids: Vec<u64> =
            g.1.iter()
                .filter(|(_, w)| w.session == session)
                .map(|(id, _)| *id)
                .collect();
        let gone: Vec<_> = ids.iter().filter_map(|id| g.1.remove(id)).collect();
        drop(g);
        for w in gone {
            match w.answer {
                Answer::Permission(_, r) => reply(r, None),
                Answer::Question(_, r) => ask_reply(r, json!({"action": "cancel"})),
            }
        }
    }

    /// The agent is gone: no answer can reach it.
    pub fn clear(&self) {
        self.inner.lock().unwrap().1.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_answer_must_pick_the_fields_own_options() {
        let f = fields_of(&json!({"properties": {
            "question_0": {"type": "string", "oneOf": [{"const": "Small"}, {"const": "Large"}]},
            "question_1": {"type": "array", "items": {"anyOf": [{"const": "A"}, {"const": "B"}]}},
            "question_0_custom": {"type": "string"}}}));
        assert!(valid(
            &f,
            &json!({"question_0": "Small", "question_1": ["A", "B"]})
        ));
        assert!(valid(&f, &json!({"question_0_custom": "anything at all"})));
        assert!(!valid(&f, &json!({"question_0": "Huge"})), "not an option");
        assert!(
            !valid(&f, &json!({"question_1": ["A", "C"]})),
            "not an option"
        );
        assert!(!valid(&f, &json!({"question_9": "Small"})), "not a field");
        assert!(!valid(&f, &json!({"question_0": 3})), "not a label");
        assert!(!valid(&f, &json!("Small")), "not an object");
    }
}
