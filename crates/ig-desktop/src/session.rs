//! One agent process (the ACP adapter over stdio, run in the project root) and its session:
//! start, prompt, cancel, list, load. Updates go to a sink as `AgentEvent`s; the adapter exiting
//! on its own sends `Stopped`. Dropping the `Session` stops the process.

use crate::events::{self, AgentEvent};
use crate::permissions::Pending;
use agent_client_protocol::schema::ProtocolVersion;
use agent_client_protocol::schema::v1::{
    CancelNotification, ContentBlock, Implementation, InitializeRequest, ListSessionsRequest,
    LoadSessionRequest, Meta, NewSessionRequest, PromptRequest, RequestPermissionRequest,
    ResourceLink, SessionId, TextContent,
};
use agent_client_protocol::{Agent, ByteStreams, Client, ConnectionTo, JsonRpcNotification};
use futures::channel::oneshot;
use futures::future::{Either, select};
use futures::{AsyncBufReadExt, StreamExt};
use ig_core::workspace;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, VecDeque};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, Mutex};

pub type Sink = Arc<dyn Fn(AgentEvent) + Send + Sync>;

/// `session/update`, untyped: `events::from_update` reads what the app shows and ignores the rest,
/// so an update kind this crate does not know yet cannot break the session.
#[derive(Clone, Debug, Serialize, Deserialize, JsonRpcNotification)]
#[notification(method = "session/update")]
struct Update(Value);

/// The adapter's login state (outside ACP proper).
#[derive(Clone, Debug, Serialize, Deserialize, JsonRpcNotification)]
#[notification(method = "_auth/status_update")]
struct AuthStatus(Value);

/// How many stderr lines a crash shows, each cut to `TAIL_CHARS`.
const TAIL_LINES: usize = 5;
const TAIL_CHARS: usize = 200;
const SESSIONS: &str = ".ig/sessions.json";

pub struct Session {
    conn: ConnectionTo<Agent>,
    root: PathBuf,
    id: Mutex<Option<String>>,
    pending: Arc<Pending>,
    /// Sent or dropped (`stop`, or the session dropped): the process is killed, no `Stopped` is sent.
    quit: Mutex<Option<oneshot::Sender<()>>>,
}

fn err(e: impl std::fmt::Display) -> String {
    format!("agent: {e}")
}

/// Every session gets the ig plugin and project + local settings only (see Global Constraints).
fn meta(root: &Path) -> Meta {
    let mut opts = json!({"settingSources": ["project", "local"]});
    let plugin = root.join(".claude/skills/ig");
    if plugin.is_dir() {
        opts["plugins"] = json!([{"type": "local", "path": plugin}]);
    }
    let v = json!({"claudeCode": {"options": opts}});
    v.as_object().cloned().unwrap_or_default()
}

