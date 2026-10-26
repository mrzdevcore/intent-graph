use super::*;

#[test]
fn protocol_basics() {
    let root = Path::new(".");
    let r = handle(
        root,
        &json!({"jsonrpc": "2.0", "id": 1, "method": "tools/list"}),
    )
    .unwrap();
    let names: Vec<&str> = r["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    for t in [
        "ig_state",
        "ig_notes",
        "ig_note_done",
        "ig_focus",
        "ig_wait_notes",
        "ig_show",
    ] {
        assert!(names.contains(&t), "{t} missing");
    }
    assert_eq!(names.len(), 10);
    let r = handle(
        root,
        &json!({"jsonrpc": "2.0", "id": 3, "method": "initialize", "params": {}}),
    )
    .unwrap();
    assert_eq!(
        r["result"]["capabilities"]["experimental"]["claude/channel"],
        json!({})
    );
    assert!(
        handle(
            root,
            &json!({"jsonrpc": "2.0", "method": "notifications/initialized"})
        )
        .is_none()
    );
    let r = handle(root, &json!({"jsonrpc": "2.0", "id": 2, "method": "nope"})).unwrap();
    assert_eq!(r["error"]["code"], -32601);
}

#[test]
fn query_suggests() {
    let mut g = Graph::empty();
    let n = |id: &str| crate::model::Node {
        id: id.into(),
        kind: "function".into(),
        name: id.into(),
        file: "a.ts".into(),
        range: crate::model::Range {
            start_line: 1,
            end_line: 1,
        },
        parent: None,
        package: None,
        exported: false,
        hash: String::new(),
        metrics: crate::model::Metrics {
            loc: 1,
            complexity: 1,
        },
        intents: vec![],
    };
    g.nodes = vec![n("a.ts#foobar"), n("a.ts#foo")];
    let v = query(&g, "foo", "both", 1);
    assert_eq!(v["suggestions"][0], "a.ts#foo");
}

fn call_tool(root: &Path, name: &str, args: Value) -> Value {
    let msg = json!({"jsonrpc": "2.0", "id": 9, "method": "tools/call", "params": {"name": name, "arguments": args}});
    handle(root, &msg).unwrap()["result"].clone()
}

#[test]
fn a_new_note_becomes_a_channel_event_and_wakes_ig_wait_notes() {
    use crate::serve::tests::{COMMENT, post, start};
    let (dir, info) = start("mcp");
    let got: Arc<Mutex<Vec<Value>>> = Arc::new(Mutex::new(vec![]));
    {
        let (dir, got) = (dir.clone(), got.clone());
        std::thread::spawn(move || relay(&dir, &move |v| got.lock().unwrap().push(v)));
    }
    let waiter = {
        let dir = dir.clone();
        std::thread::spawn(move || call_tool(&dir, "ig_wait_notes", json!({"timeout_s": 30})))
    };
    std::thread::sleep(Duration::from_millis(1500));
    let note = json!({"kind": "comment", "target": COMMENT, "text": "split this"});
    assert_eq!(
        post(&info, "/api/notes", &note, Some(&info.token), None).0,
        200
    );
    let r = waiter.join().unwrap();
    assert!(
        r["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("N1 comment on"),
        "{r}"
    );
    let end = std::time::Instant::now() + Duration::from_secs(10);
    let msg = loop {
        if let Some(m) = got.lock().unwrap().first().cloned() {
            break m;
        }
        assert!(std::time::Instant::now() < end, "no channel event");
        std::thread::sleep(Duration::from_millis(100));
    };
    assert_eq!(msg["method"], "notifications/claude/channel");
    assert!(
        msg["params"]["content"]
            .as_str()
            .unwrap()
            .contains("split this")
    );
    let key = regex::Regex::new("^[A-Za-z0-9_]+$").unwrap();
    let meta = msg["params"]["meta"].as_object().unwrap();
    assert!(
        meta.keys().all(|k| key.is_match(k)) && meta["note_id"] == "N1",
        "{msg}"
    );
    let open = call_tool(&dir, "ig_notes", json!({}));
    assert_eq!(open["structuredContent"]["notes"][0]["id"], "N1");
    let done = call_tool(
        &dir,
        "ig_note_done",
        json!({"id": "N1", "reply": "split into two"}),
    );
    assert!(done.get("isError").is_none(), "{done}");
    assert_eq!(
        call_tool(&dir, "ig_notes", json!({}))["structuredContent"]["notes"],
        json!([])
    );
    assert!(
        call_tool(Path::new("."), "ig_state", json!({}))["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("run `ig serve`")
    );
}

#[test]
fn channel_events_start_only_after_initialized() {
    use crate::serve::tests::{COMMENT, post, start};
    let (dir, info) = start("mcp-init");
    let note = json!({"kind": "comment", "target": COMMENT, "text": "early"});
    assert_eq!(
        post(&info, "/api/notes", &note, Some(&info.token), None).0,
        200
    );
    let got: Arc<Mutex<Vec<Value>>> = Arc::new(Mutex::new(vec![]));
    let (tx, rx) = std::sync::mpsc::channel::<String>();
    {
        let got = got.clone();
        let send: Arc<dyn Fn(Value) + Send + Sync> = Arc::new(move |v| got.lock().unwrap().push(v));
        std::thread::spawn(move || serve_lines(&dir, rx.into_iter().map(Ok), send));
    }
    let channel = |got: &Arc<Mutex<Vec<Value>>>| {
        got.lock()
            .unwrap()
            .iter()
            .filter(|m| m["method"] == "notifications/claude/channel")
            .count()
    };
    tx.send(json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {}}).to_string())
        .unwrap();
    std::thread::sleep(Duration::from_secs(3));
    assert_eq!(channel(&got), 0, "a channel event before initialized");
    tx.send(json!({"jsonrpc": "2.0", "method": "notifications/initialized"}).to_string())
        .unwrap();
    let end = std::time::Instant::now() + Duration::from_secs(10);
    while channel(&got) == 0 {
        assert!(
            std::time::Instant::now() < end,
            "no channel event after initialized"
        );
        std::thread::sleep(Duration::from_millis(100));
    }
    let msgs = got.lock().unwrap();
    assert_eq!(msgs[0]["id"], 1, "the initialize reply comes first");
}

