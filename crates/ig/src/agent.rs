//! Running an AI agent for `ig view` (the I/O side of `ig_app::map::agent`).

use ig_app::map::agent::Agent;
use std::io::{BufRead, IsTerminal, Read, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// The template `ig_app::map::agent::read` fills.
pub const PROMPT: &str = include_str!("../assets/prompts/read.md");
const TIMEOUT: Duration = Duration::from_secs(10 * 60);

/// `name` is a file in a PATH directory.
// ponytail: no executable bit or Windows extensions (.exe, .cmd) checked; add if they misfire
pub fn on_path(name: &str) -> bool {
    std::env::var_os("PATH")
        .is_some_and(|path| std::env::split_paths(&path).any(|d| d.join(name).is_file()))
}

/// Both stdin and stdout are a terminal: a person can be asked.
pub fn interactive() -> bool {
    std::io::stdin().is_terminal() && std::io::stdout().is_terminal()
}

/// `Let an AI agent ({agent}) read the architecture? [Y/n]`; yes unless the answer starts with n.
pub fn confirm(agent: Agent) -> bool {
    eprintln!("{}", agent.note());
    eprint!(
        "Let an AI agent ({}) read the architecture? [Y/n] ",
        agent.name()
    );
    let _ = std::io::stderr().flush();
    let mut line = String::new();
    let _ = std::io::stdin().lock().read_line(&mut line);
    !line.trim().to_lowercase().starts_with('n')
}

/// `ig view --read`'s status line on stderr, on a terminal only: a spinner, boxes asked about of
/// all to read as a bar, the time so far and what is happening now, redrawn in place so a long ask
/// does not look stuck. Progress lines print above it (ig-wire clears its line first); dropped
/// unfinished, it is wiped.
pub struct Bar {
    state: Arc<Mutex<Meter>>,
    tty: bool,
}

#[derive(Default)]
struct Meter {
    done: usize,
    total: usize,
    status: String,
    start: Option<Instant>,
    /// Spinner frames drawn so far.
    turns: usize,
    over: bool,
}

const BAR: usize = 20;
const SPIN: [char; 10] = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];

impl Meter {
    /// `ig: ⠹ ███████░░░░░░░░░░░░░ 20/55 boxes · 1:12 · claude is reading…`; done, a ✓.
    fn line(&self) -> String {
        let fill = self.done.min(self.total) * BAR / self.total.max(1);
        let t = self.start.map_or(0, |s| s.elapsed().as_secs());
        let spin = if self.over {
            '✓'
        } else {
            SPIN[self.turns % SPIN.len()]
        };
        format!(
            "ig: {spin} {}{} {}/{} boxes · {}:{:02} · {}",
            "█".repeat(fill),
            "░".repeat(BAR - fill),
            self.done,
            self.total,
            t / 60,
            t % 60,
            self.status
        )
    }

    fn draw(&self) {
        eprint!("\r\x1b[2K{}", self.line());
        let _ = std::io::stderr().flush();
    }
}

impl Bar {
    pub fn new() -> Bar {
        Bar {
            state: Arc::default(),
            tty: std::io::stderr().is_terminal(),
        }
    }

    /// `done` boxes of `total`, and what is happening now; the first call starts the clock, the
    /// last (all done) ends the line.
    pub fn set(&self, done: usize, total: usize, status: &str) {
        if !self.tty || total == 0 {
            return;
        }
        let mut m = self.state.lock().unwrap();
        if m.start.is_none() {
            m.start = Some(Instant::now());
            let state = Arc::clone(&self.state);
            std::thread::spawn(move || {
                loop {
                    std::thread::sleep(Duration::from_millis(100));
                    let mut m = state.lock().unwrap();
                    if m.over {
                        break;
                    }
                    m.turns += 1;
                    m.draw();
                }
            });
        }
        (m.done, m.total, m.status) = (done, total, status.to_string());
        m.over = done >= total;
        m.draw();
        if m.over {
            eprintln!();
        }
    }
}

impl Drop for Bar {
    fn drop(&mut self) {
        let mut m = self.state.lock().unwrap();
        if m.start.is_some() && !m.over {
            m.over = true;
            eprint!("\r\x1b[2K");
        }
    }
}

