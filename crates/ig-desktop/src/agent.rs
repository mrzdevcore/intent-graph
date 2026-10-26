//! The agent from the app's side: the trust gate before every entry point, finding (or
//! installing) the ACP adapter, and the commands the chat calls. Events go out as `agent-event`.

use crate::commands::{Project, open_root_and_data};
use crate::events::AgentEvent;
use crate::session::{Session, Sink};
use crate::trust;
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tauri::async_runtime::JoinHandle;
use tauri::{AppHandle, Emitter, Manager, State};

/// The adapter, pinned (the version the ACP spike tested).
pub const ADAPTER: &str = "@agentclientprotocol/claude-agent-acp@0.95.0";
const NOT_TRUSTED: &str = "not trusted: review and trust the project first";

#[derive(Debug, PartialEq)]
pub enum AgentError {
    NotTrusted,
    Setup {
        instructions: String,
        installable: bool,
    },
}

/// The trust gate: the first step of every entry point that can start or drive a session.
pub fn gate(root: &Path, data_dir: &Path) -> Result<(), AgentError> {
    if trust::is_trusted(data_dir, root, &trust::inventory(root)) {
        Ok(())
    } else {
        Err(AgentError::NotTrusted)
    }
}

fn on_path(bin: &str) -> bool {
    std::env::var_os("PATH")
        .is_some_and(|p| std::env::split_paths(&p).any(|d| d.join(bin).is_file()))
}

/// `npm` arguments that install the adapter into the app's data dir (never globally).
pub fn install_args(data_dir: &Path) -> Vec<String> {
    let prefix = data_dir.join("agent").display().to_string();
    [
        "install",
        "--prefix",
        &prefix,
        ADAPTER,
        "--no-audit",
        "--no-fund",
    ]
    .map(String::from)
    .to_vec()
}

/// The adapter to run: `over` (`IG_AGENT_CMD`), else the one installed in the data dir; else
/// what to install.
pub fn adapter(data_dir: &Path, over: Option<PathBuf>) -> Result<PathBuf, AgentError> {
    if let Some(cmd) = over {
        return Ok(cmd);
    }
    let bin = data_dir.join("agent/node_modules/.bin/claude-agent-acp");
    let node = on_path("node");
    if node && bin.is_file() {
        return Ok(bin);
    }
    let install = format!("npm {}", install_args(data_dir).join(" "));
    Err(AgentError::Setup {
        instructions: if node {
            format!("The agent adapter is not installed. Install it with:\n  {install}")
        } else {
            format!(
                "The agent needs Node.js, which is not on PATH: install it (https://nodejs.org), then run:\n  {install}"
            )
        },
        installable: node && on_path("npm"),
    })
}

/// `IG_AGENT_CMD`, in a debug build only (the tests and the manual check use debug builds): a
/// release build ignores it, so the environment cannot swap the agent the user trusted.
fn dev_override() -> Option<PathBuf> {
    if cfg!(debug_assertions) {
        std::env::var_os("IG_AGENT_CMD").map(PathBuf::from)
    } else {
        None
    }
}

/// A Finder/Dock launch gets a bare PATH (no node, no npm): when not started from a terminal,
/// take the login shell's PATH, else put the usual install dirs first. Call before any thread.
pub fn login_path() {
    if std::env::var_os("TERM").is_some() {
        return;
    }
    let path = shell_path().unwrap_or_else(|| {
        let old = std::env::var("PATH").unwrap_or_default();
        format!("/opt/homebrew/bin:/usr/local/bin:{old}")
    });
    // SAFETY: called first thing in `main`, before any other thread exists.
    unsafe { std::env::set_var("PATH", path) }
}

