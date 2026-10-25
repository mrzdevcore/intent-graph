//! `ig serve`: the live viewer on 127.0.0.1:<port>, a JSON API for the viewer and `ig mcp`, and a
//! server-sent event stream (`rebuild`, `note`, `focus`). Every POST needs the run's token.

use crate::check::{self, Opts};
use crate::model::Graph;
use crate::notes::{self, Note, NoteInput};
use crate::{view, workspace};
use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub const PORT: u16 = 7420;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServeInfo {
    pub port: u16,
    pub pid: u32,
    pub token: String,
}

impl ServeInfo {
    pub fn url(&self, path: &str) -> String {
        format!("http://127.0.0.1:{}{path}", self.port)
    }
}

/// An HTTP client for the local server: no proxy, and error statuses are responses.
pub fn agent(timeout: Option<Duration>) -> ureq::Agent {
    ureq::Agent::config_builder()
        .proxy(None)
        .http_status_as_error(false)
        .timeout_global(timeout)
        .build()
        .into()
}

const INFO: &str = ".ig/serve.json";

/// The server running for `root`, from `.ig/serve.json`, if it answers for this root.
pub fn discover(root: &Path) -> Option<ServeInfo> {
    let info: ServeInfo =
        serde_json::from_str(&std::fs::read_to_string(root.join(INFO)).ok()?).ok()?;
    let mut r = agent(Some(Duration::from_secs(2)))
        .get(info.url("/api/ping"))
        .call()
        .ok()?;
    let v: Value = r.body_mut().read_json().ok()?;
    (v["root"].as_str() == Some(&*root.to_string_lossy())).then_some(info)
}

struct Built {
    plan: Option<PathBuf>,
    /// `{state, plan}` for `/api/state`.
    state: Value,
    /// The working-tree graph with the plan applied: note targets are checked against it.
    merged: Graph,
    page: String,
}

struct Server {
    root: PathBuf,
    port: u16,
    token: String,
    built: Mutex<Built>,
    notes: Mutex<Vec<Note>>,
    clients: Mutex<Vec<Box<dyn Write + Send>>>,
}

fn build(
    root: &Path,
    opts: &Opts,
    plan: Option<&Path>,
    no_plan: bool,
    token: &str,
) -> Result<Built> {
    // Re-pick the plan on every rebuild so the view follows plan and apply changes.
    let picked = view::pick_plan(root, plan, no_plan);
    let mut run = view::run_with_plan(root, opts, &picked)?;
    let mut d = view::data(&run, root, opts);
    let _ = check::write_atomic(&root.join(".ig/view.html"), view::page(&d, None).as_bytes());
    let _ = std::fs::remove_file(root.join(view::FOCUS)); // a focus is shown once
    let page = view::page(&d, Some(token));
    let mut state = json!({"state": d["state"].take(), "plan": d["plan"].take()});
    if let (Some(p), Some(o)) = (&picked.0, state["plan"].as_object_mut()) {
        let rel = workspace::rel(root, p).unwrap_or_else(|| p.to_string_lossy().into());
        o.insert("path".into(), json!(rel));
    }
    let merged = std::mem::replace(&mut run.result.graph, Graph::empty());
    Ok(Built {
        plan: picked.0,
        state,
        merged,
        page,
    })
}

impl Server {
    fn broadcast(&self, event: &str, data: &Value) {
        let msg = format!("event: {event}\ndata: {data}\n\n");
        self.clients
            .lock()
            .unwrap()
            .retain_mut(|w| w.write_all(msg.as_bytes()).and_then(|_| w.flush()).is_ok());
    }

    /// Re-reads the notes and sends a `note` event for each new or changed one.
    fn sync_notes(&self) {
        let plan = self.built.lock().unwrap().plan.clone();
        let fresh = plan.as_deref().map(notes::read).unwrap_or_default();
        let mut last = self.notes.lock().unwrap();
        for n in fresh.iter().filter(|n| !last.contains(n)) {
            self.broadcast("note", &json!(n));
        }
        *last = fresh;
    }