/// The feature slug → session id map in `.ig/sessions.json`.
pub fn features(root: &Path) -> BTreeMap<String, String> {
    std::fs::read(root.join(SESSIONS))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

fn remember(root: &Path, feature: &str, id: &str) -> std::io::Result<()> {
    let mut m = features(root);
    m.insert(feature.into(), id.into());
    std::fs::create_dir_all(root.join(".ig"))?;
    std::fs::write(root.join(SESSIONS), serde_json::to_vec_pretty(&m)?)
}

impl Session {
    /// Runs `cmd args` in `root` and initializes ACP (protocol version 1).
    pub async fn spawn(
        cmd: &Path,
        args: &[&str],
        root: &Path,
        sink: Sink,
    ) -> Result<Session, String> {
        let mut c = std::process::Command::new(cmd);
        c.args(args).current_dir(root);
        let mut c = async_process::Command::from(c);
        c.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        let mut child = c.spawn().map_err(|e| format!("{}: {e}", cmd.display()))?;
        let (Some(stdin), Some(stdout), Some(stderr)) =
            (child.stdin.take(), child.stdout.take(), child.stderr.take())
        else {
            return Err(err("no stdio"));
        };

        // Only the last few stderr lines are kept, for a crash report.
        let tail = Arc::new(Mutex::new(VecDeque::new()));
        let t = tail.clone();
        let stderr_done = tauri::async_runtime::spawn(async move {
            let mut lines = futures::io::BufReader::new(stderr).lines();
            while let Some(Ok(l)) = lines.next().await {
                let mut t = t.lock().unwrap();
                if t.len() == TAIL_LINES {
                    t.pop_front();
                }
                t.push_back(l.chars().take(TAIL_CHARS).collect::<String>());
            }
        });

        let pending = Arc::new(Pending::default());
        let (tx, rx) = oneshot::channel();
        let (s1, s2, s3, p) = (sink.clone(), sink.clone(), sink.clone(), pending.clone());
        let conn = Client
            .builder()
            .name("ig")
            .on_receive_notification(
                async move |u: Update, _cx| {
                    if let Some(e) = events::from_update(&u.0) {
                        s1(e);
                    }
                    Ok(())
                },
                agent_client_protocol::on_receive_notification!(),
            )
            .on_receive_notification(
                async move |a: AuthStatus, _cx| {
                    s2(events::from_auth(&a.0));
                    Ok(())
                },
                agent_client_protocol::on_receive_notification!(),
            )
            .on_receive_request(
                async move |r: RequestPermissionRequest, responder, _cx| {
                    s3(p.add(r, responder));
                    Ok(())
                },
                agent_client_protocol::on_receive_request!(),
            )
            .connect_with(ByteStreams::new(stdin, stdout), async move |cx| {
                let _ = tx.send(cx.clone());
                // The connection lives on after the adapter's stdout closes: end it then.
                select(Box::pin(cx.incoming_closed()), Box::pin(cx.terminated())).await;
                Ok(())
            });

        let (quit, quit_rx) = oneshot::channel::<()>();
        let p = pending.clone();
        tauri::async_runtime::spawn(async move {
            match select(Box::pin(conn), quit_rx).await {
                Either::Left(_) => {
                    // The adapter went away by itself: report it with its last words.
                    p.clear();
                    let status = child.status().await;
                    // ponytail: a grandchild holding stderr open would stall the tail: 0.5 s cap.
                    let nap = tauri::async_runtime::spawn_blocking(|| {
                        std::thread::sleep(std::time::Duration::from_millis(500))
                    });
                    let _ = select(stderr_done, nap).await;
                    let reason = match status {
                        Ok(s) => format!("the agent stopped ({s})"),
                        Err(e) => format!("the agent stopped: {e}"),
                    };
                    let lines: Vec<String> = tail.lock().unwrap().iter().cloned().collect();
                    sink(AgentEvent::Stopped {
                        reason,
                        stderr: (!lines.is_empty()).then(|| lines.join("\n")),
                    });
                }
                Either::Right(_) => {
                    let _ = child.kill();
                    let _ = child.status().await;
                }
            }
        });

        let conn = rx.await.map_err(|_| err("the adapter did not start"))?;
        let init = InitializeRequest::new(ProtocolVersion::V1)
            .client_info(Implementation::new("ig", env!("CARGO_PKG_VERSION")));
        let init = conn.send_request(init).block_task().await.map_err(err)?;
        if init.protocol_version != ProtocolVersion::V1 {
            return Err(err(format!(
                "protocol version {:?}, not 1",
                init.protocol_version
            )));
        }
        Ok(Session {
            conn,
            root: root.to_path_buf(),
            id: Mutex::new(None),
            pending,
            quit: Mutex::new(Some(quit)),
        })
    }

    /// A new session (`session/new`); with a feature, its id is kept in `.ig/sessions.json`.
    pub async fn start(&self, feature: Option<&str>) -> Result<String, String> {
        let req = NewSessionRequest::new(&self.root).meta(meta(&self.root));
        let r = self
            .conn
            .send_request(req)
            .block_task()
            .await
            .map_err(err)?;
        let id = r.session_id.0.to_string();
        if let Some(f) = feature {
            remember(&self.root, f, &id).map_err(|e| format!("{SESSIONS}: {e}"))?;
        }
        *self.id.lock().unwrap() = Some(id.clone());
        Ok(id)
    }

    /// Resumes an earlier session (`session/load`): the adapter replays it as updates.
    pub async fn load(&self, id: &str) -> Result<(), String> {
        let req = LoadSessionRequest::new(SessionId::new(id), &self.root).meta(meta(&self.root));
        self.conn
            .send_request(req)
            .block_task()
            .await
            .map_err(err)?;
        *self.id.lock().unwrap() = Some(id.into());
        Ok(())
    }

    fn sid(&self) -> Result<String, String> {
        self.id
            .lock()
            .unwrap()
            .clone()
            .ok_or("no session: start one first".into())
    }

    /// One turn; `attachments` are project files (relative paths), sent as links. The stop reason.
    pub async fn prompt(&self, text: &str, attachments: &[String]) -> Result<String, String> {
        let sid = self.sid()?;
        let mut blocks = vec![ContentBlock::Text(TextContent::new(text))];
        for a in attachments {
            let abs = self
                .root
                .join(a)
                .canonicalize()
                .map_err(|e| format!("{a}: {e}"))?;
            if workspace::rel(&self.root, &abs).is_none() {
                return Err(format!("{a}: outside the project"));
            }
            let uri = format!("file://{}", abs.display());
            blocks.push(ContentBlock::ResourceLink(ResourceLink::new(
                a.clone(),
                uri,
            )));
        }
        let req = PromptRequest::new(SessionId::new(sid), blocks);
        let r = self
            .conn
            .send_request(req)
            .block_task()
            .await
            .map_err(err)?;
        Ok(serde_json::to_value(r.stop_reason)
            .ok()
            .and_then(|v| v.as_str().map(Into::into))
            .unwrap_or_default())
    }

    /// Stops the running turn; its open permission requests are answered `cancelled`.
    pub fn cancel(&self) -> Result<(), String> {
        let sid = self.sid()?;
        self.pending.cancel(&sid);
        self.conn
            .send_notification(CancelNotification::new(SessionId::new(sid)))
            .map_err(err)
    }

    pub fn answer(&self, request_id: u64, option_id: &str) -> Result<(), String> {
        self.pending.answer(request_id, option_id)
    }

    /// The window is closing: every open permission request is answered `reject`.
    pub fn reject_pending(&self) {
        self.pending.reject_all();
    }

    /// Stops the adapter on purpose (window closed, project switched, restart): open permission
    /// requests are rejected first, and a turn still running fails.
    pub fn stop(&self) {
        self.reject_pending();
        self.quit.lock().unwrap().take();
    }

    /// The adapter's sessions for this project (`session/list`) and the feature map.
    pub async fn list(&self) -> Result<Value, String> {
        let req = ListSessionsRequest::new().cwd(self.root.clone());
        let r = self
            .conn
            .send_request(req)
            .block_task()
            .await
            .map_err(err)?;
        let r = serde_json::to_value(r).map_err(err)?;
        Ok(json!({"sessions": r["sessions"], "features": features(&self.root)}))
    }
}

#[cfg(test)]
#[path = "session_tests.rs"]
mod tests;
