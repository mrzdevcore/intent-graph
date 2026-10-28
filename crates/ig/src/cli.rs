//! Command-line arguments.

use anyhow::Result;
use std::path::PathBuf;

pub const USAGE: &str = "usage: ig [--version] | init [claude|codex|all]
   | view [--no-open] [--json] [--read | --no-read] [--agent claude|gemini|codex] | mcp | plugin export <dir>
   (all accept --root <dir>)";

#[derive(Default)]
pub struct Args {
    pub cmd: String,
    pub sub: Option<String>,
    pub root: Option<PathBuf>,
    pub json: bool,
    pub no_open: bool,
    /// `--read`: an agent reads every box, without asking.
    pub read: bool,
    pub no_read: bool,
    pub agent: Option<String>,
    pub version: bool,
    pub more: Vec<String>,
}

pub fn parse_args() -> Result<Args> {
    use lexopt::prelude::*;
    let mut a = Args::default();
    let mut p = lexopt::Parser::from_env();
    while let Some(arg) = p.next()? {
        match arg {
            Long("version") | Short('V') => a.version = true,
            Long("help") | Short('h') => {
                println!("{USAGE}");
                std::process::exit(0);
            }
            Long("root") => a.root = Some(p.value()?.into()),
            Long("json") => a.json = true,
            Long("no-open") => a.no_open = true,
            Long("read") => a.read = true,
            Long("no-read") => a.no_read = true,
            Long("agent") => a.agent = Some(p.value()?.string()?),
            Value(v) if a.cmd.is_empty() => a.cmd = v.string()?,
            Value(v) if a.sub.is_none() => a.sub = Some(v.string()?),
            Value(v) if a.cmd == "plugin" => a.more.push(v.string()?),
            _ => return Err(arg.unexpected().into()),
        }
    }
    if a.read && a.no_read {
        anyhow::bail!("--read and --no-read cannot be used together\n{USAGE}");
    }
    Ok(a)
}