    fn handle(&self, mut req: tiny_http::Request) {
        let url = req.url().split('?').next().unwrap_or("").to_string();
        let post = *req.method() == tiny_http::Method::Post;
        let header = |name: &str| {
            req.headers()
                .iter()
                .find(|h| h.field.as_str().as_str().eq_ignore_ascii_case(name))
                .map(|h| h.value.as_str().to_string())
        };
        // The Host check stops DNS rebinding; the token and Origin checks stop other pages
        // (and so prompt injection through notes).
        let hosts = [
            format!("127.0.0.1:{}", self.port),
            format!("localhost:{}", self.port),
        ];
        let host_ok = header("Host").is_some_and(|h| hosts.contains(&h));
        let origin_ok =
            header("Origin").is_none_or(|o| hosts.iter().any(|h| o == format!("http://{h}")));
        let token_ok = header("X-IG-Token").as_deref() == Some(self.token.as_str());
        if !host_ok || (post && !(origin_ok && token_ok)) {
            return respond(req, 403, &json!({"error": "forbidden"}));
        }
        let mut body = String::new();
        if post
            && req
                .as_reader()
                .take(1 << 20)
                .read_to_string(&mut body)
                .is_err()
        {
            return respond(req, 400, &json!({"error": "unreadable body"}));
        }
        match (post, url.as_str()) {
            (false, "/" | "/index.html") => {
                let html = self.built.lock().unwrap().page.clone();
                let h = |k: &str, v: &str| tiny_http::Header::from_bytes(k, v).unwrap();
                // The page holds the token: no other site may frame it.
                let _ = req.respond(
                    tiny_http::Response::from_string(html)
                        .with_header(h("Content-Type", "text/html; charset=utf-8"))
                        .with_header(h("X-Frame-Options", "DENY"))
                        .with_header(h("Content-Security-Policy", "frame-ancestors 'none'")),
                );
            }
            (false, "/api/events") => {
                let mut w = req.into_writer();
                let ok = w
                    .write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-cache\r\nConnection: close\r\n\r\n: connected\n\n")
                    .and_then(|_| w.flush())
                    .is_ok();
                if ok {
                    self.clients.lock().unwrap().push(w);
                }
            }
            (false, "/api/ping") => {
                respond(req, 200, &json!({"root": self.root.to_string_lossy()}))
            }
            (false, "/api/state") => {
                let mut v = self.built.lock().unwrap().state.clone();
                v["notes"] = json!(*self.notes.lock().unwrap());
                respond(req, 200, &v)
            }
            (false, "/api/notes") => {
                respond(req, 200, &json!({"notes": *self.notes.lock().unwrap()}))
            }
            (true, "/api/notes") => {
                let r = serde_json::from_str::<NoteInput>(&body)
                    .map_err(anyhow::Error::from)
                    .and_then(|input| {
                        let b = self.built.lock().unwrap();
                        let Some(plan) = &b.plan else {
                            bail!("no plan: run `ig plan check <file>` first");
                        };
                        notes::append(plan, input, &b.merged)
                    });
                match r {
                    Ok(n) => {
                        self.sync_notes();
                        respond(req, 200, &json!(n))
                    }
                    Err(e) => respond(req, 400, &json!({"error": format!("{e:#}")})),
                }
            }
            (true, "/api/focus") => match serde_json::from_str::<Value>(&body) {
                Ok(v)
                    if v["ids"]
                        .as_array()
                        .is_some_and(|a| a.iter().all(Value::is_string)) =>
                {
                    self.broadcast("focus", &v["ids"]);
                    respond(req, 200, &json!({}))
                }
                _ => respond(req, 400, &json!({"error": "expected {\"ids\": [..]}"})),
            },
            (true, u) if u.starts_with("/api/notes/") => {
                let id = &u["/api/notes/".len()..];
                let Ok(v) = serde_json::from_str::<Value>(&body)
                    .map_err(drop)
                    .and_then(|v| {
                        let ok = v.is_object()
                            && (v["done"].is_null() || v["done"].is_boolean())
                            && (v["reply"].is_null() || v["reply"].is_string());
                        if ok { Ok(v) } else { Err(()) }
                    })
                else {
                    return respond(
                        req,
                        400,
                        &json!({"error": "expected {\"done\": bool, \"reply\": string}"}),
                    );
                };
                let plan = self.built.lock().unwrap().plan.clone();
                let r = match &plan {
                    Some(p) => notes::set_done(
                        p,
                        id,
                        v["done"].as_bool().unwrap_or(true),
                        v["reply"].as_str(),
                    ),
                    None => Err(anyhow::anyhow!("no plan")),
                };
                match r {
                    Ok(()) => {
                        self.sync_notes();
                        let n = self
                            .notes
                            .lock()
                            .unwrap()
                            .iter()
                            .find(|n| n.id == id)
                            .cloned();
                        respond(req, 200, &json!(n))
                    }
                    Err(e) => respond(req, 400, &json!({"error": format!("{e:#}")})),
                }
            }
            _ => respond(req, 404, &json!({"error": "not found"})),
        }
    }
}

