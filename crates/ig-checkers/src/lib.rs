//! The `CheckerRunner` adapter: runs a checker as `sh -c` and parses its output.

mod formats;

pub use formats::parse_output;

use ig_core::config::{Checker, Severity};
use ig_core::model::Finding;
use ig_core::ports::{CheckerOutput, CheckerRunner};
use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const MAX_COMMAND: usize = 100 << 10;
const TIMEOUT: Duration = Duration::from_secs(120);
const CAP: usize = 64 << 20;

/// Runs checkers as `sh -c <run>`: 120 s timeout, 64 MB output cap.
pub struct ProcessRunner;

impl CheckerRunner for ProcessRunner {
    fn run(&self, root: &Path, c: &Checker, files: Option<&[String]>) -> CheckerOutput {
        let mut out = CheckerOutput::default();
        let Some(run) = expand(c, files, &mut out.notices) else {
            return out;
        };
        let r = run_checker(root, &run).and_then(|(b, code, stderr)| {
            let tail = |t: &str| t.chars().take(300).collect::<String>();
            let found = parse_output(root, &c.name, c.format, &b)
                .map_err(|e| format!("{e}; stderr: {stderr}"))?;
            match code {
                Some(0) => Ok(found),
                _ if !found.is_empty() => Ok(found),
                _ => {
                    let msg = if stderr.trim().is_empty() {
                        tail(&String::from_utf8_lossy(&b))
                    } else {
                        stderr
                    };
                    let code = code.map_or("a signal".into(), |c| c.to_string());
                    Err(format!("exited {code} with no findings; stderr: {msg}"))
                }
            }
        });
        match r {
            Ok(f) => out.findings = f,
            Err(e) => out.errors.push(Finding::new(
                "ig/checker-error",
                Severity::Warning,
                &c.name,
                "ig.yaml",
                1,
                format!("checker {}: {e}", c.name),
            )),
        }
        out
    }
}

fn shell_quote(f: &str) -> String {
    // A leading `-` would read as a flag.
    let f = if f.starts_with('-') {
        format!("./{f}")
    } else {
        f.to_string()
    };
    format!("'{}'", f.replace('\'', r"'\''"))
}

/// The checker's `run` with `{files}` replaced by `files`, quoted; `.` when `files` is None
/// (`--all`) or the list is too long. None: nothing to check, skip it.
fn expand(c: &Checker, files: Option<&[String]>, notices: &mut Vec<String>) -> Option<String> {
    if !c.run.contains("{files}") {
        return Some(c.run.clone());
    }
    let whole = || Some(c.run.replace("{files}", "."));
    let Some(files) = files else {
        return whole();
    };
    if files.is_empty() {
        return None;
    }
    let quoted: Vec<String> = files.iter().map(|f| shell_quote(f)).collect();
    let cmd = c.run.replace("{files}", &quoted.join(" "));
    if cmd.len() > MAX_COMMAND {
        notices.push(format!(
            "checker {}: {} changed files make too long a command; checking the whole root",
            c.name,
            files.len()
        ));
        return whole();
    }
    Some(cmd)
}

fn read_capped(mut r: impl Read + Send + 'static) -> std::thread::JoinHandle<(Vec<u8>, bool)> {
    std::thread::spawn(move || {
        let mut buf = vec![];
        let mut chunk = [0u8; 65536];
        let mut over = false;
        loop {
            match r.read(&mut chunk) {
                Ok(0) | Err(_) => break,
                Ok(n) if buf.len() + n > CAP => {
                    over = true;
                    buf.extend_from_slice(&chunk[..CAP - buf.len()]);
                }
                Ok(n) if !over => buf.extend_from_slice(&chunk[..n]),
                Ok(_) => {}
            }
        }
        (buf, over)
    })
}

