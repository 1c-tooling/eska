//! Every currently supported profile is explicit; semantic checking is not an implicit default.

use std::path::PathBuf;

use clap::{ArgAction, Parser, ValueEnum};
use eska::tool_cli::OutputFormat;

#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum Profile {
    Syntax,
}

#[derive(Debug, Parser)]
#[command(
    name = "eska-check",
    disable_help_flag = true,
    disable_version_flag = true
)]
pub struct Cli {
    #[arg(long)]
    pub lang: Option<String>,
    #[arg(long, default_value = ".")]
    pub project_dir: PathBuf,
    #[arg(long)]
    pub project: Option<String>,
    #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
    pub format: OutputFormat,
    #[arg(long, value_enum)]
    pub profile: Profile,
    #[arg(long)]
    pub config: Option<PathBuf>,
    #[arg(long, default_value_t = 300)]
    pub timeout_seconds: u64,
    #[arg(long, default_value_t = 64 * 1024 * 1024)]
    pub max_document_bytes: u64,
    #[arg(short, long, action = ArgAction::Help)]
    help: Option<bool>,
    #[arg(short = 'V', long, action = ArgAction::Version)]
    version: Option<bool>,
}