/// Runs `agent` in `root` with `prompt` on stdin and the user's environment; its stdout, or why
/// it failed (it is killed after 10 minutes). Its stderr goes to ours. Ollama is asked over HTTP
/// instead, its answer held to the boxes `ids`.
pub fn run(root: &Path, agent: Agent, prompt: &str, ids: &[String]) -> Result<String, String> {
    match agent {
        Agent::Ollama => ollama(prompt, ids),
        _ => run_argv(root, agent.name(), agent.argv(), prompt, TIMEOUT),
    }
}

/// The model `--agent ollama` uses unless `IG_OLLAMA_MODEL` names another.
pub const OLLAMA_MODEL: &str = "qwen2.5-coder:7b";
/// Tokens of context asked for: Ollama's default (4k at most) would cut the prompt silently.
const OLLAMA_CONTEXT: u32 = 32_768;

/// The answer's shape, for Ollama's structured output: `{"annotations": [...]}` (an object at the
/// top, as its docs show; ig takes the array out), each `id` one of `ids`: a small model copies the
/// prompt's example ids otherwise.
fn schema(ids: &[String]) -> serde_json::Value {
    let text = serde_json::json!({"type": "string"});
    serde_json::json!({
        "type": "object",
        "required": ["annotations"],
        "properties": {"annotations": {"type": "array", "items": {
            "type": "object",
            "required": ["id", "title", "kind", "description"],
            "properties": {
                "id": {"enum": ids}, "title": text, "description": text, "tech": text,
                "kind": {"enum": ["person", "system", "external", "app", "api", "worker", "store",
                    "queue", "library", "entry", "usecase", "port", "domain", "group"]},
                "confidence": {"enum": ["extracted", "inferred", "ambiguous"]},
            },
        }}},
    })
}

/// `prompt` posted to the Ollama server (`OLLAMA_HOST`, else 127.0.0.1:11434); its answer.
// ponytail: plain HTTP/1.0 over a socket, so no HTTP client crate; a local server only
fn ollama(prompt: &str, ids: &[String]) -> Result<String, String> {
    use std::net::TcpStream;
    let host = std::env::var("OLLAMA_HOST").unwrap_or_else(|_| "127.0.0.1:11434".into());
    let host = host.trim_start_matches("http://").trim_end_matches('/');
    let model = std::env::var("IG_OLLAMA_MODEL").unwrap_or_else(|_| OLLAMA_MODEL.into());
    let body = serde_json::json!({
        "model": model,
        "prompt": prompt,
        "stream": false,
        // decoding held to this shape: a small model's answer is then always an array
        "format": schema(ids),
        "options": {"num_ctx": OLLAMA_CONTEXT, "temperature": 0.2},
    })
    .to_string();
    let mut sock = TcpStream::connect(host).map_err(|e| {
        format!("ollama is not running at {host} ({e}): start it with `ollama serve`")
    })?;
    let _ = sock.set_read_timeout(Some(TIMEOUT));
    let req = format!(
        "POST /api/generate HTTP/1.0\r\nHost: {host}\r\nContent-Type: application/json\r\n\
         Content-Length: {}\r\n\r\n{body}",
        body.len()
    );
    sock.write_all(req.as_bytes())
        .map_err(|e| format!("ollama: {e}"))?;
    let mut raw = String::new();
    sock.read_to_string(&mut raw)
        .map_err(|e| format!("ollama: {e}"))?;
    let (head, json) = raw.split_once("\r\n\r\n").unwrap_or((&raw, ""));
    let v: serde_json::Value = serde_json::from_str(json)
        .map_err(|_| format!("ollama: {}", head.lines().next().unwrap_or("")))?;
    if let Some(e) = v["error"].as_str() {
        let hint = if e.contains("not found") {
            format!(" (run `ollama pull {model}`)")
        } else {
            String::new()
        };
        return Err(format!("ollama: {e}{hint}"));
    }
    // the schema leaves no room for the tags the prompt asks for
    v["response"]
        .as_str()
        .map(|r| format!("<ig-annotations>{r}</ig-annotations>"))
        .ok_or_else(|| "ollama: no response in its answer".into())
}

