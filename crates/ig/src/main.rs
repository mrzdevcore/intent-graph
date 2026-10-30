mod agent;
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
use ig_app::map::view;
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
        Some("check") => {
            eprintln!("ig check was removed in this version; run ig view");
            Some(2)
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
                view::run(&env)?
            } else {
                let flag = match a.agent.as_deref() {
                    Some(n) => Some(Agent::parse(n).ok_or_else(|| {
                        anyhow::anyhow!("--agent {n}: expected claude, gemini, codex or ollama")
                    })?),
                    None => None,
                };
                let bar = agent::Bar::new();
                let mut confirm = agent::confirm;
                let mut run =
                    |ag: Agent, prompt: &str, ids: &[String]| agent::run(&root, ag, prompt, ids);
                view::run_with(
                    &env,
                    Some(&mut Ask {
                        all: a.read,
                        flag,
                        template: agent::PROMPT,
                        found: &agent::on_path,
                        confirm: agent::interactive()
                            .then_some(&mut confirm as &mut dyn FnMut(Agent) -> bool),
                        run: &mut run,
                        meter: &|done, total, status| bar.set(done, total, status),
                    }),
                )?
            };
            for n in &v.notices {
                eprintln!("ig: {n}");
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
            Ok(0)
        }
        "" => {
            eprintln!("{USAGE}");
            Ok(2)
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
