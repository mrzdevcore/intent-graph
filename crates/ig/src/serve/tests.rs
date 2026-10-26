use super::*;
use std::io::BufRead;
use std::sync::mpsc;
use std::time::Instant;

/// A scratch copy of the monorepo fixture with a checked plan, served on a free port.
pub(crate) fn start(tag: &str) -> (PathBuf, ServeInfo) {
    let dir = crate::tests::scratch(tag);
    std::fs::create_dir_all(dir.join("plans")).unwrap();
    std::fs::write(dir.join("plans/export.md"), crate::tests::PLAN).unwrap();
    crate::tests::plan_check(&dir, &dir.join("plans/export.md")).unwrap();
    let d = dir.clone();
    std::thread::spawn(move || run(&d, Some(0), &Opts::default(), None, false, false));
    let end = Instant::now() + Duration::from_secs(60);
    loop {
        if let Some(info) = discover(&dir) {
            return (dir, info);
        }
        assert!(Instant::now() < end, "ig serve did not start");
        std::thread::sleep(Duration::from_millis(100));
    }
}

pub(crate) fn post(
    info: &ServeInfo,
    path: &str,
    body: &Value,
    token: Option<&str>,
    origin: Option<&str>,
) -> (u16, Value) {
    let mut r = agent(Some(Duration::from_secs(10))).post(info.url(path));
    if let Some(t) = token {
        r = r.header("X-IG-Token", t);
    }
    if let Some(o) = origin {
        r = r.header("Origin", o);
    }
    let mut resp = r.send_json(body).unwrap();
    let v = resp.body_mut().read_json().unwrap_or(Value::Null);
    (resp.status().as_u16(), v)
}

/// The lines of `/api/events`, once the stream is open.
pub(crate) fn events(info: &ServeInfo) -> mpsc::Receiver<String> {
    let (tx, rx) = mpsc::channel();
    let url = info.url("/api/events");
    std::thread::spawn(move || {
        let resp = agent(None).get(&url).call().unwrap();
        for line in std::io::BufReader::new(resp.into_body().into_reader()).lines() {
            if tx.send(line.unwrap_or_default()).is_err() {
                break;
            }
        }
    });
    assert_eq!(
        rx.recv_timeout(Duration::from_secs(10)).unwrap(),
        ": connected"
    );
    rx
}

#[test]
fn a_taken_default_port_falls_back_but_an_explicit_one_errors() {
    let dir = crate::tests::scratch("bind");
    let held = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let p = held.local_addr().unwrap().port();
    assert!(bind(&dir, p, true).is_err());
    let s = bind(&dir, p, false).unwrap().unwrap();
    assert_ne!(s.server_addr().to_ip().unwrap().port(), p);
}

pub(crate) const COMMENT: &str = "packages/core/src/repo.ts#Repo.find";

#[test]
fn api_view_serves_the_pages_data_without_a_token() {
    let (_dir, info) = start("serve-view");
    let mut resp = agent(Some(Duration::from_secs(10)))
        .get(info.url("/api/view"))
        .call()
        .unwrap();
    let v: Value = resp.body_mut().read_json().unwrap();
    assert!(v["nodes"].as_array().is_some_and(|n| !n.is_empty()), "{v}");
    assert_eq!(v["state"], "planned");
    assert!(v["plan"]["title"].is_string(), "{v}");
    let page = agent(None)
        .get(info.url("/"))
        .call()
        .unwrap()
        .into_body()
        .read_to_string()
        .unwrap();
    assert!(
        page.contains(&v["root"].to_string()),
        "root differs from the page's"
    );
}

#[test]
fn posts_need_the_token_and_a_local_origin_and_a_note_reaches_the_file_and_the_stream() {
    let (dir, info) = start("serve");
    let note = json!({"kind": "comment", "target": COMMENT, "text": "why here?"});
    assert_eq!(post(&info, "/api/notes", &note, None, None).0, 403);
    let evil = Some("http://evil.example");
    assert_eq!(
        post(&info, "/api/notes", &note, Some(&info.token), evil).0,
        403
    );
    let rx = events(&info);
    let local = format!("http://localhost:{}", info.port);
    let (code, v) = post(&info, "/api/notes", &note, Some(&info.token), Some(&local));
    assert_eq!((code, v["id"].as_str()), (200, Some("N1")), "{v}");
    let text = std::fs::read_to_string(dir.join("plans/export.notes.md")).unwrap();
    assert!(
        text.contains(&format!("- [ ] N1 comment on {COMMENT}: why here?\n")),
        "{text}"
    );
    let (code, v) = post(
        &info,
        "/api/notes",
        &json!({"kind": "remove", "target": "nope.ts"}),
        Some(&info.token),
        None,
    );
    assert_eq!(code, 400, "{v}");
    let mut got = vec![];
    while let Ok(l) = rx.recv_timeout(Duration::from_secs(10)) {
        got.push(l);
        if got
            .iter()
            .any(|l| l.starts_with("data:") && l.contains("\"N1\""))
        {
            break;
        }
    }
    let i = got
        .iter()
        .position(|l| l == "event: note")
        .expect("a note event");
    assert!(got[i + 1].contains("why here?"), "{got:?}");
    for bad in [json!("x"), json!({"done": "yes"}), json!({"reply": 3})] {
        assert_eq!(
            post(&info, "/api/notes/N1", &bad, Some(&info.token), None).0,
            400,
            "{bad}"
        );
    }
    assert!(
        !std::fs::read_to_string(dir.join("plans/export.notes.md"))
            .unwrap()
            .contains("[x]")
    );
    let link = json!({"kind": "link", "target": COMMENT, "to": COMMENT, "text": "eats"});
    assert_eq!(
        post(&info, "/api/notes", &link, Some(&info.token), None).0,
        400
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(dir.join(".ig/serve.json"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
    }
    let mut page = agent(None).get(info.url("/")).call().unwrap();
    assert_eq!(page.headers().get("X-Frame-Options").unwrap(), "DENY");
    assert_eq!(
        page.headers().get("Content-Security-Policy").unwrap(),
        "frame-ancestors 'none'"
    );
    page.body_mut().read_to_string().unwrap();
    let (code, v) = post(
        &info,
        "/api/notes/N1",
        &json!({"done": true, "reply": "kept"}),
        Some(&info.token),
        None,
    );
    assert_eq!((code, v["done"].as_bool()), (200, Some(true)), "{v}");
    let mut state = agent(None).get(info.url("/api/state")).call().unwrap();
    let state: Value = state.body_mut().read_json().unwrap();
    assert_eq!(state["state"], "planned");
    assert_eq!(state["notes"][0]["reply"], "kept");
    assert_eq!(
        post(
            &info,
            "/api/focus",
            &json!({"ids": [COMMENT]}),
            Some(&info.token),
            None
        )
        .0,
        200
    );
}