fn shell_path() -> Option<String> {
    use std::process::{Command, Stdio};
    let shell = std::env::var_os("SHELL")?;
    let mut c = Command::new(shell)
        .args(["-lc", "printf %s \"$PATH\""])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let t0 = std::time::Instant::now();
    while c.try_wait().ok()?.is_none() {
        if t0.elapsed() > std::time::Duration::from_secs(3) {
            let _ = c.kill();
            return None;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    let mut out = String::new();
    std::io::Read::read_to_string(&mut c.stdout?, &mut out).ok()?;
    (!out.trim().is_empty()).then_some(out)
}

/// How to run the adapter: `over` (else the installed one) with `args`; events go to `sink`.
pub struct Launch {
    pub over: Option<PathBuf>,
    pub args: Vec<String>,
    pub sink: Sink,
}

fn launch(app: &AppHandle) -> Launch {
    let app = app.clone();
    Launch {
        over: dev_override(),
        args: vec![],
        sink: Arc::new(move |e: AgentEvent| {
            let _ = app.emit("agent-event", e);
        }),
    }
}

/// The running agent: one adapter process for the open project.
#[derive(Default)]
pub struct Agent {
    running: Mutex<Option<Running>>,
    /// Held across spawn, store and session setup: two starts never spawn two adapters.
    connecting: futures::lock::Mutex<()>,
}

struct Running {
    root: PathBuf,
    session: Arc<Session>,
    feature: Option<String>,
}

fn gate_for(root: &Path, data: &Path) -> Result<(), String> {
    gate(root, data).map_err(|_| NOT_TRUSTED.to_string())
}

impl Agent {
    /// Stops the agent (open permission requests are rejected); the task to wait on.
    pub fn stop(&self) -> Option<JoinHandle<()>> {
        let r = self.running.lock().unwrap().take();
        r.and_then(|r| r.session.stop())
    }

    /// The live session for `root` (a crashed one does not count).
    fn current(&self, root: &Path) -> Option<Arc<Session>> {
        match self.running.lock().unwrap().as_ref() {
            Some(r) if r.root == root && r.session.alive() => Some(r.session.clone()),
            _ => None,
        }
    }

    fn running(&self, root: &Path) -> Result<Arc<Session>, String> {
        self.current(root)
            .ok_or("no agent running: start a session".into())
    }

    /// The gate, then the live adapter for `root` (unless `fresh`) or a new one. `connecting` held.
    async fn connect(
        &self,
        root: &Path,
        data: &Path,
        fresh: bool,
        l: &Launch,
    ) -> Result<Arc<Session>, String> {
        gate_for(root, data)?;
        if !fresh && let Some(s) = self.current(root) {
            return Ok(s);
        }
        let cmd = match adapter(data, l.over.clone()) {
            Ok(cmd) => cmd,
            Err(AgentError::Setup {
                instructions,
                installable,
            }) => {
                (l.sink)(AgentEvent::Setup {
                    instructions: instructions.clone(),
                    installable,
                });
                return Err(instructions);
            }
            Err(AgentError::NotTrusted) => return Err(NOT_TRUSTED.into()),
        };
        // A replaced adapter (crash, restart) keeps the project's feature.
        let feature = match self.running.lock().unwrap().as_ref() {
            Some(r) if r.root == root => r.feature.clone(),
            _ => None,
        };
        self.stop();
        let args: Vec<&str> = l.args.iter().map(String::as_str).collect();
        let session = Arc::new(Session::spawn(&cmd, &args, root, l.sink.clone()).await?);
        *self.running.lock().unwrap() = Some(Running {
            root: root.to_path_buf(),
            session: session.clone(),
            feature,
        });
        Ok(session)
    }

    /// A new session for `feature`; `fresh` (a restart): in a new adapter, for the last feature.
    pub async fn start(
        &self,
        root: &Path,
        data: &Path,
        feature: Option<String>,
        fresh: bool,
        l: Launch,
    ) -> Result<String, String> {
        let _one = self.connecting.lock().await;
        let last = || self.running.lock().unwrap().as_ref()?.feature.clone();
        let feature = if fresh {
            feature.or_else(last)
        } else {
            feature
        };
        let s = self.connect(root, data, fresh, &l).await?;
        let id = s.start(feature.as_deref()).await?;
        if let Some(r) = self.running.lock().unwrap().as_mut() {
            r.feature = feature;
        }
        Ok(id)
    }

    /// An earlier session, replayed as events.
    pub async fn load(&self, root: &Path, data: &Path, id: &str, l: Launch) -> Result<(), String> {
        let _one = self.connecting.lock().await;
        self.connect(root, data, false, &l).await?.load(id).await
    }
}

/// Stops the agent: the window closed or the project changed. The task to wait on.
pub fn shutdown(app: &AppHandle) -> Option<JoinHandle<()>> {
    app.state::<Agent>().stop()
}

/// A new session, for `feature` when given. Its id.
#[tauri::command]
pub async fn agent_start(
    app: AppHandle,
    project: State<'_, Project>,
    agent: State<'_, Agent>,
    feature: Option<String>,
) -> Result<String, String> {
    let (root, data) = open_root_and_data(&project, &app)?;
    agent
        .start(&root, &data, feature, false, launch(&app))
        .await
}

/// One turn; resolves with the stop reason when it ends (updates arrive as `agent-event`).
#[tauri::command]
pub async fn agent_prompt(
    app: AppHandle,
    project: State<'_, Project>,
    agent: State<'_, Agent>,
    text: String,
    attachments: Vec<String>,
) -> Result<String, String> {
    let (root, data) = open_root_and_data(&project, &app)?;
    gate_for(&root, &data)?;
    agent.running(&root)?.prompt(&text, &attachments).await
}

/// An earlier session (from `agent_list`), replayed as `agent-event`s.
#[tauri::command]
pub async fn agent_load(
    app: AppHandle,
    project: State<'_, Project>,
    agent: State<'_, Agent>,
    session_id: String,
) -> Result<(), String> {
    let (root, data) = open_root_and_data(&project, &app)?;
    agent.load(&root, &data, &session_id, launch(&app)).await
}

/// After a `Stopped`: a new adapter and a new session for the same feature. Its id.
#[tauri::command]
pub async fn agent_restart(
    app: AppHandle,
    project: State<'_, Project>,
    agent: State<'_, Agent>,
) -> Result<String, String> {
    let (root, data) = open_root_and_data(&project, &app)?;
    gate_for(&root, &data)?;
    agent.start(&root, &data, None, true, launch(&app)).await
}

#[tauri::command]
pub async fn agent_cancel(
    app: AppHandle,
    project: State<'_, Project>,
    agent: State<'_, Agent>,
) -> Result<(), String> {
    let (root, _) = open_root_and_data(&project, &app)?;
    agent.running(&root)?.cancel()
}

/// The user's answer to a `Permission` event: one of its `optionId`s, once.
#[tauri::command]
pub async fn agent_answer(
    app: AppHandle,
    project: State<'_, Project>,
    agent: State<'_, Agent>,
    request_id: u64,
    option_id: String,
) -> Result<(), String> {
    let (root, _) = open_root_and_data(&project, &app)?;
    agent.running(&root)?.answer(request_id, &option_id)
}

/// The user's answer to a `Question`: `{field: label | [labels]}`, or null to skip it. Once.
#[tauri::command]
pub async fn agent_reply(
    app: AppHandle,
    project: State<'_, Project>,
    agent: State<'_, Agent>,
    request_id: u64,
    content: Option<Value>,
) -> Result<(), String> {
    let (root, _) = open_root_and_data(&project, &app)?;
    agent.running(&root)?.reply(request_id, content)
}

/// A permission mode that skips the asking (`bypassPermissions`): set only after a native confirm.
pub fn needs_confirm(value: &str) -> bool {
    value.to_ascii_lowercase().contains("bypass")
}

/// Sets a session `configOptions` entry (mode, model, ...); a `Config` event follows. A bypass
/// mode needs the user's yes in a native dialog, which webview script cannot click.
#[tauri::command]
pub async fn agent_set_config(
    app: AppHandle,
    project: State<'_, Project>,
    agent: State<'_, Agent>,
    config_id: String,
    value: String,
) -> Result<(), String> {
    use tauri_plugin_dialog::{DialogExt, MessageDialogButtons};
    let (root, data) = open_root_and_data(&project, &app)?;
    gate_for(&root, &data)?;
    let session = agent.running(&root)?;
    if needs_confirm(&value) {
        let a = app.clone();
        let yes = tauri::async_runtime::spawn_blocking(move || {
            a.dialog()
                .message("Bypass permissions?\n\nThe agent will run every tool and edit every file without asking.")
                .title("ig")
                .buttons(MessageDialogButtons::OkCancelCustom("Bypass".into(), "Cancel".into()))
                .blocking_show()
        })
        .await
        .map_err(|e| e.to_string())?;
        if !yes {
            return Err("not confirmed: the mode is unchanged".into());
        }
    }
    session.set_config(&config_id, &value).await
}

/// `{sessions, features}`: the adapter's sessions here and the feature → session map.
#[tauri::command]
pub async fn agent_list(
    app: AppHandle,
    project: State<'_, Project>,
    agent: State<'_, Agent>,
) -> Result<Value, String> {
    let (root, _) = open_root_and_data(&project, &app)?;
    agent.running(&root)?.list().await
}

/// Installs the pinned adapter into the app's data dir (the UI asks the user first).
#[tauri::command]
pub async fn agent_install(app: AppHandle) -> Result<(), String> {
    let data = app.path().app_data_dir().map_err(|e| e.to_string())?;
    tauri::async_runtime::spawn_blocking(move || {
        std::fs::create_dir_all(data.join("agent")).map_err(|e| e.to_string())?;
        let out = std::process::Command::new("npm")
            .args(install_args(&data))
            .output()
            .map_err(|e| format!("npm: {e}"))?;
        if out.status.success() {
            return Ok(());
        }
        let err = String::from_utf8_lossy(&out.stderr);
        let last: Vec<&str> = err.lines().rev().take(5).collect();
        Err(format!(
            "npm install failed ({}):\n{}",
            out.status,
            last.into_iter().rev().collect::<Vec<_>>().join("\n")
        ))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_bypass_mode_needs_the_confirm() {
        assert!(needs_confirm("bypassPermissions"));
        assert!(needs_confirm("BYPASS"));
        for v in ["default", "acceptEdits", "plan", "auto", "opus"] {
            assert!(!needs_confirm(v), "{v}");
        }
    }

    #[test]
    fn an_untrusted_project_never_passes_the_gate() {
        let base = std::env::temp_dir().join(format!("ig-agent-{}", std::process::id()));
        std::fs::create_dir_all(&base).unwrap();
        let data = base.join("data");
        assert_eq!(gate(&base, &data), Err(AgentError::NotTrusted));
        trust::trust(&data, &base, &trust::inventory(&base)).unwrap();
        assert_eq!(gate(&base, &data), Ok(()));
        let _ = std::fs::remove_dir_all(&base);
    }

    /// A trusted scratch project, its data dir, and a launch of the fake agent recording to `rec`.
    fn rig(tag: &str) -> (PathBuf, PathBuf, PathBuf) {
        let base = std::env::temp_dir().join(format!("ig-agent-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();
        let base = base.canonicalize().unwrap();
        let data = base.join(".data");
        trust::trust(&data, &base, &trust::inventory(&base)).unwrap();
        (base.clone(), data, base.join("rec.log"))
    }

    fn fake(rec: &Path) -> Launch {
        Launch {
            over: Some(crate::session::tests::fake_agent()),
            args: vec!["--serve".into(), rec.display().to_string()],
            sink: Arc::new(|_| {}),
        }
    }

    fn spawns(rec: &Path) -> usize {
        let log = std::fs::read_to_string(rec).unwrap_or_default();
        log.lines().filter(|l| *l == "spawn").count()
    }

    #[test]
    fn two_concurrent_starts_spawn_one_adapter() {
        let (root, data, rec) = rig("two-starts");
        let a = Agent::default();
        tauri::async_runtime::block_on(async {
            let (x, y) = futures::join!(
                a.start(&root, &data, None, false, fake(&rec)),
                a.start(&root, &data, None, false, fake(&rec))
            );
            assert_eq!((x.unwrap(), y.unwrap()), ("fake-1".into(), "fake-1".into()));
        });
        assert_eq!(spawns(&rec), 1);
        a.stop();
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_crashed_adapter_is_replaced_by_the_next_start_or_load() {
        let (root, data, rec) = rig("crash");
        let a = Agent::default();
        tauri::async_runtime::block_on(async {
            a.start(&root, &data, Some("f".into()), false, fake(&rec))
                .await
                .unwrap();
            let s = a.running(&root).unwrap();
            assert!(s.prompt("crash", &[]).await.is_err());
            for _ in 0..250 {
                if a.current(&root).is_none() {
                    break;
                }
                crate::session::nap(20).await.unwrap();
            }
            assert!(a.running(&root).is_err(), "the dead session is gone");
            a.load(&root, &data, "fake-1", fake(&rec)).await.unwrap();
            assert_eq!(spawns(&rec), 2);
            // A restart keeps the feature.
            a.start(&root, &data, None, true, fake(&rec)).await.unwrap();
            assert_eq!(spawns(&rec), 3);
            assert_eq!(
                a.running
                    .lock()
                    .unwrap()
                    .as_ref()
                    .unwrap()
                    .feature
                    .as_deref(),
                Some("f")
            );
        });
        a.stop();
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn an_untrusted_project_spawns_nothing() {
        let (root, data, rec) = rig("untrusted");
        std::fs::remove_dir_all(&data).unwrap();
        let a = Agent::default();
        let r = tauri::async_runtime::block_on(a.start(&root, &data, None, false, fake(&rec)));
        assert_eq!(r, Err(NOT_TRUSTED.into()));
        assert_eq!(spawns(&rec), 0);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn without_the_adapter_setup_shows_the_exact_install_command() {
        let data = Path::new("/nonexistent/ig-data");
        let Err(AgentError::Setup { instructions, .. }) = adapter(data, None) else {
            panic!("expected Setup")
        };
        assert!(
            instructions.contains(
                "npm install --prefix /nonexistent/ig-data/agent @agentclientprotocol/claude-agent-acp@0.95.0 --no-audit --no-fund"
            ),
            "{instructions}"
        );
        let over = PathBuf::from("/x/claude-agent-acp");
        assert_eq!(adapter(data, Some(over.clone())), Ok(over));
    }
}
