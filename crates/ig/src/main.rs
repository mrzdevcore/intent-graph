mod cli;
mod hook;
mod init;
mod mcp;
mod render;
mod report;
mod serve;
mod skills;
#[cfg(test)]
mod tests;

use anyhow::{Result, bail};
use cli::{Args, USAGE, opts, parse_args};
use ig_app::view_data;
use ig_app::{Env, apply, brief, check, notes, plan, review, setup};
// `crate::model` for the modules (and their tests) that name model types.
use ig_core::model;
use ig_core::ports::Files;
use ig_fs::FsFiles;
use ig_wire::{Wired, read_plan, shown_plan};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{Duration, Instant};

/// Plan and notes paths are relative to the working directory: files rooted there.
static CWD: FsFiles = FsFiles {
    root: PathBuf::new(),
};

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
        "check" => {
            let run = check::run(&env, &opts(&a))?;
            let r = &run.result;
            if a.json {
                println!("{}", serde_json::to_string_pretty(r)?);
            } else if a.agent {
                print!("{}", check::agent_text(r, run.config.block_on));
            } else {
                print!("{}", check::summary_text(r));
            }
            Ok(if r.blocking { 1 } else { 0 })
        }
        "diff" => {
            let run = check::run(
                &env,
                &check::Opts {
                    all: false,
                    ..opts(&a)
                },
            )?;
            if a.json {
                let v = check::diff_json(&run.result, run.base_graph.as_ref());
                println!("{}", serde_json::to_string_pretty(&v)?);
            } else {
                print!("{}", check::diff_text(&run.result, run.base_graph.as_ref()));
            }
            Ok(0)
        }
        "review" => review_cmd(&env, &a),
        "query" => query_cmd(&env, &a),
        "plan" => match (a.sub.as_deref(), a.more.as_slice()) {
            (Some("check"), [file]) => {
                let (rel, text) = read_plan(&root, Path::new(file))?;
                let v = plan::check(&env, &rel, &text)?;
                if a.json {
                    println!("{}", serde_json::to_string_pretty(&v)?);
                } else {
                    print!("{}", plan::text(&v));
                }
                Ok(if v["ok"] == true { 0 } else { 1 })
            }
            (Some("notes"), []) => {
                let Some(plan) = view_data::pick_plan(&env, a.plan.as_deref(), false).0 else {
                    bail!("plan notes: no current plan (run `ig plan check <file>`)");
                };
                let list: Vec<notes::Note> = if a.wait {
                    wait_notes(&plan, Duration::from_secs(1800))
                } else {
                    notes::read(&CWD, &plan)
                        .into_iter()
                        .filter(|n| !n.done)
                        .collect()
                };
                if a.json {
                    println!("{}", serde_json::to_string_pretty(&list)?);
                } else {
                    for n in &list {
                        println!("{}", notes::format(n));
                    }
                }
                Ok(if a.wait && list.is_empty() { 1 } else { 0 })
            }
            _ => bail!("plan: expected `plan check <file>` or `plan notes [--wait] [--json]`"),
        },
        "apply" => match (a.sub.as_deref(), a.more.as_slice()) {
            (Some("start"), more) if more.len() <= 1 => {
                let Some(plan) = more
                    .first()
                    .map(PathBuf::from)
                    .or_else(|| plan::current(&env))
                else {
                    bail!(
                        "apply start: no plan given and no current plan (run `ig plan check <file>`)"
                    );
                };
                let (rel, text) = read_plan(&root, &plan)?;
                apply::start(&env, &rel, &text)?;
                println!("apply started: {}", plan.display());
                Ok(0)
            }
            (Some("accept"), []) => {
                apply::accept(&env)?;
                Ok(0)
            }
            (Some("review"), []) => {
                let mut input = String::new();
                std::io::Read::read_to_string(&mut std::io::stdin(), &mut input)?;
                println!("review: {} finding(s)", apply::review_ingest(&env, &input)?);
                Ok(0)
            }
            (Some("finish"), []) => {
                let mut out = String::new();
                let r = apply::finish(&env, &mut out);
                print!("{out}");
                Ok(r?)
            }
            (Some("abort"), []) => {
                println!("apply aborted: {}", apply::abort(&env)?);
                Ok(0)
            }
            _ => bail!(
                "apply: expected `apply start [<plan>]`, `apply accept`, `apply review`, `apply finish` or `apply abort`"
            ),
        },
        "except" => match (a.sub.as_deref(), a.reason.as_deref()) {
            (Some(fp), Some(reason)) if !reason.trim().is_empty() => {
                apply::except(&env, fp, reason)?;
                Ok(0)
            }
            _ => bail!("except: expected `except <fingerprint> --reason <text>`"),
        },
        "focus" => {
            let ids: Vec<&String> = a.sub.iter().chain(a.more.iter()).collect();
            if ids.is_empty() {
                bail!("focus: expected at least one node id");
            }
            w.files()
                .write_atomic(view_data::FOCUS, &serde_json::to_vec_pretty(&ids)?)?;
            Ok(0)
        }
        "brief" => {
            let run = check::run(&env, &opts(&a))?;
            let text = brief::brief(&env, &run, &opts(&a), a.md || a.out.is_some());
            match &a.out {
                Some(path) => {
                    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
                        std::fs::create_dir_all(parent)?;
                    }
                    std::fs::write(path, &text)?;
                    println!("{}", path.display());
                }
                None => print!("{text}"),
            }
            Ok(0)
        }
        "view" => {
            let plan = view_data::pick_plan(&env, a.plan.as_deref(), a.no_plan);
            if a.json {
                let d = view_data::json(&env, &opts(&a), shown_plan(&root, &plan))?;
                println!("{}", serde_json::to_string_pretty(&d)?);
            } else if a.watch {
                if a.head.is_some() {
                    bail!("view --watch: --head is not supported");
                }
                serve::run(
                    &root,
                    a.port,
                    &opts(&a),
                    a.plan.clone(),
                    a.no_plan,
                    !a.no_open,
                )?;
            } else {
                let path = render::write(&w, &opts(&a), &plan)?;
                if !a.no_open {
                    render::open(&path);
                }
                println!("{}", path.display());
            }
            Ok(0)
        }
        "report" => {
            if !a.gitlab {
                bail!("report: only --gitlab is supported");
            }
            report::gitlab(&root)
        }
        "serve" => match a.sub.as_deref() {
            None => {
                if a.head.is_some() {
                    bail!("serve: --head is not supported");
                }
                serve::run(&root, a.port, &opts(&a), a.plan.clone(), a.no_plan, false)?;
                Ok(0)
            }
            Some("claude") => {
                // Channels are a research preview: a custom channel needs this flag (unverified;
                // from the plugin alone the value may be `plugin:ig:ig`).
                let args = ["--dangerously-load-development-channels", "server:ig"];
                eprintln!("ig: running claude {}", args.join(" "));
                let status = std::process::Command::new("claude")
                    .args(args)
                    .current_dir(&root)
                    .status()
                    .map_err(|e| anyhow::anyhow!("serve claude: cannot run claude: {e}"))?;
                Ok(status.code().unwrap_or(1).clamp(0, 255) as u8)
            }
            Some(other) => {
                bail!("serve: unknown {other:?}; expected `serve [--port N]` or `serve claude`")
            }
        },
        "mcp" => {
            mcp::serve(&root)?;
            Ok(0)
        }
        "hook" => match a.sub.as_deref() {
            Some("claude") => {
                hook::claude(&root);
                Ok(0)
            }
            _ => bail!("hook: expected `hook claude`"),
        },
        "init" => {
            let agents: Vec<&skills::Agent> = match a.sub.as_deref() {
                None | Some("all") => skills::AGENTS.iter().collect(),
                Some(name) => vec![skills::agent(name)?],
            };
            init::run(&root, &agents)?;
            Ok(0)
        }
        "setup" | "explore" => match a.sub.as_deref() {
            Some(name) if a.cmd == "setup" => skills::run_setup(&root, name),
            _ => {
                if a.cmd == "explore" {
                    eprintln!("ig: `ig explore` is now `ig setup`");
                }
                let f = setup::facts(&env)?;
                if a.json {
                    println!("{}", serde_json::to_string_pretty(&f)?);
                } else {
                    print!("{}", setup::report(&f));
                }
                Ok(0)
            }
        },
        "" => {
            eprintln!("{USAGE}");
            Ok(2)
        }
        other => bail!("unknown command {other:?}\n{USAGE}"),
    }
}