#[test]
fn writes_show_json_canonically_and_nothing_else() {
    let dir = crate::tests::git_fixture("monorepo");
    let g = head_graph(&dir).unwrap();
    let e = g.edges.iter().find(|e| e.kind == "calls").unwrap();
    let ig_files = |d: &Path| -> Vec<String> {
        std::fs::read_dir(d.join(".ig"))
            .map(|r| {
                r.map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
                    .collect()
            })
            .unwrap_or_default()
    };
    let before = ig_files(&dir);
    let args = json!({"title": "How it flows", "nodes": [e.from, e.to], "edges": [format!("{}|{}|{}", e.from, e.kind, e.to)],
        "steps": [{"target": e.from, "text": "starts here"}], "mode": "chain"});
    let r = call_tool(&dir, "ig_show", args.clone());
    assert!(r.get("isError").is_none(), "{r}");
    assert_eq!(
        r["content"][0]["text"],
        "no viewer is open; answer with a diagram too"
    );
    let mut new: Vec<String> = ig_files(&dir)
        .into_iter()
        .filter(|f| !before.contains(f))
        .collect();
    new.sort();
    assert_eq!(new, vec!["show.json".to_string()]);
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(&dir)
        .args(["status", "--porcelain"])
        .output()
        .unwrap();
    let status = String::from_utf8(out.stdout).unwrap();
    assert!(
        status.lines().all(|l| l.ends_with(" .ig/")),
        "outside .ig/: {status}"
    );
    let text = std::fs::read_to_string(dir.join(ig_app::show::SHOW)).unwrap();
    let show: ig_app::show::Show = serde_json::from_str(&text).unwrap();
    assert_eq!(text, ig_app::show::canonical(&show));
    assert_eq!(show.seq, 1);
    assert_eq!(show.edges, vec![(e.from.clone(), e.to.clone())]);
    call_tool(&dir, "ig_show", args);
    let text = std::fs::read_to_string(dir.join(ig_app::show::SHOW)).unwrap();
    assert_eq!(serde_json::from_str::<Value>(&text).unwrap()["seq"], 2);
}

#[test]
fn unknown_id_is_a_tool_error_and_writes_nothing() {
    let dir = crate::tests::git_fixture("monorepo");
    let r = call_tool(
        &dir,
        "ig_show",
        json!({"title": "t", "nodes": ["packages/core/src/repo.ts#Repo.fnd"]}),
    );
    assert_eq!(r["isError"], true);
    let text = r["content"][0]["text"].as_str().unwrap();
    assert!(
        text.contains("unknown id packages/core/src/repo.ts#Repo.fnd; did you mean packages/core/src/repo.ts#Repo.find"),
        "{text}"
    );
    assert!(!dir.join(ig_app::show::SHOW).exists());
}
