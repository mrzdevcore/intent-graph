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

/// Runs `agent` in `root` with `prompt` on stdin and the user's environment; its stdout, or why
/// it failed (it is killed after 10 minutes). Its stderr goes to ours.
pub fn run(root: &Path, agent: Agent, prompt: &str) -> Result<String, String> {
    run_argv(root, agent.name(), agent.argv(), prompt, TIMEOUT)
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
