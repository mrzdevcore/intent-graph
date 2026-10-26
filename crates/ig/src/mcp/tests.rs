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
    assert_eq!(names.len(), 11);
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

const DESIGN: &str = r#"{"version":1,"title":"Export","scope":{"kind":"dir","id":"."},"elements":[{"id":"fn-a","kind":"function","name":"a","file":"packages/core/src/a.ts"}]}"#;

fn design_fixture() -> std::path::PathBuf {
    let dir = crate::tests::git_fixture("monorepo");
    std::fs::create_dir_all(dir.join("plans/export")).unwrap();
    std::fs::write(dir.join("plans/export/design.json"), DESIGN).unwrap();
    dir
}

fn design_bytes(dir: &Path) -> Vec<u8> {
    std::fs::read(dir.join("plans/export/design.json")).unwrap()
}

#[test]
fn design_edit_writes_canonically_and_reports_touched_and_ok() {
    let dir = design_fixture();
    let ops = json!([
        {"op": "add", "id": "fn-b", "kind": "function", "name": "b", "file": "packages/core/src/b.ts"},
        {"op": "link", "from": "fn-a", "to": "fn-b", "kind": "calls"}
    ]);
    let r = call_tool(&dir, "design_edit", json!({"slug": "export", "ops": ops}));
    assert!(r.get("isError").is_none(), "{r}");
    let text = String::from_utf8(design_bytes(&dir)).unwrap();
    let d = ig_core::design::parse(&text).unwrap();
    assert_eq!(text, ig_core::design::to_text(&d));
    assert_eq!(d.elements.len(), 2);
    let s = &r["structuredContent"];
    assert_eq!(s["touched"]["elements"], json!(["fn-b"]));
    assert_eq!(s["touched"]["links"], json!(["fn-a|calls|fn-b"]));
    assert_eq!(s["check"]["ok"], true, "{s}");
    let t = r["content"][0]["text"].as_str().unwrap();
    assert!(t.contains("fn-b") && t.contains("check: ok"), "{t}");
}

#[test]
fn design_edit_refuses_bad_slug() {
    let dir = design_fixture();
    for slug in ["../export", "Export", ""] {
        let r = call_tool(&dir, "design_edit", json!({"slug": slug, "ops": []}));
        assert_eq!(r["isError"], true, "{slug}");
        assert!(
            r["content"][0]["text"]
                .as_str()
                .unwrap()
                .contains("bad feature name")
        );
    }
    assert_eq!(design_bytes(&dir), DESIGN.as_bytes());
}

#[test]
fn invalid_ops_leave_the_bytes_unchanged() {
    let dir = design_fixture();
    // the first op is fine, the second is not: all or nothing
    let ops = json!([
        {"op": "update", "id": "fn-a", "set": {"name": "A"}},
        {"op": "move", "id": "fn-zzz", "x": 1, "y": 2}
    ]);
    let r = call_tool(&dir, "design_edit", json!({"slug": "export", "ops": ops}));
    assert_eq!(r["isError"], true, "{r}");
    assert_eq!(design_bytes(&dir), DESIGN.as_bytes());
    let r = call_tool(
        &dir,
        "design_edit",
        json!({"slug": "export", "ops": [{"op": "nope"}]}),
    );
    assert_eq!(r["isError"], true, "{r}");
    assert_eq!(design_bytes(&dir), DESIGN.as_bytes());
}

#[test]
fn design_edit_and_engine_share_validation() {
    let dir = design_fixture();
    let ops = json!([{"op": "link", "from": "fn-a", "to": "fn-nope", "kind": "calls"}]);
    let r = call_tool(&dir, "design_edit", json!({"slug": "export", "ops": ops}));
    let board = ig_core::design::Board {
        design: ig_core::design::parse(DESIGN).unwrap(),
        screens: Default::default(),
    };
    let parsed: Vec<ig_core::design::Op> = serde_json::from_value(ops).unwrap();
    let want = ig_core::design::apply(&board, &parsed)
        .unwrap_err()
        .to_string();
    assert_eq!(r["content"][0]["text"], format!("ig: {want}"));
}