/// `argv` run as `run` says, in its own process group: when it ends or times out, the whole
/// group is killed, so its helpers (workers, shells, MCP servers) neither outlive it nor hold
/// its output open past `timeout`.
fn run_argv(
    root: &Path,
    name: &str,
    argv: &[&str],
    prompt: &str,
    timeout: Duration,
) -> Result<String, String> {
    let deadline = Instant::now() + timeout;
    let mut cmd = Command::new(argv[0]);
    cmd.args(&argv[1..])
        .current_dir(root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped());
    #[cfg(unix)]
    std::os::unix::process::CommandExt::process_group(&mut cmd, 0);
    let mut child = cmd
        .spawn()
        .map_err(|e| format!("{name} could not start: {e}"))?;
    let mut stdin = child.stdin.take().unwrap();
    let prompt = prompt.to_string();
    // written from a thread: a big prompt must not block on a full pipe while we wait
    std::thread::spawn(move || stdin.write_all(prompt.as_bytes()));
    let mut stdout = child.stdout.take().unwrap();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut s = String::new();
        let _ = tx.send(stdout.read_to_string(&mut s).map(|_| s));
    });
    let minutes = timeout.as_secs() / 60;
    let late = || format!("{name} took over {minutes} minutes; stopped");
    let status = loop {
        match child.try_wait() {
            Ok(Some(s)) => break s,
            Ok(None) if Instant::now() > deadline => {
                kill_group(&mut child);
                return Err(late());
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(50)),
            Err(e) => {
                kill_group(&mut child);
                return Err(format!("{name}: {e}"));
            }
        }
    };
    // what it left running would keep its output open
    kill_group(&mut child);
    let text = match rx.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
        Ok(r) => r.map_err(|e| format!("{name}: {e}"))?,
        Err(_) => return Err(late()),
    };
    if !status.success() {
        return Err(format!("{name} failed ({status})"));
    }
    Ok(text)
}

/// Kills `child`'s process group (it leads it) and reaps `child`.
fn kill_group(child: &mut std::process::Child) {
    // `kill` is a shell builtin: no libc dependency, and no PATH lookup
    #[cfg(unix)]
    let _ = Command::new("/bin/sh")
        .args(["-c", &format!("kill -KILL -- -{} 2>/dev/null", child.id())])
        .status();
    let _ = child.kill();
    let _ = child.wait();
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn the_bar_fills_with_the_boxes_asked_about() {
        let m = |done, total, over| {
            let status = "claude is reading…".to_string();
            Meter {
                done,
                total,
                over,
                status,
                ..Meter::default()
            }
            .line()
        };
        let empty = "░".repeat(BAR);
        assert_eq!(
            m(0, 55, false),
            format!("ig: ⠋ {empty} 0/55 boxes · 0:00 · claude is reading…")
        );
        assert!(m(55, 55, true).starts_with(&format!("ig: ✓ {} 55/55", "█".repeat(BAR))));
        assert!(m(20, 55, false).starts_with(&format!("ig: ⠋ {}░", "█".repeat(7))));
    }

    /// A fake agent whose helper (a grandchild) holds stdout: ig returns by the deadline and
    /// the helper is gone.
    #[test]
    fn a_timeout_kills_the_whole_group() {
        let dir = crate::tests::tmp_with(&[]);
        std::fs::create_dir_all(&dir).unwrap();
        let pid = dir.join("helper.pid");
        let script = |rest: &str| {
            format!(
                "/bin/sleep 30 & echo $! > {}; echo partial; {rest}",
                pid.display()
            )
        };
        let alive = |pid: &str| {
            Command::new("/bin/sh")
                .args(["-c", &format!("kill -0 {pid} 2>/dev/null")])
                .status()
                .unwrap()
                .success()
        };
        let gone = |pid: &str| {
            (0..40).any(|_| {
                std::thread::sleep(Duration::from_millis(50));
                !alive(pid)
            })
        };
        // the agent hangs: stopped at the deadline
        let start = Instant::now();
        let argv = ["/bin/sh", "-c", &script("/bin/sleep 30")];
        let r = run_argv(&dir, "fake", &argv, "", Duration::from_secs(1));
        assert!(r.unwrap_err().contains("stopped"));
        assert!(
            start.elapsed() < Duration::from_secs(5),
            "{:?}",
            start.elapsed()
        );
        let helper = std::fs::read_to_string(&pid).unwrap();
        assert!(gone(helper.trim()), "the helper outlived the agent");
        // the agent ends but leaves its helper holding stdout: its answer, right away
        let start = Instant::now();
        let argv = ["/bin/sh", "-c", &script("exit 0")];
        let r = run_argv(&dir, "fake", &argv, "", Duration::from_secs(20));
        assert_eq!(r.unwrap(), "partial\n");
        assert!(
            start.elapsed() < Duration::from_secs(5),
            "{:?}",
            start.elapsed()
        );
        let helper = std::fs::read_to_string(&pid).unwrap();
        assert!(gone(helper.trim()), "the helper outlived the agent");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
