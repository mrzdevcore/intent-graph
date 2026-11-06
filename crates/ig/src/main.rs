mod accept;
mod agent;
mod app;
mod cli;
mod init;
mod mcp;
mod plan;
mod render;
mod skills;
#[cfg(test)]
mod tests;

use anyhow::{Result, bail};
use clap::CommandFactory;
use cli::{AgentName, Cli, Cmd, InitAgents, PluginCmd, ViewArgs};
use ig_app::map::agent::{Agent, Ask};
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

fn agent_flag(a: Option<AgentName>) -> Option<Agent> {
    a.and_then(|n| Agent::parse(n.as_str()))
}

fn read_project(dir: &Path, v: &ViewArgs, quiet: bool) -> Result<()> {
    say(quiet, &format!("ig: reading {}", dir.display()));
    let w = Wired::new(dir);
    let flag = agent_flag(v.agent);
    let template = agent::prompt();
    let bar = agent::Bar::new();
    let mut confirm = agent::confirm;
    let mut run = |ag: Agent, prompt: &str, ids: &[String]| agent::run(dir, ag, prompt, ids);
    view::run_with(
        &w.env(),
        Some(&mut Ask {
            all: true,
            flag,
            template: &template,
            found: &agent::on_path,
            confirm: agent::interactive().then_some(&mut confirm as &mut dyn FnMut(Agent) -> bool),
            run: &mut run,
            meter: &|done, total, status| bar.set(done, total, status),
        }),
    )?;
    Ok(())
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
    let w = Wired::new(root);
    let env = w.env();
    let folder = ig_wire::is_landscape(root);
    if folder && a.read {
        for dir in ig_wire::unread_projects(root) {
            read_project(&dir, a, quiet)?;
        }
    }
    let v = if folder {
        ig_wire::landscape(root).expect("just checked")
    } else if a.no_read {
        view::run_live(&env, None, ig_wire::CONNECTORS)?
    } else {
        let flag = agent_flag(a.agent);
        let template = agent::prompt();
        let bar = agent::Bar::new();
        let mut confirm = agent::confirm;
        let mut run = |ag: Agent, prompt: &str, ids: &[String]| agent::run(root, ag, prompt, ids);
        view::run_live(
            &env,
            Some(&mut Ask {
                all: a.read,
                flag,
                template: &template,
                found: &agent::on_path,
                confirm: agent::interactive()
                    .then_some(&mut confirm as &mut dyn FnMut(Agent) -> bool),
                run: &mut run,
                meter: &|done, total, status| bar.set(done, total, status),
            }),
            ig_wire::CONNECTORS,
        )?
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
        Cmd::Plan { json } | Cmd::Check { json } => {
            let check = matches!(command, Cmd::Check { .. });
            let name = if check { "check" } else { "plan" };
            // check only reads: ig/ is written by plan and view
            if check && !has_spec(&env)? {
                eprintln!("ig: no ig/ yet: run `ig plan` or `ig view` to write it");
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
                eprintln!("ig: no ig/ yet: run `ig plan` or `ig view` to write it");
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
                eprintln!("ig: no ig/ yet: run `ig plan` or `ig view` to write it");
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
            let folder = ig_wire::is_landscape(&root);
            init::run(&root, &agents)?;
            if folder {
                let n = ig_fs::folders::scan(&root).projects.len();
                println!(
                    "A folder of {n} projects: each keeps its own ig/. `ig view --read` reads those without one."
                );
                return Ok(0);
            }
            // ig/ starts as ig's map of the code: an empty repo gets an empty system
            if !has_spec(&env)? {
                say_all(quiet, &view::run(&env)?);
            }
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
