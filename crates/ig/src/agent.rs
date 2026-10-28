//! Running an AI agent for `ig view` (the I/O side of `ig_app::map::agent`).

use ig_app::map::agent::Agent;
use std::io::{BufRead, IsTerminal, Read, Write};
use std::path::Path;
use std::process::{Command, Stdio};
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
    eprint!(
        "Let an AI agent ({}) read the architecture? [Y/n] ",
        agent.name()
    );
    let _ = std::io::stderr().flush();
    let mut line = String::new();
    let _ = std::io::stdin().lock().read_line(&mut line);
    !line.trim().to_lowercase().starts_with('n')
}

/// Runs `agent` in `root` with `prompt` on stdin and the user's environment; its stdout, or why
/// it failed (it is killed after 10 minutes). Its stderr goes to ours.
pub fn run(root: &Path, agent: Agent, prompt: &str) -> Result<String, String> {
    let argv = agent.argv();
    let name = agent.name();
    let mut child = Command::new(argv[0])
        .args(&argv[1..])
        .current_dir(root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .map_err(|e| format!("{name} could not start: {e}"))?;
    let mut stdin = child.stdin.take().unwrap();
    let prompt = prompt.to_string();
    // written from a thread: a big prompt must not block on a full pipe while we wait
    std::thread::spawn(move || stdin.write_all(prompt.as_bytes()));
    let mut stdout = child.stdout.take().unwrap();
    let out = std::thread::spawn(move || {
        let mut s = String::new();
        stdout.read_to_string(&mut s).map(|_| s)
    });
    let start = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(s)) => break s,
            Ok(None) if start.elapsed() > TIMEOUT => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("{name} took over 10 minutes; stopped"));
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(100)),
            Err(e) => return Err(format!("{name}: {e}")),
        }
    };
    let text = out
        .join()
        .map_err(|_| format!("{name}: reading its answer failed"))?
        .map_err(|e| format!("{name}: {e}"))?;
    if !status.success() {
        return Err(format!("{name} failed ({status})"));
    }
    Ok(text)
}
