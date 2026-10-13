//! Explicit offline project commands and selectors; no native artifact or Git options.

use crate::tool_cli::OutputFormat;
use clap::{ArgAction, Args, Parser, Subcommand, ValueEnum};
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
    Migrate(Migrate),
    #[command(disable_help_flag = true)]
    UpdateBase(UpdateBase),
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
            Self::Migrate(_) => "migrate",
            Self::UpdateBase(_) => "update-base",
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
    #[arg(long, value_enum, default_value_t = ImportLayout::Family)]
    pub layout: ImportLayout,
    #[arg(long)]
    pub platform_version: Option<String>,
    #[arg(long)]
    pub freeze: Vec<String>,
}

/// Layout names are stable CLI values; native conversion rules remain in the project core.
#[derive(Clone, Copy, Debug, ValueEnum)]
pub(super) enum ImportLayout {
    Family,
    Readable,
}

impl ImportLayout {
    /// Report the explicit selection without localizing machine-facing layout identifiers.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Family => "family",
            Self::Readable => "readable",
        }
    }
}

#[derive(Debug, Args)]
pub(super) struct Selector {
    #[arg(long)]
    pub project: Option<String>,
}

/// Project migration requires an independently approved old base; preview never publishes files.
#[derive(Debug, Args)]
pub(super) struct Migrate {
    pub destination: PathBuf,
    #[command(flatten)]
    pub selector: Selector,
    #[arg(long)]
    pub accepted_base_sha256: Option<String>,
    #[arg(long, conflicts_with_all = ["project", "accepted_base_sha256"])]
    pub workspace: bool,
    #[arg(long, requires = "workspace")]
    pub accepted_bases: Option<PathBuf>,
    #[arg(long)]
    pub write: bool,
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

/// Both pins are independent caller approvals; an incoming source tree is never reaccepted.
#[derive(Debug, Args)]
pub(super) struct UpdateBase {
    pub incoming: PathBuf,
    pub destination: PathBuf,
    #[command(flatten)]
    pub selector: Selector,
    #[arg(long, conflicts_with = "workspace")]
    pub incoming_project: Option<String>,
    #[arg(long, conflicts_with = "workspace")]
    pub accepted_base_sha256: Option<String>,
    #[arg(
        long,
        required_unless_present = "workspace",
        conflicts_with = "workspace"
    )]
    pub incoming_base_sha256: Option<String>,
    #[arg(long, conflicts_with = "project", requires_all = ["accepted_bases", "incoming_bases"])]
    pub workspace: bool,
    #[arg(long, requires = "workspace")]
    pub accepted_bases: Option<PathBuf>,
    #[arg(long, requires = "workspace")]
    pub incoming_bases: Option<PathBuf>,
    #[arg(long)]
    pub write: bool,
}

#[derive(Debug, Args)]
pub(super) struct Transport {
    #[command(flatten)]
    pub selector: Selector,
    #[arg(long)]
    pub write: bool,
}
