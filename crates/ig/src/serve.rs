//! `ig serve`: the live viewer on 127.0.0.1:<port>, a JSON API for the viewer and `ig mcp`, and a
//! server-sent event stream (`rebuild`, `note`, `focus`, `show`). Every POST needs the run's token.

use crate::CWD;
use crate::model::Graph;
use crate::render::{self, open_str};
use anyhow::{Result, bail};
use ig_app::check::Opts;
use ig_app::notes::{self, Note, NoteInput};
use ig_app::view_data;
use ig_core::workspace;
use ig_wire::{Wired, shown_plan};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
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
    /// The page's data (`view::data`), for `/api/view`.
    view: Value,
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
    let w = Wired::new(root);
    let env = w.env();
    let picked = view_data::pick_plan(&env, plan, no_plan);
    let mut run = view_data::run_with_plan(&env, opts, shown_plan(root, &picked))?;
    let mut d = view_data::data(&env, &run, opts);
    let _ = ig_fs::write_atomic(
        &root.join(".ig/view.html"),
        render::page(&d, None).as_bytes(),
    );
    let _ = std::fs::remove_file(root.join(view_data::FOCUS)); // a focus is shown once
    let page = render::page(&d, Some(token));
    let view = d.clone();
    let mut state = json!({"state": d["state"].take(), "plan": d["plan"].take()});
    if let (Some(p), Some(o)) = (&picked.0, state["plan"].as_object_mut()) {
        let rel = workspace::rel(root, p).unwrap_or_else(|| p.to_string_lossy().into());
        o.insert("path".into(), json!(rel));
    }
    let merged = std::mem::replace(&mut run.result.graph, Graph::empty());
    Ok(Built {
        plan: picked.0,
        state,
        view,
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
        let fresh = plan
            .as_deref()
            .map(|p| notes::read(&CWD, p))
            .unwrap_or_default();
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
            // Not token-gated, like every GET: it is what the page already embeds.
            (false, "/api/view") => {
                let v = self.built.lock().unwrap().view.clone();
                respond(req, 200, &v)
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
                        Ok(notes::append(&CWD, plan, input, &b.merged)?)
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
            // From `ig_show`, which checked the ids: only the shape is checked here.
            (true, "/api/show") => match serde_json::from_str::<ig_app::show::Show>(&body) {
                Ok(show) => {
                    self.broadcast("show", &json!(show));
                    respond(req, 200, &json!({}))
                }
                Err(e) => respond(req, 400, &json!({"error": format!("expected a show: {e}")})),
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
                        &CWD,
                        p,
                        id,
                        v["done"].as_bool().unwrap_or(true),
                        v["reply"].as_str(),
                    ),
                    None => Err(anyhow::anyhow!("no plan").into()),
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

/// Binds 127.0.0.1:`port`. None: our `ig serve` for this root already holds it. Another program
/// on it is an error for an `explicit` port; otherwise any free port is taken, with a notice.
fn bind(root: &Path, port: u16, explicit: bool) -> Result<Option<tiny_http::Server>> {
    match tiny_http::Server::http(("127.0.0.1", port)) {
        Ok(s) => Ok(Some(s)),
        Err(_) if discover(root).is_some_and(|i| i.port == port) => Ok(None),
        Err(e) if explicit => {
            bail!("port {port} is in use by another program ({e}); pick another with --port")
        }
        Err(_) => {
            eprintln!("ig: port {port} is in use by another program; using a free port");
            tiny_http::Server::http(("127.0.0.1", 0))
                .map_err(|e| anyhow::anyhow!("{e}"))
                .map(Some)
        }
    }
}

/// Serves `root` on 127.0.0.1:`port` (default 7420; 0: any free port) until killed. If the port is
/// taken by an `ig serve` for the same root, prints its URL and returns.
pub fn run(
    root: &Path,
    port: Option<u16>,
    opts: &Opts,
    plan: Option<PathBuf>,
    no_plan: bool,
    open_browser: bool,
) -> Result<()> {
    let Some(http) = bind(root, port.unwrap_or(PORT), port.is_some())? else {
        if let Some(info) = discover(root) {
            println!("ig serve is already running: {}", info.url("/"));
            if open_browser {
                open_str(&info.url("/"));
            }
        }
        return Ok(());
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
        open_str(&url);
    }

    // A change to notes only re-reads them; anything else rebuilds.
    let watcher = {
        let s = s.clone();
        ig_wire::watch(root, move |c| {
            if !c.full {
                return s.sync_notes();
            }
            match rebuild(&s.root, &s.token) {
                Ok(b) => {
                    *s.built.lock().unwrap() = b;
                    s.sync_notes();
                    s.broadcast("rebuild", &json!({}));
                    eprintln!("ig: rebuilt");
                }
                Err(err) => eprintln!("ig: rebuild failed, keeping the last good page: {err:#}"),
            }
        })?
    };
    for req in http.incoming_requests() {
        s.handle(req);
    }
    drop(watcher);
    let _ = std::fs::remove_file(&info_path);
    Ok(())
}

#[cfg(test)]
pub(crate) mod tests;
