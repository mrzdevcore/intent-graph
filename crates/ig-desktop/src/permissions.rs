//! The agent's permission requests waiting for the user. Each gets an id of its own (concurrent
//! ones, from sub-agents too, stay apart) and is answered exactly once, with one of the adapter's
//! own option ids; when the window closes, every one still waiting is answered `reject`.

use crate::events::AgentEvent;
use agent_client_protocol::Responder;
use agent_client_protocol::schema::v1::{
    PermissionOption, PermissionOptionKind, RequestPermissionOutcome, RequestPermissionRequest,
    RequestPermissionResponse, SelectedPermissionOutcome,
};
use std::collections::HashMap;
use std::sync::Mutex;

struct Waiting {
    session: String,
    options: Vec<PermissionOption>,
    responder: Responder<RequestPermissionResponse>,
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

impl Pending {
    /// Keeps the request; the event to show it.
    pub fn add(
        &self,
        req: RequestPermissionRequest,
        responder: Responder<RequestPermissionResponse>,
    ) -> AgentEvent {
        let mut g = self.inner.lock().unwrap();
        g.0 += 1;
        let request_id = g.0;
        let event = AgentEvent::Permission {
            request_id,
            session_id: req.session_id.0.to_string(),
            tool_call: serde_json::to_value(&req.tool_call).unwrap_or_default(),
            options: serde_json::to_value(&req.options).unwrap_or_default(),
        };
        g.1.insert(
            request_id,
            Waiting {
                session: req.session_id.0.to_string(),
                options: req.options,
                responder,
            },
        );
        event
    }

    /// The user's answer: one of the request's own option ids, once.
    pub fn answer(&self, request_id: u64, option_id: &str) -> Result<(), String> {
        let mut g = self.inner.lock().unwrap();
        let w = g.1.get(&request_id).ok_or(format!(
            "permission request {request_id}: already answered or unknown"
        ))?;
        let Some(i) = w.options.iter().position(|o| &*o.option_id.0 == option_id) else {
            return Err(format!(
                "permission request {request_id}: no option {option_id}"
            ));
        };
        let w = g.1.remove(&request_id).unwrap();
        drop(g);
        reply(w.responder, Some(&w.options[i]));
        Ok(())
    }

    /// Answers every waiting request with the adapter's reject option (cancelled if it has none).
    pub fn reject_all(&self) {
        let all: Vec<_> = self.inner.lock().unwrap().1.drain().collect();
        for (_, w) in all {
            let reject = w
                .options
                .iter()
                .find(|o| o.kind == PermissionOptionKind::RejectOnce)
                .or_else(|| {
                    w.options
                        .iter()
                        .find(|o| o.kind == PermissionOptionKind::RejectAlways)
                });
            reply(w.responder, reject);
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
            reply(w.responder, None);
        }
    }

    /// The agent is gone: no answer can reach it.
    pub fn clear(&self) {
        self.inner.lock().unwrap().1.clear();
    }
}
