//! Native commands select one project and explicit process-local overrides.

use crate::tool_cli::OutputFormat;
use clap::{ArgAction, Args, Parser, Subcommand};
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(
    name = "eska-build",
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
    Build(Box<Build>),
    #[command(disable_help_flag = true)]
    Capabilities,
    #[command(disable_help_flag = true)]
    Unpack(Unpack),
}

impl Command {
    /// Use stable operation identifiers for both dispatch and machine output.
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Build(_) => "build",
            Self::Capabilities => "capabilities",
            Self::Unpack(_) => "unpack",
        }
    }
}

#[derive(Debug, Args)]
pub(super) struct Build {
    #[arg(long)]
    pub project: Option<String>,
    #[arg(long)]
    pub output: Option<PathBuf>,
    #[arg(long)]
    pub platform_version: Option<String>,
    #[arg(long)]
    pub ibcmd: Option<PathBuf>,
    #[arg(long)]
    pub platform_arch: Option<String>,
    #[arg(long)]
    pub distrobox: Option<String>,
    #[arg(long)]
    pub base_configuration: Option<PathBuf>,
    #[arg(long)]
    pub recreate_infobase: bool,
    #[arg(long)]
    pub accepted_base_sha256: Option<String>,
    #[arg(long)]
    pub generations: Option<PathBuf>,
    #[arg(long)]
    pub allow_exclusions: bool,
}

#[derive(Debug, Args)]
pub(super) struct Unpack {
    pub file: PathBuf,
    pub destination: PathBuf,
    #[arg(long)]
    pub platform_version: String,
    #[arg(long)]
    pub ibcmd: Option<PathBuf>,
    #[arg(long)]
    pub platform_arch: Option<String>,
    #[arg(long)]
    pub distrobox: Option<String>,
}
