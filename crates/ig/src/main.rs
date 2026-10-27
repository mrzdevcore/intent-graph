mod cli;
mod init;
mod mcp;
mod render;
mod skills;
#[cfg(test)]
mod tests;

use anyhow::{Result, bail};
use cli::{USAGE, parse_args};
use ig_app::map::view;
// `crate::model` for the modules (and their tests) that name model types.
use ig_core::model;
use ig_wire::Wired;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

fn real_main() -> Result<u8> {
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
            let v = view::run(&env)?;
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