fn respond(req: tiny_http::Request, status: u16, v: &Value) {
    let h = tiny_http::Header::from_bytes("Content-Type", "application/json").unwrap();
    let _ = req.respond(
        tiny_http::Response::from_string(v.to_string())
            .with_status_code(status)
            .with_header(h),
    );
}

fn new_token() -> String {
    let mut seed = [0u8; 16];
    let _ = std::fs::File::open("/dev/urandom").and_then(|mut f| f.read_exact(&mut seed));
    let t = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    crate::model::sha1_hex(&format!("{seed:?}{t}{}", std::process::id()))
}

#[cfg(unix)]
mod on_exit {
    use std::ffi::{CString, c_char};
    use std::sync::OnceLock;

    static PATH: OnceLock<CString> = OnceLock::new();

    unsafe extern "C" {
        fn signal(sig: i32, handler: extern "C" fn(i32)) -> usize;
        fn unlink(path: *const c_char) -> i32;
        fn _exit(code: i32) -> !;
    }

    extern "C" fn on_signal(_: i32) {
        // unlink and _exit are async-signal-safe.
        if let Some(p) = PATH.get() {
            unsafe { unlink(p.as_ptr()) };
        }
        unsafe { _exit(130) }
    }

    /// Removes `path` when the process gets SIGINT, SIGTERM or SIGHUP.
    // ponytail: first path wins (one server per process); a stale file is harmless anyway since
    // `discover` pings the server.
    pub fn remove(path: &std::path::Path) {
        use std::os::unix::ffi::OsStrExt;
        if let Ok(c) = CString::new(path.as_os_str().as_bytes()) {
            let _ = PATH.set(c);
            unsafe {
                signal(2, on_signal);
                signal(15, on_signal);
                signal(1, on_signal);
            }
        }
    }
}

