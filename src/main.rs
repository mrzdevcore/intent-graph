mod brief;
mod check;
mod checks;
mod config;
mod diff;
mod embedded;
mod extract;
mod git;
mod graph;
mod hook;
mod init;
mod intents;
mod linediff;
mod mcp;
mod model;
mod patterns;
mod report;
mod review;
mod setup;
mod skills;
mod tsconfig;
mod view;
mod workspace;

use anyhow::{Result, bail};
use std::path::PathBuf;
use std::process::ExitCode;

const USAGE: &str =
    "usage: ig [--version] | init [claude|codex|all] | setup [--json] | setup claude|codex
   | check [--base <ref>] [--head <ref>] [--all] [--json] [--agent]
   | brief [--base <ref>] [--head <ref>] [--md] [--out <file>]
   | review [--all] [--json] [--agent] | diff [--base <ref>] [--head <ref>] [--json]
   | view [--base <ref>] [--head <ref>] [--watch] [--no-open] | report --gitlab | mcp | hook claude
   (all accept --root <dir>)";

#[derive(Default)]
struct Args {
    cmd: String,
    sub: Option<String>,
    root: Option<PathBuf>,
    base: Option<String>,
    head: Option<String>,
    all: bool,
    json: bool,
    agent: bool,
    watch: bool,
    no_open: bool,
    gitlab: bool,
    version: bool,
    md: bool,
    out: Option<PathBuf>,
}

fn parse_args() -> Result<Args> {
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
            Long("out") => a.out = Some(p.value()?.into()),
            Value(v) if a.cmd.is_empty() => a.cmd = v.string()?,
            Value(v) if a.sub.is_none() => a.sub = Some(v.string()?),
            _ => return Err(arg.unexpected().into()),
        }
    }
    Ok(a)
}

fn opts(a: &Args) -> check::Opts {
    check::Opts {
        base: a.base.clone(),
        head: a.head.clone(),
        all: a.all,
    }
}

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
    match a.cmd.as_str() {
        "check" => {
            let run = check::run(&root, &opts(&a))?;
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
                &root,
                &check::Opts {
                    all: false,
                    ..opts(&a)
                },
            )?;
            if a.json {
                let lookup: std::collections::HashMap<&str, &model::Node> = run
                    .result
                    .graph
                    .nodes
                    .iter()
                    .chain(run.base_graph.iter().flat_map(|b| b.nodes.iter()))
                    .map(|n| (n.id.as_str(), n))
                    .collect();
                let v: Vec<serde_json::Value> = run
                    .result
                    .changes
                    .iter()
                    .filter_map(|(id, s)| {
                        let n = lookup.get(id.as_str())?;
                        Some(serde_json::json!({"id": id, "status": s, "file": n.file, "kind": n.kind, "name": n.name}))
                    })
                    .collect();
                println!("{}", serde_json::to_string_pretty(&v)?);
            } else {
                print!("{}", check::diff_text(&run.result, run.base_graph.as_ref()));
            }
            Ok(0)
        }
        "review" => review_cmd(&root, &a),
        "brief" => {
            let run = check::run(&root, &opts(&a))?;
            let text = brief::brief(&run, &root, &opts(&a), a.md || a.out.is_some());
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
            if a.watch {
                if a.head.is_some() {
                    bail!("view --watch: --head is not supported");
                }
                view::watch(&root, &opts(&a), !a.no_open)?;
            } else {
                let path = view::write(&root, &opts(&a))?;
                if !a.no_open {
                    view::open(&path);
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
            Some(name) if a.cmd == "setup" => setup::run_agent(&root, name),
            _ => {
                if a.cmd == "explore" {
                    eprintln!("ig: `ig explore` is now `ig setup`");
                }
                let f = setup::facts(&root)?;
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

fn review_cmd(root: &std::path::Path, a: &Args) -> Result<u8> {
    let cfg = config::Config::load(root)?;
    let repo_head = git::Repo::open(root).and_then(|r| r.head());
    let built = graph::build(root, &cfg, repo_head)?;
    let tests = workspace::globset(&cfg.tests)?;
    let eps = workspace::globset(&cfg.entry_points)?;
    let ctx = review::Ctx {
        config: &cfg,
        tests: &tests,
        packages: &built.workspace.packages,
        entry_points: &eps,
    };
    let items = review::review(&built.graph, &ctx, root);
    let (head, rest) = review::rank(items, cfg.review.top);
    let dir = root.join(".ig");
    std::fs::create_dir_all(&dir)?;
    let all: Vec<&review::ReviewItem> = head.iter().chain(rest.iter()).collect();
    check::write_atomic(&dir.join("review.json"), &serde_json::to_vec_pretty(&all)?)?;
    if a.json {
        println!("{}", serde_json::to_string_pretty(&all)?);
    } else if a.agent {
        let shown: Vec<&review::ReviewItem> = if a.all {
            all.clone()
        } else {
            head.iter().collect()
        };
        if shown.is_empty() {
            println!("ig review: no structural problems found");
        }
        for it in shown {
            println!(
                "- [{}] {}: {} Fix: {}",
                it.rule_id, it.title, it.evidence, it.fix
            );
        }
    } else {
        review::print(&head, rest.len(), a.all, &rest);
    }
    Ok(0)
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