#[test]
fn design_edit_caps_ops_and_strings_and_leaves_empty_edits_alone() {
    let dir = design_fixture();
    let mv = json!({"op": "move", "id": "fn-a", "x": 8, "y": 8});
    let r = call_tool(
        &dir,
        "design_edit",
        json!({"slug": "export", "ops": vec![mv; 201]}),
    );
    assert!(
        r["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("at most 200 ops"),
        "{r}"
    );
    let long = json!([{"op": "update", "id": "fn-a", "set": {"name": "x".repeat(201)}}]);
    let r = call_tool(&dir, "design_edit", json!({"slug": "export", "ops": long}));
    assert!(
        r["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("longer than 200 bytes"),
        "{r}"
    );
    let r = call_tool(&dir, "design_edit", json!({"slug": "export", "ops": []}));
    assert!(r.get("isError").is_none(), "{r}");
    assert_eq!(
        design_bytes(&dir),
        DESIGN.as_bytes(),
        "not even rewritten canonically"
    );
}

#[test]
fn a_check_that_fails_after_the_write_is_reported_not_an_error() {
    let dir = design_fixture();
    std::fs::write(dir.join("ig.yaml"), "layers: [unclosed\n").unwrap();
    let ops = json!([{"op": "update", "id": "fn-a", "set": {"name": "A"}}]);
    let r = call_tool(&dir, "design_edit", json!({"slug": "export", "ops": ops}));
    assert!(r.get("isError").is_none(), "{r}");
    let t = r["content"][0]["text"].as_str().unwrap();
    assert!(t.contains("check: unavailable ("), "{t}");
    assert!(
        String::from_utf8(design_bytes(&dir))
            .unwrap()
            .contains("\"A\"")
    );
}

#[test]
fn design_edit_writes_screen_files() {
    let dir = design_fixture();
    let ops = json!([
        {"op": "add_screen", "id": "login", "title": "Login"},
        {"op": "add_screen", "id": "home", "title": "Home"},
        {"op": "screen_add", "screen": "login", "kind": "button", "label": "Go", "props": {"text": "Go"}},
        {"op": "nav", "screen": "login", "from": "btn-go", "to": "home", "label": "ok"},
        {"op": "bridge", "from": "login/btn-go", "to": "fn-a"}
    ]);
    let r = call_tool(&dir, "design_edit", json!({"slug": "export", "ops": ops}));
    assert!(r.get("isError").is_none(), "{r}");
    assert_eq!(
        r["structuredContent"]["touched"]["screens"],
        json!(["login", "home"])
    );
    let f = |id: &str| {
        std::fs::read_to_string(dir.join(format!("plans/export/screens/{id}.json"))).unwrap()
    };
    let s = ig_core::design::parse_screen(&f("login")).unwrap();
    assert_eq!(f("login"), ig_core::design::screen_to_text(&s));
    assert_eq!(s.elements[0].id, "btn-go");
    // removing a screen deletes its file and the bridge in the same call
    let ops = json!([{"op": "remove_screen", "id": "login"}]);
    let r = call_tool(&dir, "design_edit", json!({"slug": "export", "ops": ops}));
    assert!(r.get("isError").is_none(), "{r}");
    assert!(!dir.join("plans/export/screens/login.json").exists());
    assert!(
        !String::from_utf8(design_bytes(&dir))
            .unwrap()
            .contains("login/btn-go")
    );
    // a broken screen file is never overwritten, and blocks only screen ops
    std::fs::write(dir.join("plans/export/screens/home.json"), "<<<").unwrap();
    let ops = json!([{"op": "screen_add", "screen": "home", "kind": "text", "label": "x"}]);
    let r = call_tool(&dir, "design_edit", json!({"slug": "export", "ops": ops}));
    assert_eq!(r["isError"], true, "{r}");
    assert_eq!(f("home"), "<<<");
    let ops = json!([{"op": "update", "id": "fn-a", "set": {"name": "A"}}]);
    let r = call_tool(&dir, "design_edit", json!({"slug": "export", "ops": ops}));
    assert!(r.get("isError").is_none(), "{r}");
}
