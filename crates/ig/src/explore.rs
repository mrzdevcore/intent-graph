use crate::cli::{AgentName, ExploreArgs};
use crate::{agent, render, say_all};
use anyhow::Result;
use ig_app::map::agent::Agent;
use ig_app::map::view;
use ig_wire::Wired;
use std::path::Path;

const PROMPT: &str = include_str!("../assets/explore-prompt.md");

const IG_TOOLS: &str = "mcp__ig__ig_state,mcp__ig__ig_show";
const TOOLS: &str = "Read,Grep,Glob,mcp__ig__ig_state,mcp__ig__ig_show";

pub fn run(root: &Path, args: &ExploreArgs, quiet: bool) -> Result<u8> {
    if args.agent.is_some_and(|a| !matches!(a, AgentName::Claude)) {
        eprintln!("ig: explore needs claude for now");
        return Ok(2);
    }
    if !agent::on_path("claude") {
        eprintln!("ig: claude was not found on PATH");
        return Ok(2);
    }
    let w = Wired::new(root);
    let v = view::run_live(&w.env(), &[])?;
    say_all(quiet, &v);
    std::fs::create_dir_all(root.join(".ig"))?;
    ig_fs::write_atomic(
        &root.join(".ig/view.json"),
        serde_json::to_string_pretty(&v)?.as_bytes(),
    )?;
    render::ignore_ig_dir(root)?;
    let mcp = mcp_config(&std::env::current_exe()?, root);
    let argv = argv(&mcp);
    let prompt = PROMPT.replace("{{question}}", &args.question);
    let answer = match agent::run_argv(root, "claude", &argv, &prompt, agent::TIMEOUT) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("ig: {e}");
            return Ok(2);
        }
    };
    println!("{}", answer.trim_end());
    let path = render::write(&w, &v)?;
    if !args.no_open {
        render::open(&path);
    }
    Ok(0)
}

fn mcp_config(exe: &Path, root: &Path) -> String {
    serde_json::json!({"mcpServers": {"ig": {
        "command": exe.to_string_lossy(),
        "args": ["mcp", "-C", root.to_string_lossy()],
    }}})
    .to_string()
}

// `--safe-mode` (the reading runner's) would turn MCP off, and with it `ig_show`
fn argv(mcp: &str) -> Vec<&str> {
    let mut argv: Vec<&str> = vec![
        "claude",
        "-p",
        "--output-format",
        "text",
        "--restricted",
        "--strict-mcp-config",
        "--mcp-config",
        mcp,
        // -p answers no permission prompt: the MCP tools are approved here or never
        "--allowed-tools",
        IG_TOOLS,
        "--tools",
        TOOLS,
        "--disallowed-tools",
    ];
    argv.extend(secret_rules().map(|r| -> &str { r }));
    argv
}

fn secret_rules() -> impl Iterator<Item = &'static str> {
    (Agent::Claude.argv().iter().copied())
        .skip_while(|a| *a != "--disallowed-tools")
        .skip(1)
        .take_while(|a| !a.starts_with("--"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_secret_rules_come_from_the_reading_runner() {
        let rules: Vec<&str> = secret_rules().collect();
        assert!(rules.contains(&"Read(**/.env*)"), "{rules:?}");
        assert!(rules.iter().all(|r| r.starts_with("Read(")), "{rules:?}");
        assert_eq!(argv("{}").last(), rules.last());
    }

    #[test]
    fn the_mcp_server_is_this_ig_on_the_root() {
        let c: serde_json::Value =
            serde_json::from_str(&mcp_config(Path::new("/bin/ig"), Path::new("/r"))).unwrap();
        assert_eq!(c["mcpServers"]["ig"]["command"], "/bin/ig");
        assert_eq!(
            c["mcpServers"]["ig"]["args"],
            serde_json::json!(["mcp", "-C", "/r"])
        );
    }
}
