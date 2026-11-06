mod accept;
mod agent;
mod analyse;
mod app;
mod cli;
mod explore;
mod init;
mod mcp;
mod plan;
mod render;
mod skills;
#[cfg(test)]
mod tests;

use anyhow::{Result, bail};
use clap::CommandFactory;
use cli::{Cli, Cmd, InitAgents, PluginCmd, ViewArgs};
use ig_app::map::{edit, origin, spec, view};
// `crate::model` for the modules (and their tests) that name model types.
use ig_core::model;
use ig_wire::Wired;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

fn say(quiet: bool, line: &str) {
    if !quiet {
        eprintln!("{line}");
    }
}

fn say_all(quiet: bool, v: &view::View) {
    for n in &v.notices {
        say(quiet, &format!("ig: {n}"));
    }
    for w in &v.warnings {
        say(quiet, &format!("warning: {w}"));
    }
}

fn run_view(root: &Path, a: &ViewArgs, quiet: bool) -> Result<u8> {
    if a.read || a.agent.is_some() {
        eprintln!("ig: the reading moved to `ig analyse`");
        return Ok(2);
    }
    if a.no_read {
        say(
            quiet,
            "ig: --no-read is the default now; run `ig init` to update the skills",
        );
    }
    let w = Wired::new(root);
    let v = match ig_wire::landscape(root) {
        Some(v) => v,
        None => view::run_live(&w.env(), ig_wire::CONNECTORS)?,
    };
    say_all(quiet, &v);
    if a.json {
        println!("{}", serde_json::to_string_pretty(&v)?);
    } else {
        let path = render::write(&w, &v)?;
        if !a.no_open {
            render::open(&path);
        }
        println!("{}", path.display());
    }
    // after the page: `.ig/` may not exist before it
    render::ignore_ig_dir(root)?;
    Ok(0)
}

const NO_SPEC: &str = "ig: no ig/ yet: run `ig analyse`";

fn has_spec(env: &ig_app::Env) -> Result<bool> {
    Ok(!env
        .files
        .glob(&[format!("{}/*.kdl", spec::DIR)])?
        .is_empty())
}

fn real_main() -> Result<u8> {
    let cli = cli::parse();
    let quiet = cli.quiet;
    let Some(command) = cli.command else {
        eprintln!("{}", cli::usage());
        return Ok(2);
    };
    match &command {
        Cmd::Hook { .. } => {
            eprintln!("ig hook was removed; delete ig's Stop hook from .claude/settings.json");
            return Ok(0);
        }
        Cmd::Completions { shell } => {
            clap_complete::generate(*shell, &mut Cli::command(), "ig", &mut std::io::stdout());
            return Ok(0);
        }
        Cmd::Plugin(PluginCmd::Export { dir }) => {
            // Prints each file written, relative to <dir> (the app prunes anything else).
            for p in skills::export(dir)? {
                println!("{}", p.strip_prefix(dir).unwrap_or(&p).display());
            }
            return Ok(0);
        }
        _ => {}
    }
    let root = cli.root.clone().unwrap_or_else(|| PathBuf::from("."));
    if !root.is_dir() {
        bail!("-C {}: not a directory", root.display());
    }
    let root = root.canonicalize()?;
    let w = Wired::new(&root);
    let env = w.env();
    match command {
        Cmd::View(a) => run_view(&root, &a, quiet),
        Cmd::Analyse(a) => analyse::run(&root, &a, quiet),
        Cmd::Explore(a) => explore::run(&root, &a, quiet),
        Cmd::Plan { json } | Cmd::Check { json } => {
            let check = matches!(command, Cmd::Check { .. });
            let name = if check { "check" } else { "plan" };
            if !has_spec(&env)? {
                eprintln!("{NO_SPEC}");
                return Ok(2);
            }
            let v = view::run(&env)?;
            say_all(quiet, &v);
            if v.broken {
                return Ok(2);
            }
            let unread = view::unread(&env, &v, &ig_fs::BUILD_DIRS)?;
            let p = origin::plan_of(&v.boxes, &v.arrows, &unread);
            if json {
                println!("{}", serde_json::to_string_pretty(&plan::json(&p))?);
            } else {
                println!("{}", p.text(name));
            }
            Ok(u8::from(check && !p.is_empty()))
        }
        Cmd::Fmt { check } => {
            if !has_spec(&env)? {
                eprintln!("{NO_SPEC}");
                return Ok(2);
            }
            match edit::fmt(env.files, !check) {
                Ok(changed) => {
                    let verb = if check { "would format" } else { "formatted" };
                    changed.iter().for_each(|p| println!("{verb} {p}"));
                    Ok(u8::from(check && !changed.is_empty()))
                }
                Err(errs) => {
                    errs.iter().for_each(|e| eprintln!("ig: {e}"));
                    Ok(2)
                }
            }
        }
        Cmd::Accept { all, ids } => {
            if !has_spec(&env)? {
                eprintln!("{NO_SPEC}");
                return Ok(2);
            }
            let v = view::run(&env)?;
            if v.broken {
                v.notices.iter().for_each(|n| eprintln!("ig: {n}"));
                return Ok(2);
            }
            accept::run(env.files, &v, all, &ids)
        }
        Cmd::Mcp => {
            mcp::serve(&root)?;
            Ok(0)
        }
        Cmd::Init { agents } => {
            let agents: Vec<&skills::Agent> = match agents {
                None | Some(InitAgents::All) => skills::AGENTS.iter().collect(),
                Some(InitAgents::Claude) => vec![skills::agent("claude")?],
                Some(InitAgents::Codex) => vec![skills::agent("codex")?],
            };
            init::run(&root, &agents)?;
            if ig_wire::is_landscape(&root) {
                let n = ig_fs::folders::scan(&root).projects.len();
                say(
                    quiet,
                    &format!(
                        "A folder of {n} projects: each keeps its own ig/. `ig analyse` reads those without one."
                    ),
                );
            }
            say(quiet, "next: ig analyse");
            Ok(0)
        }
        Cmd::App { dir } => {
            app::open(dir.as_deref().unwrap_or(&root))?;
            Ok(0)
        }
        Cmd::Hook { .. } | Cmd::Completions { .. } | Cmd::Plugin(_) => unreachable!(),
    }
}

fn main() -> ExitCode {
    match real_main() {
        Ok(code) => ExitCode::from(code),
        Err(e) => {
            eprintln!("ig: {e:#}");
            ExitCode::from(2)
        }
    }
}
