//! `session.rs` against the fake agent (`examples/fake_agent.rs`, built by `cargo test`).

use super::*;
use std::sync::mpsc;
use std::time::Duration;
use tauri::async_runtime::{block_on, spawn};

/// The newest `fake_agent-<hash>` next to this test binary (both in target/<profile>/deps).
pub(crate) fn fake_agent() -> PathBuf {
    let deps = std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();
    std::fs::read_dir(&deps)
        .unwrap()
        .filter_map(|e| e.ok()?.path().into())
        .filter(|p: &PathBuf| {
            let name = p.file_name().unwrap().to_string_lossy();
            name.starts_with("fake_agent-") && p.extension().is_none()
        })
        .max_by_key(|p| p.metadata().and_then(|m| m.modified()).ok())
        .expect("no fake_agent: run `cargo test -p ig-desktop` (it builds the test target)")
}

struct Rig {
    root: PathBuf,
    s: Arc<Session>,
    rx: mpsc::Receiver<AgentEvent>,
}

impl Rig {
    async fn new(tag: &str) -> Rig {
        let root = std::env::temp_dir().join(format!("ig-session-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join(".claude/skills/ig")).unwrap();
        let root = root.canonicalize().unwrap();
        let (tx, rx) = mpsc::channel();
        let tx = Mutex::new(tx);
        let sink: Sink = Arc::new(move |e| {
            let _ = tx.lock().unwrap().send(e);
        });
        let rec = root.join("rec.log");
        let s = Session::spawn(
            &fake_agent(),
            &["--serve", rec.to_str().unwrap()],
            &root,
            sink,
        )
        .await
        .unwrap();
        Rig {
            root,
            s: Arc::new(s),
            rx,
        }
    }

    /// The next event `pick` accepts, skipping the others.
    fn next<T>(&self, mut pick: impl FnMut(AgentEvent) -> Option<T>) -> T {
        loop {
            let e = self
                .rx
                .recv_timeout(Duration::from_secs(10))
                .expect("no event in 10 s");
            if let Some(t) = pick(e) {
                return t;
            }
        }
    }

    fn text(&self) -> String {
        self.next(|e| match e {
            AgentEvent::MessageChunk { content, .. } => content["text"].as_str().map(Into::into),
            _ => None,
        })
    }

    fn permission(&self) -> (u64, Value) {
        self.next(|e| match e {
            AgentEvent::Permission {
                request_id,
                tool_call,
                ..
            } => Some((request_id, tool_call)),
            _ => None,
        })
    }

    /// The next tool call update: (toolCallId, status, the fake's echo of the answer).
    fn tool_end(&self) -> (String, String, String) {
        self.next(|e| match e {
            AgentEvent::ToolCallUpdate { update, .. } => Some((
                update["toolCallId"].as_str()?.into(),
                update["status"].as_str()?.into(),
                update["rawOutput"].as_str()?.into(),
            )),
            _ => None,
        })
    }

    fn prompt_in_background(
        &self,
        text: &'static str,
    ) -> impl Future<Output = Result<String, String>> {
        let s = self.s.clone();
        let h = spawn(async move { s.prompt(text, &[]).await });
        async move { h.await.unwrap() }
    }
}

impl Drop for Rig {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

#[test]
fn a_prompt_streams_in_order_and_the_session_gets_the_ig_plugin() {
    block_on(async {
        let r = Rig::new("stream").await;
        assert_eq!(r.s.start(Some("login")).await.unwrap(), "fake-1");
        assert_eq!(r.s.prompt("stream", &[]).await.unwrap(), "end_turn");
        let got: Vec<String> = (0..3).map(|_| r.text()).collect();
        assert_eq!(got, ["Hel", "lo", " world"]);
        r.s.prompt("meta", &[]).await.unwrap();
        let meta: Value = serde_json::from_str(&r.text()).unwrap();
        let opts = &meta["claudeCode"]["options"];
        assert_eq!(opts["settingSources"], json!(["project", "local"]));
        let plugin = r.root.join(".claude/skills/ig");
        assert_eq!(opts["plugins"], json!([{"type": "local", "path": plugin}]));
        assert_eq!(features(&r.root)["login"], "fake-1");
        // Attachments must stay inside the project.
        assert!(r.s.prompt("stream", &["../x".into()]).await.is_err());
    });
}

#[test]
fn a_permission_is_answered_once_with_the_adapters_option() {
    block_on(async {
        let r = Rig::new("perm").await;
        r.s.start(None).await.unwrap();
        let turn = r.prompt_in_background("perm");
        let (id, call) = r.permission();
        assert_eq!(call["content"][0]["type"], "diff");
        assert!(r.s.answer(id, "nope").is_err(), "not one of its options");
        r.s.answer(id, "reject").unwrap();
        assert!(r.s.answer(id, "reject").is_err(), "answered twice");
        assert_eq!(r.tool_end(), ("a".into(), "failed".into(), "reject".into()));
        assert_eq!(turn.await.unwrap(), "end_turn");
    });
}

#[test]
fn a_question_is_answered_once_with_its_own_fields() {
    block_on(async {
        let r = Rig::new("ask").await;
        r.s.start(None).await.unwrap();
        let turn = r.prompt_in_background("ask");
        let (id, message, schema) = r.next(|e| match e {
            AgentEvent::Question {
                request_id,
                message,
                schema,
                ..
            } => Some((request_id, message, schema)),
            _ => None,
        });
        assert_eq!(message, "How big?");
        assert_eq!(
            schema["properties"]["question_0"]["oneOf"][0]["const"],
            "Small"
        );
        assert!(r.s.answer(id, "reject").is_err(), "not a permission");
        assert!(
            r.s.reply(id, Some(json!({"other": "x"}))).is_err(),
            "not its field"
        );
        assert!(
            r.s.reply(id, Some(json!({"question_0": 3}))).is_err(),
            "not a label"
        );
        assert!(
            r.s.reply(id, Some(json!({"question_0": "Huge"}))).is_err(),
            "not an option"
        );
        r.s.reply(id, Some(json!({"question_0": "Small"}))).unwrap();
        assert!(r.s.reply(id, None).is_err(), "answered twice");
        assert_eq!(
            r.text(),
            r#"answer: {"action":"accept","content":{"question_0":"Small"}}"#
        );
        assert_eq!(turn.await.unwrap(), "end_turn");
    });
}

#[test]
fn config_options_arrive_with_the_session_and_can_be_set() {
    block_on(async {
        let r = Rig::new("config").await;
        let mode = |r: &Rig| {
            r.next(|e| match e {
                AgentEvent::Config { options, .. } => Some(options[0]["currentValue"].clone()),
                _ => None,
            })
        };
        r.s.start(None).await.unwrap();
        assert_eq!(mode(&r), "default");
        r.s.set_config("mode", "plan").await.unwrap();
        assert_eq!(mode(&r), "plan");
        let rec = std::fs::read_to_string(r.root.join("rec.log")).unwrap();
        assert!(rec.contains("config \"mode\" \"plan\""), "{rec}");
    });
}

/// Records the `tour` script's events (a reject, a picked answer) as the chat reducer's fixture:
/// `cargo test -p ig-desktop record_chat_fixture -- --ignored`.
#[test]
#[ignore]
fn record_chat_fixture() {
    block_on(async {
        let r = Rig::new("fixture").await;
        r.s.start(None).await.unwrap();
        let turn = r.prompt_in_background("tour");
        let mut all = vec![];
        loop {
            let e = r.next(Some);
            all.push(serde_json::to_value(&e).unwrap());
            match e {
                AgentEvent::Permission { request_id, .. } => {
                    r.s.answer(request_id, "reject").unwrap()
                }
                AgentEvent::Question { request_id, .. } => {
                    r.s.reply(request_id, Some(json!({"question_0": "Small"})))
                        .unwrap()
                }
                AgentEvent::Usage { .. } => break,
                _ => {}
            }
        }
        assert_eq!(turn.await.unwrap(), "end_turn");
        let out = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../viewer/test/fixtures/agent-events.json"
        );
        std::fs::create_dir_all(Path::new(out).parent().unwrap()).unwrap();
        std::fs::write(out, serde_json::to_string_pretty(&all).unwrap() + "\n").unwrap();
    });
}

#[test]
fn two_concurrent_permissions_are_each_answered_once() {
    block_on(async {
        let r = Rig::new("two").await;
        r.s.start(None).await.unwrap();
        let turn = r.prompt_in_background("two");
        let (a, _) = r.permission();
        let (b, _) = r.permission();
        assert_ne!(a, b);
        r.s.answer(b, "allow-once").unwrap();
        r.s.answer(a, "reject").unwrap();
        assert!(r.s.answer(a, "allow-once").is_err());
        assert!(r.s.answer(b, "reject").is_err());
        let mut ends = [r.tool_end(), r.tool_end()];
        ends.sort();
        assert_eq!(
            ends,
            [
                ("a".into(), "failed".into(), "reject".into()),
                ("b".into(), "completed".into(), "allow-once".into())
            ]
        );
        assert_eq!(turn.await.unwrap(), "end_turn");
    });
}

#[test]
fn a_closed_window_rejects_what_is_pending() {
    block_on(async {
        let r = Rig::new("close").await;
        r.s.start(None).await.unwrap();
        let _turn = r.prompt_in_background("perm");
        let (id, _) = r.permission();
        // As on a closed window: the reject reaches the adapter before it is killed.
        wait(r.s.stop().unwrap(), 5000).await;
        let rec = std::fs::read_to_string(r.root.join("rec.log")).unwrap();
        assert_eq!(rec, "spawn\nanswer reject\n");
        assert!(r.s.answer(id, "allow-once").is_err());
        assert!(r.s.stop().is_none(), "stopped once");
    });
}

#[test]
fn cancel_stops_the_turn() {
    block_on(async {
        let r = Rig::new("cancel").await;
        r.s.start(None).await.unwrap();
        let turn = r.prompt_in_background("hang");
        assert_eq!(r.text(), "thinking");
        r.s.cancel().unwrap();
        assert_eq!(turn.await.unwrap(), "cancelled");
    });
}

#[test]
fn a_crash_mid_turn_stops_with_the_reason() {
    block_on(async {
        let r = Rig::new("crash").await;
        r.s.start(None).await.unwrap();
        assert!(r.s.prompt("crash", &[]).await.is_err());
        let (reason, stderr) = r.next(|e| match e {
            AgentEvent::Stopped { reason, stderr } => Some((reason, stderr)),
            _ => None,
        });
        assert!(reason.contains('3'), "{reason}");
        assert!(stderr.unwrap().contains("boom"));
    });
}

#[test]
fn not_logged_in_asks_for_a_login_list_and_load_work() {
    block_on(async {
        let r = Rig::new("auth").await;
        r.s.start(Some("f")).await.unwrap();
        r.s.prompt("auth", &[]).await.unwrap();
        let auth = r.next(|e| matches!(e, AgentEvent::Auth { .. }).then_some(e));
        assert_eq!(
            auth,
            AgentEvent::Auth {
                needed: true,
                label: None
            }
        );
        let l = r.s.list().await.unwrap();
        assert_eq!(l["sessions"][0]["sessionId"], "fake-1");
        assert_eq!(l["features"]["f"], "fake-1");
        r.s.load("fake-1").await.unwrap();
        assert_eq!(r.text(), "earlier");
    });
}

#[test]
fn stop_ends_a_running_turn_without_a_stopped_event() {
    block_on(async {
        let r = Rig::new("stop").await;
        r.s.start(None).await.unwrap();
        let turn = r.prompt_in_background("hang");
        assert_eq!(r.text(), "thinking");
        r.s.stop();
        assert!(turn.await.is_err());
        let late: Vec<_> = r.rx.try_iter().collect();
        assert!(
            !late.iter().any(|e| matches!(e, AgentEvent::Stopped { .. })),
            "{late:?}"
        );
    });
}

/// The manual check against the real adapter (not run by `cargo test`; it uses the Claude account):
/// `IG_AGENT_CMD=<…/claude-agent-acp> IG_AGENT_PROJ=<scratch project with ig init>
/// IG_AGENT_DATA=<scratch data dir> cargo test -p ig-desktop real_adapter -- --ignored --nocapture`.
/// Two prompts: `/ig:explore` (streams, then cancelled), then a file write (rejected).
#[test]
#[ignore]
fn real_adapter() {
    use crate::agent::{adapter, gate};
    let var = |k| PathBuf::from(std::env::var_os(k).unwrap_or_else(|| panic!("{k} unset")));
    let (root, data) = (
        var("IG_AGENT_PROJ").canonicalize().unwrap(),
        var("IG_AGENT_DATA"),
    );
    crate::trust::trust(&data, &root, &crate::trust::inventory(&root)).unwrap();
    gate(&root, &data).unwrap();
    let cmd = adapter(&data, std::env::var_os("IG_AGENT_CMD").map(PathBuf::from)).unwrap();
    block_on(async {
        let (tx, rx) = mpsc::channel();
        let tx = Mutex::new(tx);
        let sink: Sink = Arc::new(move |e| {
            let _ = tx.lock().unwrap().send(e);
        });
        let s = Arc::new(Session::spawn(&cmd, &[], &root, sink).await.unwrap());
        let r = Rig {
            root: PathBuf::new(),
            s,
            rx,
        };
        let id = r.s.start(Some("manual")).await.unwrap();
        eprintln!("session {id}");

        // 1. /ig:explore streams; cancel after a few chunks (a permission request is rejected).
        let turn = r.prompt_in_background("/ig:explore what is in this repo?");
        let (mut chunks, mut commands) = (0, None);
        while chunks < 5 {
            match r
                .rx
                .recv_timeout(Duration::from_secs(120))
                .expect("no event in 120 s")
            {
                AgentEvent::MessageChunk {
                    role: "agent",
                    content,
                    ..
                } => {
                    chunks += 1;
                    eprintln!("chunk {chunks}: {:?}", content["text"]);
                }
                AgentEvent::Commands { commands: c, .. } => commands = Some(c),
                AgentEvent::Permission {
                    request_id,
                    tool_call,
                    ..
                } => {
                    eprintln!(
                        "permission during explore: {}, rejecting",
                        tool_call["title"]
                    );
                    r.s.answer(request_id, "reject").unwrap();
                }
                AgentEvent::Auth { needed, label } => {
                    eprintln!("auth: needed={needed} label={label:?}")
                }
                e @ AgentEvent::Stopped { .. } => panic!("{e:?}"),
                _ => {}
            }
        }
        r.s.cancel().unwrap();
        eprintln!("explore stop reason: {:?}", turn.await);
        let commands = commands.or_else(|| {
            r.rx.try_iter().find_map(|e| match e {
                AgentEvent::Commands { commands, .. } => Some(commands),
                _ => None,
            })
        });
        if let Some(c) = commands {
            let user = c
                .iter()
                .filter(|c| c.description.ends_with("(user)"))
                .count();
            let names: Vec<&str> = c.iter().map(|c| c.name.as_str()).collect();
            eprintln!(
                "available commands: {} (described \"(user)\": {user}): {names:?}",
                c.len()
            );
        }

        // 2. A write is asked for and rejected: the tool call fails, no file.
        let _ = std::fs::remove_file(root.join("hello.txt"));
        let turn = r.prompt_in_background("Create a file named hello.txt containing hi");
        let mut rejected = None;
        loop {
            match r
                .rx
                .recv_timeout(Duration::from_secs(120))
                .expect("no event in 120 s")
            {
                AgentEvent::Permission {
                    request_id,
                    tool_call,
                    options,
                    ..
                } => {
                    eprintln!("permission: {} options {options}", tool_call["title"]);
                    r.s.answer(request_id, "reject").unwrap();
                    rejected = tool_call["toolCallId"].as_str().map(String::from);
                }
                AgentEvent::ToolCallUpdate { update, .. }
                    if rejected.is_some()
                        && update["toolCallId"].as_str() == rejected.as_deref()
                        && update["status"] == "failed" =>
                {
                    eprintln!("rejected tool call failed: {}", update["rawOutput"]);
                    break;
                }
                _ => {}
            }
        }
        r.s.cancel().unwrap();
        eprintln!("write stop reason: {:?}", turn.await);
        eprintln!("hello.txt exists: {}", root.join("hello.txt").exists());
        eprintln!("list: {}", r.s.list().await.unwrap());
        r.s.stop();
    });
}

/// Task 7's check of `session/load` on the real adapter (2 prompts at most): a session with one
/// tiny turn, then a new adapter process loads it and the history must replay. Same env as above.
#[test]
#[ignore]
fn real_adapter_load() {
    use crate::agent::adapter;
    let var = |k| PathBuf::from(std::env::var_os(k).unwrap_or_else(|| panic!("{k} unset")));
    let (root, data) = (
        var("IG_AGENT_PROJ").canonicalize().unwrap(),
        var("IG_AGENT_DATA"),
    );
    let cmd = adapter(&data, std::env::var_os("IG_AGENT_CMD").map(PathBuf::from)).unwrap();
    let rig = |s: Session, rx| Rig {
        root: PathBuf::new(),
        s: Arc::new(s),
        rx,
    };
    let sink = || {
        let (tx, rx) = mpsc::channel();
        let tx = Mutex::new(tx);
        let sink: Sink = Arc::new(move |e| {
            let _ = tx.lock().unwrap().send(e);
        });
        (sink, rx)
    };
    block_on(async {
        let (k, rx) = sink();
        let r = rig(Session::spawn(&cmd, &[], &root, k).await.unwrap(), rx);
        let id = r.s.start(None).await.unwrap();
        let stop = r.s.prompt("Reply with the single word: pong", &[]).await;
        eprintln!("session {id}: first turn {stop:?}");
        wait(r.s.stop().unwrap(), 2000).await;

        let (k, rx) = sink();
        let r2 = rig(Session::spawn(&cmd, &[], &root, k).await.unwrap(), rx);
        r2.s.load(&id).await.unwrap();
        nap(1500).await.unwrap();
        for e in r2.rx.try_iter() {
            match e {
                AgentEvent::MessageChunk { role, content, .. } => {
                    eprintln!("replay {role}: {:?}", content["text"])
                }
                AgentEvent::Config { options, .. } => eprintln!(
                    "replay config: {} options",
                    options.as_array().map_or(0, Vec::len)
                ),
                e => eprintln!(
                    "replay other: {}",
                    serde_json::to_value(&e).unwrap()["type"]
                ),
            }
        }
        r2.s.stop();
    });
}