/// Runs one checker as `sh -c <run>` in `root`.
fn run_checker(root: &Path, run: &str) -> Result<(Vec<u8>, Option<i32>, String), String> {
    let mut child = Command::new("sh")
        .arg("-c")
        .arg(run)
        .current_dir(root)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| e.to_string())?;
    let out = read_capped(child.stdout.take().unwrap());
    let err = read_capped(child.stderr.take().unwrap());
    let start = Instant::now();
    let mut code = None;
    let timed_out = loop {
        match child.try_wait() {
            Ok(Some(st)) => {
                code = st.code();
                break false;
            }
            Ok(None) if start.elapsed() > TIMEOUT => {
                let _ = child.kill();
                let _ = child.wait();
                break true;
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(20)),
            Err(e) => return Err(e.to_string()),
        }
    };
    let (stdout, over) = out.join().unwrap_or_default();
    let (stderr, _) = err.join().unwrap_or_default();
    let stderr: String = String::from_utf8_lossy(&stderr).chars().take(300).collect();
    if timed_out {
        return Err(format!("timed out after 120 s; stderr: {stderr}"));
    }
    if over {
        return Err(format!("output exceeded 64 MB; stderr: {stderr}"));
    }
    Ok((stdout, code, stderr))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ig_core::config::Config;
    use std::path::PathBuf;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("ig-checkers-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn checker(yaml: &str) -> Checker {
        Config::parse(&format!("checkers:\n  - name: t\n{yaml}"))
            .unwrap()
            .checkers
            .remove(0)
    }

    #[test]
    fn process_runner_meets_the_contract() {
        let d = tmp("contract");
        ig_core::testing::runner_contract(&ProcessRunner, &d, &ig_core::testing::files_checker());
        // A `{files}` checker that would fail if it ran.
        let c = checker("    format: lines\n    run: \"exit 1 {files}\"\n");
        let out = ProcessRunner.run(&d, &c, Some(&[]));
        std::fs::remove_dir_all(&d).ok();
        assert!(out.errors.is_empty() && out.findings.is_empty());
    }

    #[test]
    fn lines_checker_ignores_exit_status() {
        let d = tmp("lines");
        let c = checker("    format: lines\n    run: \"echo 'src/a.ts:1: boom'; exit 3\"\n");
        let out = ProcessRunner.run(&d, &c, None);
        std::fs::remove_dir_all(&d).ok();
        assert!(out.errors.is_empty(), "{:?}", out.errors);
        assert_eq!(out.findings.len(), 1);
        assert_eq!(out.findings[0].node_id, None, "callers attach nodes");
    }

    #[test]
    fn broken_checker_is_reported() {
        let d = tmp("broken");
        let run = |cmd: &str| {
            let c = checker(&format!("    format: lines\n    run: \"{cmd}\"\n"));
            ProcessRunner.run(&d, &c, None)
        };
        let bad = run("echo boom >&2; exit 127");
        let ok = run("exit 0");
        std::fs::remove_dir_all(&d).ok();
        assert_eq!(bad.errors.len(), 1);
        assert!(
            bad.errors[0].message.contains("boom"),
            "{}",
            bad.errors[0].message
        );
        assert!(bad.errors[0].message.contains("exited 127"));
        assert!(ok.errors.is_empty() && ok.findings.is_empty());
    }

    #[test]
    fn files_placeholder() {
        let files: Vec<String> = ["src/it's a.ts", "b.vue", "-x.ts"].map(String::from).into();
        let mut notes = vec![];
        let lint = checker("    run: \"lint {files}\"\n");
        assert_eq!(
            expand(&lint, Some(&files), &mut notes).unwrap(),
            r#"lint 'src/it'\''s a.ts' 'b.vue' './-x.ts'"#
        );
        assert!(expand(&lint, Some(&[]), &mut notes).is_none());
        assert_eq!(expand(&lint, None, &mut notes).unwrap(), "lint .");
        let plain = checker("    run: \"lint\"\n");
        assert_eq!(expand(&plain, Some(&[]), &mut notes).unwrap(), "lint");
        let many: Vec<String> = (0..20000).map(|i| format!("f{i}.ts")).collect();
        assert_eq!(expand(&lint, Some(&many), &mut notes).unwrap(), "lint .");
        assert_eq!(notes.len(), 1, "{notes:?}");
    }
}
