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
    use std::process::{Command, Stdio};
    let shell = std::env::var_os("SHELL")?;
    let mut c = Command::new(shell)
        .args(["-ilc", &format!("printf '\\n{MARK}%s' \"$PATH\"")])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let t0 = std::time::Instant::now();
    while c.try_wait().ok()?.is_none() {
        if t0.elapsed() > std::time::Duration::from_secs(3) {
            let _ = c.kill();
            return None;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    let mut out = String::new();
    std::io::Read::read_to_string(&mut c.stdout?, &mut out).ok()?;
    after_marker(&out)
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

    #[test]
    fn the_ig_dir_comes_first() {
        let p = child_path(Some(Path::new("/app/MacOS")), OsStr::new("/usr/bin:/bin"));
        assert_eq!(p, "/app/MacOS:/usr/bin:/bin");
        assert_eq!(child_path(None, OsStr::new("/usr/bin")), "/usr/bin");
    }
}
