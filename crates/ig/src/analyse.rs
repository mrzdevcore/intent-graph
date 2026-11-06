use crate::cli::AnalyseArgs;
use crate::{agent, render, say, say_all};
use anyhow::Result;
use ig_app::map::agent::{Agent, Ask};
use ig_app::map::{spec, view};
use ig_wire::Wired;
use std::path::{Path, PathBuf};

pub fn run(root: &Path, args: &AnalyseArgs, quiet: bool) -> Result<u8> {
    let folder = ig_wire::is_landscape(root);
    let dirs: Vec<PathBuf> = if folder {
        ig_wire::unread_projects(root)
    } else {
        vec![root.to_path_buf()]
    };
    let (mut wrote, mut broken) = (false, false);
    for dir in &dirs {
        if folder {
            say(quiet, &format!("ig: reading {}", dir.display()));
        }
        let (changed, was_broken) = project(dir, args, quiet)?;
        wrote |= changed;
        broken |= was_broken;
    }
    if wrote {
        say(quiet, "next: ig view, then commit ig/");
    }
    Ok(if broken { 2 } else { 0 })
}

fn project(dir: &Path, args: &AnalyseArgs, quiet: bool) -> Result<(bool, bool)> {
    let w = Wired::new(dir);
    let env = w.env();
    let before = spec_files(&env)?;
    let v = if args.no_agent {
        view::analyse(&env, None)?
    } else {
        let template = agent::prompt();
        let bar = agent::Bar::new();
        let mut confirm = agent::confirm;
        let mut run = |ag: Agent, prompt: &str, ids: &[String]| agent::run(dir, ag, prompt, ids);
        view::analyse(
            &env,
            Some(&mut Ask {
                flag: args.agent.and_then(|n| Agent::parse(n.as_str())),
                template: &template,
                found: &agent::on_path,
                confirm: agent::interactive()
                    .then_some(&mut confirm as &mut dyn FnMut(Agent) -> bool),
                run: &mut run,
                meter: &|done, total, status| bar.set(done, total, status),
            }),
        )?
    };
    say_all(quiet, &v);
    // the reading's proposals are journaled under `.ig/`
    render::ignore_ig_dir(dir)?;
    Ok((spec_files(&env)? != before, v.broken))
}

fn spec_files(env: &ig_app::Env) -> Result<Vec<(String, Option<String>)>> {
    let globs = [
        format!("{}/*.kdl", spec::DIR),
        format!("{}/kinds/*.kdl", spec::DIR),
    ];
    let mut paths = env.files.glob(&globs)?;
    paths.sort();
    Ok(paths
        .into_iter()
        .map(|p| {
            let text = env.files.read(&p);
            (p, text)
        })
        .collect())
}
