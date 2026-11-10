use crate::model;
use clap::error::{ContextKind, ErrorKind};
use clap::{Args, CommandFactory, Parser, Subcommand, ValueEnum};
use std::path::{Path, PathBuf};

const PLUMBING: &str = "Plumbing:
  mcp          Serve ig to an agent over MCP (stdio)
  plugin       Write the agent plugin files
  completions  Print a shell completion script

The format of ig/: man 5 ig-map";

#[derive(Parser)]
#[command(name = "ig", version = model::TOOL_VERSION, about = "Keep the map of your code and the code in step", after_help = PLUMBING)]
pub struct Cli {
    #[arg(
        short = 'C',
        long = "root",
        global = true,
        value_name = "DIR",
        help = "Run in DIR instead of the current folder"
    )]
    pub root: Option<PathBuf>,
    #[arg(
        short,
        long,
        global = true,
        help = "Print results only: no notices or warnings"
    )]
    pub quiet: bool,
    #[command(subcommand)]
    pub command: Option<Cmd>,
}

#[derive(Subcommand)]
pub enum Cmd {
    #[command(about = "Set this repo up: ig.yaml, the agent skills, the AGENTS.md snippet")]
    Init {
        #[arg(help = "Which agent to set up (default: all)")]
        agents: Option<InitAgents>,
    },
    #[command(
        visible_alias = "analyze",
        about = "Read the code and write ig/ (or propose updates to it) for you to review"
    )]
    Analyse(AnalyseArgs),
    #[command(about = "Draw the map and open it")]
    View(ViewArgs),
    #[command(about = "Ask a question; the answer is drawn on the map")]
    Explore(ExploreArgs),
    #[command(about = "Show what a change does to the architecture")]
    Diff(DiffArgs),
    #[command(about = "Show what to build, move and accept to bring code and map together")]
    Plan {
        #[arg(long, help = "Print the plan as JSON")]
        json: bool,
    },
    #[command(about = "Exit 1 when the code and the map have drifted apart")]
    Check {
        #[arg(long, help = "Print the plan as JSON")]
        json: bool,
    },
    #[command(
        about = "Take drift into the map",
        after_help = "Items are written one by one; an error stops it with the items before it accepted."
    )]
    Accept {
        #[arg(long, help = "Accept every drift box and arrow")]
        all: bool,
        #[arg(
            required_unless_present = "all",
            conflicts_with = "all",
            help = "Box or arrow ids; quote arrows: 'web->api'"
        )]
        ids: Vec<String>,
    },
    #[command(about = "Rewrite the ig/ files in canonical form")]
    Fmt {
        #[arg(long, help = "Write nothing; exit 1 when a file would change")]
        check: bool,
    },
    #[command(about = "Open the desktop app on a project")]
    App {
        #[arg(help = "The project (default: the current folder)")]
        dir: Option<PathBuf>,
    },
    #[command(hide = true)]
    Mcp,
    #[command(subcommand, hide = true)]
    Plugin(PluginCmd),
    #[command(hide = true)]
    Completions { shell: clap_complete::Shell },
    #[command(hide = true)]
    Hook {
        #[arg(hide = true, num_args = 0..)]
        rest: Vec<String>,
    },
}

#[derive(Subcommand)]
pub enum PluginCmd {
    #[command(about = "Write the plugin files into DIR")]
    Export { dir: PathBuf },
}

#[derive(Clone, Copy, ValueEnum)]
pub enum InitAgents {
    Claude,
    Codex,
    All,
}

#[derive(Clone, Copy, ValueEnum)]
pub enum AgentName {
    Claude,
    Gemini,
    Codex,
    Ollama,
}

impl AgentName {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Gemini => "gemini",
            Self::Codex => "codex",
            Self::Ollama => "ollama",
        }
    }
}

#[derive(Args)]
pub struct ViewArgs {
    #[arg(long, help = "Write the page without opening it")]
    pub no_open: bool,
    #[arg(long, help = "Print the map as JSON instead of writing a page")]
    pub json: bool,
    // `view`'s flags from before `ig analyse`, answered for one release
    #[arg(long, hide = true)]
    pub no_read: bool,
    #[arg(long, hide = true)]
    pub read: bool,
    #[arg(long, hide = true)]
    pub agent: Option<String>,
}

#[derive(Args)]
pub struct ExploreArgs {
    #[arg(help = "The question, in quotes")]
    pub question: String,
    #[arg(
        long,
        value_enum,
        help = "The agent that answers (claude only for now)"
    )]
    pub agent: Option<AgentName>,
    #[arg(long, help = "Draw the answer without opening the page")]
    pub no_open: bool,
}

#[derive(Args)]
pub struct DiffArgs {
    #[arg(help = "The commit to compare against (default: where this branch left main)")]
    pub base: Option<String>,
    #[arg(
        requires = "base",
        help = "The commit to compare (default: the working tree)"
    )]
    pub head: Option<String>,
    #[arg(long, help = "Print the diff as JSON")]
    pub json: bool,
    #[arg(long, help = "Write the page without opening it")]
    pub no_open: bool,
}

#[derive(Args)]
pub struct AnalyseArgs {
    #[arg(long, value_enum, help = "The agent to read with, without asking")]
    pub agent: Option<AgentName>,
    #[arg(
        long,
        conflicts_with = "agent",
        help = "Never start an agent: a new ig/ is ig's reading of the code alone"
    )]
    pub no_agent: bool,
}

pub fn usage() -> String {
    Cli::command().render_usage().to_string()
}

pub fn parse() -> Cli {
    match Cli::try_parse() {
        Ok(cli) => cli,
        Err(err) => {
            if err.kind() == ErrorKind::InvalidSubcommand
                && let Some(word) = err.get(ContextKind::InvalidSubcommand)
            {
                let word = word.to_string();
                if Path::new(&word).is_dir() {
                    eprintln!("ig: use `ig app {word}`");
                    std::process::exit(2);
                }
            }
            err.exit()
        }
    }
}
