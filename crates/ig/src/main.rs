mod accept;
mod agent;
mod app;
mod cli;
mod init;
mod mcp;
mod render;
mod skills;
#[cfg(test)]
mod tests;

use anyhow::{Result, bail};
use cli::{USAGE, parse_args};
use ig_app::map::agent::{Agent, Ask};
use ig_app::map::{edit, origin, spec, view};
// `crate::model` for the modules (and their tests) that name model types.
use ig_core::model;
use ig_wire::Wired;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

/// Commands of earlier versions that hooks and CI may still run (ruling R29).
fn removed() -> Option<u8> {
    let mut args = std::env::args().skip(1);
    let cmd = loop {
        match args.next() {
            Some(a) if a == "--root" => {
                args.next();
            }
            other => break other,
        }
    };
    match cmd.as_deref() {
        Some("hook") => {
            eprintln!("ig hook was removed; delete ig's Stop hook from .claude/settings.json");
            Some(0)
        }
        _ => None,
    }
}

fn real_main() -> Result<u8> {
    if let Some(code) = removed() {
        return Ok(code);
    }
    let a = parse_args()?;
    if a.version {
        println!("ig {}", model::TOOL_VERSION);
        return Ok(0);
    }
    let root = a.root.clone().unwrap_or_else(|| PathBuf::from("."));
    if !root.is_dir() {
        bail!("--root {}: not a directory", root.display());
    }
    let root = root.canonicalize()?;
    let w = Wired::new(&root);
    let env = w.env();
    match a.cmd.as_str() {
        "view" => {
            let v = if a.no_read {
                view::run_live(&env, None, ig_wire::CONNECTORS)?
            } else {
                let flag = match a.agent.as_deref() {
                    Some(n) => Some(Agent::parse(n).ok_or_else(|| {
                        anyhow::anyhow!("--agent {n}: expected claude, gemini, codex or ollama")
                    })?),
                    None => None,
                };
                let template = agent::prompt();
                let bar = agent::Bar::new();
                let mut confirm = agent::confirm;
                let mut run =
                    |ag: Agent, prompt: &str, ids: &[String]| agent::run(&root, ag, prompt, ids);
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
            for n in &v.notices {
                eprintln!("ig: {n}");
            }
            for w in &v.warnings {
                eprintln!("warning: {w}");
            }
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
            render::ignore_ig_dir(&root)?;
            Ok(0)
        }
        "plan" | "check" => {
            // check only reads: ig/ is written by plan and view
            let spec = [format!("{}/*.kdl", spec::DIR)];
            if a.cmd == "check" && env.files.glob(&spec)?.is_empty() {
                eprintln!("ig: no ig/ yet: run `ig plan` or `ig view` to write it");
                return Ok(2);
            }
            let v = view::run(&env)?;
            for n in &v.notices {
                eprintln!("ig: {n}");
            }
            for w in &v.warnings {
                eprintln!("warning: {w}");
            }
            if v.broken {
                return Ok(2);
            }
            let unread = view::unread(&env, &v, &ig_fs::BUILD_DIRS)?;
            let p = origin::plan_of(&v.boxes, &v.arrows, &unread);
            println!("{}", p.text(&a.cmd));
            Ok(u8::from(a.cmd == "check" && !p.is_empty()))
        }
        "fmt" => {
            if env
                .files
                .glob(&[format!("{}/*.kdl", spec::DIR)])?
                .is_empty()
            {
                eprintln!("ig: no ig/ yet: run `ig plan` or `ig view` to write it");
                return Ok(2);
            }
            match edit::fmt(env.files, !a.check) {
                Ok(changed) => {
                    let verb = if a.check { "would format" } else { "formatted" };
                    changed.iter().for_each(|p| println!("{verb} {p}"));
                    Ok(u8::from(a.check && !changed.is_empty()))
                }
                Err(errs) => {
                    errs.iter().for_each(|e| eprintln!("ig: {e}"));
                    Ok(2)
                }
            }
        }
        "accept" => {
            if env
                .files
                .glob(&[format!("{}/*.kdl", spec::DIR)])?
                .is_empty()
            {
                eprintln!("ig: no ig/ yet: run `ig plan` or `ig view` to write it");
                return Ok(2);
            }
            let ids: Vec<String> = a.sub.iter().chain(&a.more).cloned().collect();
            if !a.all && ids.is_empty() {
                bail!("accept: expected --all or one or more ids\n{USAGE}");
            }
            let v = view::run(&env)?;
            if v.broken {
                v.notices.iter().for_each(|n| eprintln!("ig: {n}"));
                return Ok(2);
            }
            accept::run(env.files, &v, a.all, &ids)
        }
        "mcp" => {
            mcp::serve(&root)?;
            Ok(0)
        }
        "plugin" => match (a.sub.as_deref(), a.more.as_slice()) {
            (Some("export"), [dir]) => {
                // Prints each file written, relative to <dir> (the app prunes anything else).
                for p in skills::export(Path::new(dir))? {
                    println!("{}", p.strip_prefix(dir).unwrap_or(&p).display());
                }
                Ok(0)
            }
            _ => bail!("plugin: expected `plugin export <dir>`"),
        },
        "init" => {
            let agents: Vec<&skills::Agent> = match a.sub.as_deref() {
                None | Some("all") => skills::AGENTS.iter().collect(),
                Some(name) => vec![skills::agent(name)?],
            };
            init::run(&root, &agents)?;
            // ig/ starts as ig's map of the code: an empty repo gets an empty system
            if env
                .files
                .glob(&[format!("{}/*.kdl", spec::DIR)])?
                .is_empty()
            {
                for n in view::run(&env)?.notices {
                    eprintln!("ig: {n}");
                }
            }
            Ok(0)
        }
        "" => {
            eprintln!("{USAGE}");
            Ok(2)
        }
        "app" => {
            app::open(a.sub.as_deref().map_or(&root, Path::new))?;
            Ok(0)
        }
        // `ig .`, `ig ../shop`: a directory opens in the app
        other if Path::new(other).is_dir() => {
            app::open(Path::new(other))?;
            Ok(0)
        }
        other => bail!("unknown command {other:?}\n{USAGE}"),
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
