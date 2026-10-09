//! Explicit offline project commands and selectors; no native artifact or Git options.

use crate::tool_cli::OutputFormat;
use clap::{ArgAction, Args, Parser, Subcommand};
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(
    name = "eska-project",
    disable_help_flag = true,
    disable_version_flag = true,
    disable_help_subcommand = true
)]
pub(super) struct Cli {
    #[arg(long, global = true)]
    pub lang: Option<String>,
    #[arg(long, global = true, default_value = ".")]
    pub project_dir: PathBuf,
    #[arg(long, global = true, value_enum, default_value_t = OutputFormat::Human)]
    pub format: OutputFormat,
    #[arg(short, long, global = true, action = ArgAction::Help)]
    help: Option<bool>,
    #[arg(short = 'V', long, global = true, action = ArgAction::Version)]
    version: Option<bool>,
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub(super) enum Command {
    #[command(disable_help_flag = true)]
    Import(Import),
    #[command(disable_help_flag = true)]
    Inspect(Selector),
    #[command(disable_help_flag = true)]
    Prepare(Prepare),
    #[command(disable_help_flag = true)]
    Transport(Transport),
    #[command(disable_help_flag = true)]
    Capabilities,
}

impl Command {
    /// Stable operation identifiers also name the tool's JSON envelope commands.
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Import(_) => "import",
            Self::Inspect(_) => "inspect",
            Self::Prepare(_) => "prepare",
            Self::Transport(_) => "transport",
            Self::Capabilities => "capabilities",
        }
    }
}

#[derive(Debug, Args)]
pub(super) struct Import {
    pub input: PathBuf,
    pub destination: PathBuf,
    #[arg(long)]
    pub name: String,
    #[arg(long)]
    pub platform_version: Option<String>,
    #[arg(long)]
    pub freeze: Vec<String>,
}

#[derive(Debug, Args)]
pub(super) struct Selector {
    #[arg(long)]
    pub project: Option<String>,
}

#[derive(Debug, Args)]
pub(super) struct Prepare {
    #[command(flatten)]
    pub selector: Selector,
    #[arg(long)]
    pub accepted_base_sha256: Option<String>,
    #[arg(long)]
    pub generations: Option<PathBuf>,
    #[arg(long)]
    pub allow_exclusions: bool,
}

#[derive(Debug, Args)]
pub(super) struct Transport {
    #[command(flatten)]
    pub selector: Selector,
    #[arg(long)]
    pub write: bool,
}