fn query_cmd(env: &Env, a: &Args) -> Result<u8> {
    let Some(id) = a.sub.as_deref() else {
        bail!("query: expected a node id");
    };
    let cfg = ig_app::config::load(env.files)?;
    let repo_head = env.vcs.and_then(|r| r.head());
    let built = ig_app::graph(env, env.files, &cfg, repo_head)?;
    let v = ig_app::query::query(
        &built.graph,
        id,
        a.dir.as_deref().unwrap_or("both"),
        a.depth.unwrap_or(2),
    );
    let failed = v.get("error").is_some();
    if a.json {
        println!("{}", serde_json::to_string_pretty(&v)?);
    } else if failed {
        println!("{}", v["error"].as_str().unwrap_or_default());
        for s in v["suggestions"].as_array().into_iter().flatten() {
            println!("  did you mean {}", s.as_str().unwrap_or_default());
        }
    } else {
        let s = |x: &serde_json::Value| x.as_str().unwrap_or_default().to_string();
        println!("nodes:");
        for n in v["nodes"].as_array().into_iter().flatten() {
            println!(
                "  {} {} ({}:{})",
                s(&n["kind"]),
                s(&n["id"]),
                s(&n["file"]),
                n["line"]
            );
        }
        println!("edges:");
        for e in v["edges"].as_array().into_iter().flatten() {
            println!("  {} -{}-> {}", s(&e["from"]), s(&e["kind"]), s(&e["to"]));
        }
    }
    Ok(failed as u8)
}

fn review_cmd(env: &Env, a: &Args) -> Result<u8> {
    let (head, rest) = review::run(env)?;
    if a.json {
        let all: Vec<&review::ReviewItem> = head.iter().chain(rest.iter()).collect();
        println!("{}", serde_json::to_string_pretty(&all)?);
    } else if a.agent {
        print!("{}", review::agent_text(&head, &rest, a.all));
    } else {
        print!("{}", review::text(&head, &rest, a.all));
    }
    Ok(0)
}

/// Blocks until an open note that was not there when called appears, or `timeout` passes (then
/// none). Polls the file: it works without `ig serve`.
fn wait_notes(plan: &Path, timeout: Duration) -> Vec<notes::Note> {
    let seen: HashSet<String> = notes::read(&CWD, plan).into_iter().map(|n| n.id).collect();
    let end = Instant::now() + timeout;
    while Instant::now() < end {
        let new: Vec<notes::Note> = notes::read(&CWD, plan)
            .into_iter()
            .filter(|n| !n.done && !seen.contains(&n.id))
            .collect();
        if !new.is_empty() {
            return new;
        }
        std::thread::sleep(Duration::from_millis(300));
    }
    vec![]
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
