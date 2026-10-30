//! `ig app [dir]`, `ig <dir>`: the desktop app on a project.

use anyhow::{Context, Result, bail};
use std::path::Path;
use std::process::{Command, Stdio};

pub fn open(dir: &Path) -> Result<()> {
    if !dir.is_dir() {
        bail!("{}: not a directory", dir.display());
    }
    let dir = dir.canonicalize()?;
    command(&dir)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .context("the desktop app did not start (set IG_APP to its binary)")?;
    Ok(())
}

/// `IG_APP`, else the app built next to this `ig` (`target/<profile>/`, the bundle's
/// `Contents/MacOS/`), else the installed app.
fn command(dir: &Path) -> Command {
    let near = std::env::current_exe().ok().and_then(|e| {
        let p = e
            .parent()?
            .join(format!("ig-desktop{}", std::env::consts::EXE_SUFFIX));
        p.is_file().then_some(p)
    });
    let bin = std::env::var_os("IG_APP").map(Into::into).or(near);
    let mut c = match bin {
        Some(b) => Command::new(b),
        // -n: a running app keeps its project; this one opens `dir`
        None if cfg!(target_os = "macos") => {
            let mut c = Command::new("open");
            c.args(["-n", "-a", "ig", "--args"]);
            c
        }
        None => Command::new("ig-desktop"),
    };
    c.arg(dir);
    c
}
