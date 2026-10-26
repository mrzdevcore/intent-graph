//! The agent from the app's side: the trust gate before every entry point, finding (or
//! installing) the ACP adapter, and the commands the chat calls. Events go out as `agent-event`.

use crate::commands::{Project, open_root_and_data};
use crate::events::AgentEvent;
use crate::session::{Session, Sink};
use crate::trust;
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
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

/// The running agent: one adapter process for the open project.
#[derive(Default)]
pub struct Agent(Mutex<Option<Running>>);

struct Running {
    root: PathBuf,
    session: Arc<Session>,
    feature: Option<String>,
}

/// Stops the agent, rejecting its open permission requests: the window closed or the project changed.
pub fn shutdown(app: &AppHandle) {
    if let Some(r) = app.state::<Agent>().0.lock().unwrap().take() {
        r.session.stop();
    }
}

fn sink(app: &AppHandle) -> Sink {
    let app = app.clone();
    Arc::new(move |e: AgentEvent| {
        let _ = app.emit("agent-event", e);
    })
}

/// The gate, then the running adapter for `root` (unless `fresh`) or a new one.
async fn connect(
    app: &AppHandle,
    root: &Path,
    data: &Path,
    fresh: bool,
) -> Result<Arc<Session>, String> {
    gate_for(root, data)?;
    if !fresh
        && let Some(r) = app.state::<Agent>().0.lock().unwrap().as_ref()
        && r.root == root
    {
        return Ok(r.session.clone());
    }
    let cmd = match adapter(data, std::env::var_os("IG_AGENT_CMD").map(PathBuf::from)) {
        Ok(cmd) => cmd,
        Err(AgentError::Setup {
            instructions,
            installable,
        }) => {
            let _ = app.emit(
                "agent-event",
                AgentEvent::Setup {
                    instructions: instructions.clone(),
                    installable,
                },
            );
            return Err(instructions);
        }
        Err(AgentError::NotTrusted) => return Err(NOT_TRUSTED.into()),
    };
    shutdown(app);
    let session = Arc::new(Session::spawn(&cmd, &[], root, sink(app)).await?);
    *app.state::<Agent>().0.lock().unwrap() = Some(Running {
        root: root.to_path_buf(),
        session: session.clone(),
        feature: None,
    });
    Ok(session)
}

fn gate_for(root: &Path, data: &Path) -> Result<(), String> {
    gate(root, data).map_err(|_| NOT_TRUSTED.to_string())
}

/// The running agent's session for `root`.
fn running(app: &AppHandle, root: &Path) -> Result<Arc<Session>, String> {
    match app.state::<Agent>().0.lock().unwrap().as_ref() {
        Some(r) if r.root == root => Ok(r.session.clone()),
        _ => Err("no agent running: start a session".into()),
    }
}

/// A new session, for `feature` when given. Its id.
#[tauri::command]
pub async fn agent_start(
    app: AppHandle,
    project: State<'_, Project>,
    feature: Option<String>,
) -> Result<String, String> {
    let (root, data) = open_root_and_data(&project, &app)?;
    let s = connect(&app, &root, &data, false).await?;
    let id = s.start(feature.as_deref()).await?;
    if let Some(r) = app.state::<Agent>().0.lock().unwrap().as_mut() {
        r.feature = feature;
    }
    Ok(id)
}

/// One turn; resolves with the stop reason when it ends (updates arrive as `agent-event`).
#[tauri::command]
pub async fn agent_prompt(
    app: AppHandle,
    project: State<'_, Project>,
    text: String,
    attachments: Vec<String>,
) -> Result<String, String> {
    let (root, data) = open_root_and_data(&project, &app)?;
    gate_for(&root, &data)?;
    running(&app, &root)?.prompt(&text, &attachments).await
}

/// An earlier session (from `agent_list`), replayed as `agent-event`s.
#[tauri::command]
pub async fn agent_load(
    app: AppHandle,
    project: State<'_, Project>,
    session_id: String,
) -> Result<(), String> {
    let (root, data) = open_root_and_data(&project, &app)?;
    connect(&app, &root, &data, false)
        .await?
        .load(&session_id)
        .await
}

/// After a `Stopped`: a new adapter and a new session for the same feature. Its id.
#[tauri::command]
pub async fn agent_restart(app: AppHandle, project: State<'_, Project>) -> Result<String, String> {
    let (root, data) = open_root_and_data(&project, &app)?;
    gate_for(&root, &data)?;
    let feature = app
        .state::<Agent>()
        .0
        .lock()
        .unwrap()
        .as_ref()
        .and_then(|r| r.feature.clone());
    let s = connect(&app, &root, &data, true).await?;
    let id = s.start(feature.as_deref()).await?;
    if let Some(r) = app.state::<Agent>().0.lock().unwrap().as_mut() {
        r.feature = feature;
    }
    Ok(id)
}

#[tauri::command]
pub async fn agent_cancel(app: AppHandle, project: State<'_, Project>) -> Result<(), String> {
    let (root, _) = open_root_and_data(&project, &app)?;
    running(&app, &root)?.cancel()
}

/// The user's answer to a `Permission` event: one of its `optionId`s, once.
#[tauri::command]
pub async fn agent_answer(
    app: AppHandle,
    project: State<'_, Project>,
    request_id: u64,
    option_id: String,
) -> Result<(), String> {
    let (root, _) = open_root_and_data(&project, &app)?;
    running(&app, &root)?.answer(request_id, &option_id)
}

/// `{sessions, features}`: the adapter's sessions here and the feature → session map.
#[tauri::command]
pub async fn agent_list(app: AppHandle, project: State<'_, Project>) -> Result<Value, String> {
    let (root, _) = open_root_and_data(&project, &app)?;
    running(&app, &root)?.list().await
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
    fn an_untrusted_project_never_passes_the_gate() {
        let base = std::env::temp_dir().join(format!("ig-agent-{}", std::process::id()));
        std::fs::create_dir_all(&base).unwrap();
        let data = base.join("data");
        assert_eq!(gate(&base, &data), Err(AgentError::NotTrusted));
        trust::trust(&data, &base, &trust::inventory(&base)).unwrap();
        assert_eq!(gate(&base, &data), Ok(()));
        let _ = std::fs::remove_dir_all(&base);
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
