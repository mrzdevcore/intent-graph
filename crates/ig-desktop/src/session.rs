//! One agent process (the ACP adapter over stdio, run in the project root) and its session:
//! start, prompt, cancel, list, load. Updates go to a sink as `AgentEvent`s; the adapter exiting
//! on its own sends `Stopped`. Dropping the `Session` stops the process.

use crate::events::{self, AgentEvent};
use crate::permissions::{Ask, Pending};
use agent_client_protocol::schema::ProtocolVersion;
use agent_client_protocol::schema::v1::{
    CancelNotification, ClientCapabilities, ContentBlock, ElicitationCapabilities,
    ElicitationFormCapabilities, Implementation, InitializeRequest, ListSessionsRequest,
    LoadSessionRequest, Meta, NewSessionRequest, PromptRequest, RequestPermissionRequest,
    ResourceLink, SessionId, SetSessionConfigOptionRequest, TextContent,
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
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tauri::async_runtime::JoinHandle;

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
/// How long a stop lets the queued messages reach the adapter before the kill.
const FLUSH_MS: u64 = 300;

pub struct Session {
    conn: ConnectionTo<Agent>,
    root: PathBuf,
    plugin: Option<PathBuf>,
    id: Mutex<Option<String>>,
    pending: Arc<Pending>,
    /// Sent or dropped (`stop`, or the session dropped): the process is killed, no `Stopped` is sent.
    quit: Mutex<Option<oneshot::Sender<()>>>,
    /// The task that owns the process: done once it is reaped.
    task: Mutex<Option<JoinHandle<()>>>,
    /// False once the adapter went away by itself.
    alive: Arc<AtomicBool>,
    sink: Sink,
}

/// Ready after `ms` (a tokio timer, no thread).
pub async fn nap(ms: u64) {
    tokio::time::sleep(std::time::Duration::from_millis(ms)).await
}

/// Waits for `task`, at most `ms`.
pub async fn wait(task: JoinHandle<()>, ms: u64) {
    let _ = select(task, Box::pin(nap(ms))).await;
}

fn err(e: impl std::fmt::Display) -> String {
    format!("agent: {e}")
}

/// Every session gets the ig plugin (the app's own, see `plugin.rs`) and project + local settings only (see Global Constraints).
fn meta(plugin: Option<&Path>) -> Meta {
    let mut opts = json!({"settingSources": ["project", "local"]});
    if let Some(plugin) = plugin {
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
    /// Runs `cmd args` in `root`, with `ig` first on its PATH, and initializes ACP (protocol 1).
    pub async fn spawn(
        cmd: &Path,
        args: &[&str],
        root: &Path,
        ig: Option<&Path>,
        plugin: Option<PathBuf>,
        sink: Sink,
    ) -> Result<Session, String> {
        let mut c = std::process::Command::new(cmd);
        let path = std::env::var_os("PATH").unwrap_or_default();
        c.args(args)
            .current_dir(root)
            .env("PATH", crate::shell_path::child_path(ig, &path))
            // ig_show answers "shown in app" to the agent.
            .env("IG_APP", "1");
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
        let sink_c = sink.clone();
        let (tx, rx) = oneshot::channel();
        let (s1, s2, s3, p) = (sink.clone(), sink.clone(), sink.clone(), pending.clone());
        let (s4, p4) = (sink.clone(), pending.clone());
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
            .on_receive_request(
                async move |q: Ask, responder, _cx| {
                    if let Some(e) = p4.ask(q.0, responder) {
                        s4(e);
                    }
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
        let alive = Arc::new(AtomicBool::new(true));
        let a = alive.clone();
        let task = tauri::async_runtime::spawn(async move {
            match select(Box::pin(conn), quit_rx).await {
                Either::Left(_) => {
                    // The adapter went away by itself: report it with its last words.
                    a.store(false, Ordering::SeqCst);
                    p.clear();
                    let status = child.status().await;
                    // ponytail: a grandchild holding stderr open would stall the tail: 0.5 s cap.
                    wait(stderr_done, 500).await;
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
                Either::Right((_, conn)) => {
                    // Let the connection write what is queued (the rejects `stop` sent) first.
                    let _ = select(conn, Box::pin(nap(FLUSH_MS))).await;
                    let _ = child.kill();
                    let _ = child.status().await;
                }
            }
        });

        let conn = rx.await.map_err(|_| err("the adapter did not start"))?;
        let init = InitializeRequest::new(ProtocolVersion::V1)
            .client_info(Implementation::new("ig", env!("CARGO_PKG_VERSION")))
            // Form questions only: the adapter then turns `AskUserQuestion` on and asks through us.
            .client_capabilities(ClientCapabilities::new().elicitation(
                ElicitationCapabilities::new().form(ElicitationFormCapabilities::new()),
            ));
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
            plugin,
            id: Mutex::new(None),
            pending,
            quit: Mutex::new(Some(quit)),
            task: Mutex::new(Some(task)),
            alive,
            sink: sink_c,
        })
    }

    fn config(&self, id: &str, options: impl Serialize) {
        let options = serde_json::to_value(options).unwrap_or_default();
        if !options.is_null() {
            (self.sink)(AgentEvent::Config {
                session_id: id.into(),
                options,
            });
        }
    }

    /// A new session (`session/new`); with a feature, its id is kept in `.ig/sessions.json`.
    pub async fn start(&self, feature: Option<&str>) -> Result<String, String> {
        let req = NewSessionRequest::new(&self.root).meta(meta(self.plugin.as_deref()));
        let r = self
            .conn
            .send_request(req)
            .block_task()
            .await
            .map_err(err)?;
        let id = r.session_id.0.to_string();
        self.config(&id, r.config_options);
        if let Some(f) = feature {
            remember(&self.root, f, &id).map_err(|e| format!("{SESSIONS}: {e}"))?;
        }
        *self.id.lock().unwrap() = Some(id.clone());
        Ok(id)
    }

    /// Resumes an earlier session (`session/load`): the adapter replays it as updates.
    pub async fn load(&self, id: &str) -> Result<(), String> {
        let req = LoadSessionRequest::new(SessionId::new(id), &self.root)
            .meta(meta(self.plugin.as_deref()));
        let r = self
            .conn
            .send_request(req)
            .block_task()
            .await
            .map_err(err)?;
        *self.id.lock().unwrap() = Some(id.into());
        self.config(id, r.config_options);
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
            let uri = url::Url::from_file_path(&abs)
                .map_err(|_| format!("{a}: not a file path"))?
                .to_string();
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

    /// The answer to a `Question`: the picks by field, or `None` to skip it.
    pub fn reply(&self, request_id: u64, content: Option<Value>) -> Result<(), String> {
        self.pending.reply(request_id, content)
    }

    /// Sets one of the session's `configOptions` (mode, model, ...) to one of its values.
    pub async fn set_config(&self, config_id: &str, value: &str) -> Result<(), String> {
        let sid = self.sid()?;
        let req = SetSessionConfigOptionRequest::new(
            SessionId::new(sid.clone()),
            config_id.to_string(),
            value,
        );
        let r = self
            .conn
            .send_request(req)
            .block_task()
            .await
            .map_err(err)?;
        self.config(&sid, r.config_options);
        Ok(())
    }

    /// False once the adapter went away by itself (crashed): start a new one.
    pub fn alive(&self) -> bool {
        self.alive.load(Ordering::SeqCst)
    }

    /// Stops the adapter on purpose (window closed, project switched, restart): open permission
    /// requests are answered `reject` and reach the adapter before the kill; a running turn fails.
    /// The task to wait on (once) until the process is gone.
    pub fn stop(&self) -> Option<JoinHandle<()>> {
        self.pending.reject_all();
        self.quit.lock().unwrap().take();
        self.task.lock().unwrap().take()
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
pub(crate) mod tests;