/// Serves `root` on 127.0.0.1:`port` (0: any free port) until killed. If the port is taken by an
/// `ig serve` for the same root, prints its URL and returns.
pub fn run(
    root: &Path,
    port: u16,
    opts: &Opts,
    plan: Option<PathBuf>,
    no_plan: bool,
    open_browser: bool,
) -> Result<()> {
    use notify::Watcher;
    let http = match tiny_http::Server::http(("127.0.0.1", port)) {
        Ok(s) => s,
        Err(e) => {
            if let Some(info) = discover(root).filter(|i| i.port == port) {
                println!("ig serve is already running: {}", info.url("/"));
                if open_browser {
                    view::open_str(&info.url("/"));
                }
                return Ok(());
            }
            bail!("port {port} is in use by another program ({e}); pick another with --port");
        }
    };
    let port = http
        .server_addr()
        .to_ip()
        .ok_or_else(|| anyhow::anyhow!("no address"))?
        .port();
    let token = new_token();
    let opts = opts.clone();
    let rebuild =
        move |root: &Path, token: &str| build(root, &opts, plan.as_deref(), no_plan, token);
    let s = Arc::new(Server {
        root: root.to_path_buf(),
        port,
        token: token.clone(),
        built: Mutex::new(rebuild(root, &token)?),
        notes: Mutex::new(vec![]),
        clients: Mutex::new(vec![]),
    });
    s.sync_notes();
    let info = ServeInfo {
        port,
        pid: std::process::id(),
        token,
    };
    let info_path = root.join(INFO);
    std::fs::create_dir_all(root.join(".ig"))?;
    // Created 0600 from the start: it holds the token.
    let _ = std::fs::remove_file(&info_path);
    let mut o = std::fs::OpenOptions::new();
    o.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut o, 0o600);
    o.open(&info_path)?
        .write_all(&serde_json::to_vec_pretty(&info)?)?;
    #[cfg(unix)]
    on_exit::remove(&info_path);
    let url = info.url("/");
    eprintln!("ig: serving {url} (Ctrl-C to stop)");
    if open_browser {
        view::open_str(&url);
    }

    let (tx, rx) = mpsc::channel::<notify::Result<notify::Event>>();
    let mut watcher = notify::recommended_watcher(move |e| {
        let _ = tx.send(e);
    })?;
    watcher.watch(root, notify::RecursiveMode::Recursive)?;
    {
        let s = s.clone();
        std::thread::spawn(move || {
            while let Ok(e) = rx.recv() {
                // A change to notes only re-reads them; anything else rebuilds.
                let (mut any, mut full) = (false, false);
                let mut see = |e: notify::Result<notify::Event>| {
                    for p in e
                        .iter()
                        .flat_map(|e| &e.paths)
                        .filter(|p| !view::ignored(&s.root, p))
                    {
                        any = true;
                        full |= !p
                            .file_name()
                            .is_some_and(|f| f.to_string_lossy().contains(".notes."));
                    }
                };
                see(e);
                // Debounce: wait for 300 ms of quiet.
                while let Ok(e) = rx.recv_timeout(Duration::from_millis(300)) {
                    see(e);
                }
                if !full {
                    if any {
                        s.sync_notes();
                    }
                    continue;
                }
                match rebuild(&s.root, &s.token) {
                    Ok(b) => {
                        *s.built.lock().unwrap() = b;
                        s.sync_notes();
                        s.broadcast("rebuild", &json!({}));
                        eprintln!("ig: rebuilt");
                    }
                    Err(err) => {
                        eprintln!("ig: rebuild failed, keeping the last good page: {err:#}")
                    }
                }
            }
        });
    }
    for req in http.incoming_requests() {
        s.handle(req);
    }
    drop(watcher);
    let _ = std::fs::remove_file(&info_path);
    Ok(())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::io::BufRead;
    use std::time::Instant;

    /// A scratch copy of the monorepo fixture with a checked plan, served on a free port.
    pub(crate) fn start(tag: &str) -> (PathBuf, ServeInfo) {
        let dir = crate::view::tests::scratch(tag);
        std::fs::create_dir_all(dir.join("plans")).unwrap();
        std::fs::write(dir.join("plans/export.md"), crate::plan::tests::PLAN).unwrap();
        crate::plan::check(&dir, &dir.join("plans/export.md")).unwrap();
        let d = dir.clone();
        std::thread::spawn(move || run(&d, 0, &Opts::default(), None, false, false));
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

    pub(crate) const COMMENT: &str = "packages/core/src/repo.ts#Repo.find";

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
}
