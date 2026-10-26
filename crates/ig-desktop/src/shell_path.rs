//! PATH for the app and the agent: a Finder/Dock launch gets a bare PATH, so the app takes the
//! user's shell PATH at startup, and the adapter's PATH starts with the dir of the `ig` CLI that
//! matches the app (shipped next to the app's binary: `Contents/MacOS/ig`, or `target/<profile>/ig`).

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

const MARK: &str = "__IG_PATH__";

/// When not started from a terminal, take the interactive login shell's PATH, else put the usual
/// install dirs first. Call before any thread.
pub fn login_path() {
    if std::env::var_os("TERM").is_some() {
        return;
    }
    let path = shell_path().unwrap_or_else(|| {
        let old = std::env::var("PATH").unwrap_or_default();
        let cargo = std::env::var("HOME").map(|h| format!("{h}/.cargo/bin:"));
        format!(
            "{}/opt/homebrew/bin:/usr/local/bin:{old}",
            cargo.unwrap_or_default()
        )
    });
    // SAFETY: called first thing in `main`, before any other thread exists.
    unsafe { std::env::set_var("PATH", path) }
}

/// What follows the marker: a profile may print anything before it.
fn after_marker(out: &str) -> Option<String> {
    let p = out.rsplit_once(MARK)?.1.trim();
    (!p.is_empty()).then(|| p.to_string())
}

fn shell_path() -> Option<String> {
    probe(
        &std::env::var_os("SHELL")?,
        std::time::Duration::from_secs(3),
    )
}

/// `shell -ilc` printing PATH after the marker, all within `limit`: a profile's background job
/// may hold stdout open, so the read runs on a thread and the shell is killed and reaped at the end.
fn probe(shell: &OsStr, limit: std::time::Duration) -> Option<String> {
    use std::process::{Command, Stdio};
    let mut c = Command::new(shell)
        .args(["-ilc", &format!("printf '\\n{MARK}%s\\n' \"$PATH\"")])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let mut out = c.stdout.take()?;
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        // Line by line: the marker line is enough, even while a background job keeps the pipe open.
        let mut all = String::new();
        let mut r = std::io::BufReader::new(&mut out);
        let mut line = String::new();
        while std::io::BufRead::read_line(&mut r, &mut line).is_ok_and(|n| n > 0) {
            all.push_str(&line);
            if line.contains(MARK) {
                break;
            }
            line.clear();
        }
        let _ = tx.send(all);
    });
    let got = rx.recv_timeout(limit).ok();
    let _ = c.kill();
    let _ = c.wait();
    after_marker(&got?)
}

/// The dir of the `ig` CLI next to the app's own binary, if there is one.
pub fn ig_dir() -> Option<PathBuf> {
    let dir = std::env::current_exe().ok()?.parent()?.to_path_buf();
    dir.join("ig").is_file().then_some(dir)
}

/// `path` with `ig` (if any) first.
pub fn child_path(ig: Option<&Path>, path: &OsStr) -> OsString {
    let rest = std::env::split_paths(path);
    match ig {
        Some(d) => std::env::join_paths(std::iter::once(d.to_path_buf()).chain(rest)),
        None => std::env::join_paths(rest),
    }
    .unwrap_or_else(|_| path.to_os_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_path_is_what_follows_the_marker() {
        let out = format!("welcome!\nnvm: loaded\n{MARK}/a:/b\n");
        assert_eq!(after_marker(&out).as_deref(), Some("/a:/b"));
        assert_eq!(after_marker("no marker"), None);
        assert_eq!(after_marker(MARK), None);
    }

    #[cfg(unix)]
    #[test]
    fn a_background_job_holding_stdout_does_not_hang_the_probe() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("ig-shell-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let sh = dir.join("fake-shell");
        // Ignores its args like a noisy profile would, keeps stdout open with a background job.
        std::fs::write(
            &sh,
            format!("#!/bin/sh\necho welcome\nsleep 30 &\nprintf '\\n{MARK}/x:/y\\n'\nsleep 30\n"),
        )
        .unwrap();
        std::fs::set_permissions(&sh, std::fs::Permissions::from_mode(0o755)).unwrap();
        let t0 = std::time::Instant::now();
        let p = probe(sh.as_os_str(), std::time::Duration::from_secs(3));
        assert!(
            t0.elapsed() < std::time::Duration::from_secs(4),
            "{:?}",
            t0.elapsed()
        );
        assert_eq!(p.as_deref(), Some("/x:/y"));
        // Nothing printed: the deadline, then the fallback.
        std::fs::write(&sh, "#!/bin/sh\nsleep 30 &\nsleep 30\n").unwrap();
        let t0 = std::time::Instant::now();
        assert_eq!(
            probe(sh.as_os_str(), std::time::Duration::from_millis(500)),
            None
        );
        assert!(t0.elapsed() < std::time::Duration::from_secs(2));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_ig_dir_comes_first() {
        let p = child_path(Some(Path::new("/app/MacOS")), OsStr::new("/usr/bin:/bin"));
        assert_eq!(p, "/app/MacOS:/usr/bin:/bin");
        assert_eq!(child_path(None, OsStr::new("/usr/bin")), "/usr/bin");
    }
}
