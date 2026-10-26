//! Command-line arguments.

use anyhow::Result;
use ig_app::check;
use std::path::PathBuf;

pub const USAGE: &str =
    "usage: ig [--version] | init [claude|codex|all] | setup [--json] | setup claude|codex
   | check [--base <ref>] [--head <ref>] [--all] [--json] [--agent]
   | brief [--base <ref>] [--head <ref>] [--md] [--out <file>]
   | plan check <file> [--json] | plan notes [--wait] [--json] | apply start [<plan>] | apply accept | apply review | apply finish | apply abort
   | except <fingerprint> --reason <text>
   | query <id> [--in|--out|--both] [--depth N] [--json] | focus <id>...
   | review [--all] [--json] [--agent] | diff [--base <ref>] [--head <ref>] [--json]
   | view [--base <ref>] [--head <ref>] [--watch] [--no-open] [--json] [--checkers] [--plan <file> | --no-plan] | report --gitlab | mcp | hook claude
   | plugin export <dir> | serve [--port N] [--checkers] [--plan <file> | --no-plan] | serve claude
   (all accept --root <dir>)";

#[derive(Default)]
pub struct Args {
    pub cmd: String,
    pub sub: Option<String>,
    pub root: Option<PathBuf>,
    pub base: Option<String>,
    pub head: Option<String>,
    pub all: bool,
    pub json: bool,
    pub agent: bool,
    pub watch: bool,
    pub no_open: bool,
    pub gitlab: bool,
    pub version: bool,
    pub md: bool,
    pub out: Option<PathBuf>,
    pub dir: Option<String>,
    pub depth: Option<usize>,
    pub more: Vec<String>,
    pub plan: Option<PathBuf>,
    pub no_plan: bool,
    pub reason: Option<String>,
    pub port: Option<u16>,
    pub wait: bool,
    pub checkers: bool,
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
            Long("base") => a.base = Some(p.value()?.string()?),
            Long("head") => a.head = Some(p.value()?.string()?),
            Long("all") => a.all = true,
            Long("json") => a.json = true,
            Long("agent") => a.agent = true,
            Long("watch") => a.watch = true,
            Long("no-open") => a.no_open = true,
            Long("gitlab") => a.gitlab = true,
            Long("md") => a.md = true,
            Long("plan") => a.plan = Some(p.value()?.into()),
            Long("no-plan") => a.no_plan = true,
            Long("reason") => a.reason = Some(p.value()?.string()?),
            Long("port") => a.port = Some(p.value()?.parse()?),
            Long("wait") => a.wait = true,
            Long("checkers") => a.checkers = true,
            Long("out") if a.cmd == "query" => a.dir = Some("out".into()),
            Long("out") => a.out = Some(p.value()?.into()),
            Value(v) if a.cmd.is_empty() => a.cmd = v.string()?,
            Long("in") => a.dir = Some("in".into()),
            Long("both") => a.dir = Some("both".into()),
            Long("depth") => a.depth = Some(p.value()?.parse()?),
            Value(v) if a.sub.is_none() => a.sub = Some(v.string()?),
            Value(v) if matches!(a.cmd.as_str(), "focus" | "plan" | "apply" | "plugin") => {
                a.more.push(v.string()?)
            }
            _ => return Err(arg.unexpected().into()),
        }
    }
    Ok(a)
}

pub fn opts(a: &Args) -> check::Opts {
    check::Opts {
        base: a.base.clone(),
        head: a.head.clone(),
        all: a.all,
        skip_checkers: matches!(a.cmd.as_str(), "view" | "serve") && !a.checkers,
    }
}
