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
    ] {
        assert!(names.contains(&t), "{t} missing");
    }
    assert_eq!(names.len(), 9);
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
