//! Every currently supported profile is explicit; semantic checking is not an implicit default.

use std::path::PathBuf;

use clap::{ArgAction, Parser};
use eska::tool_cli::OutputFormat;

use crate::tool_profile::Profile;

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
    #[arg(long, value_enum, required_unless_present = "capabilities")]
    pub profile: Option<Profile>,
    #[arg(long, conflicts_with_all = ["profile", "project_dir", "project", "config", "timeout_seconds", "max_document_bytes", "max_retained_source_bytes"])]
    pub capabilities: bool,
    #[arg(long)]
    pub config: Option<PathBuf>,
    #[arg(long, default_value_t = 300)]
    pub timeout_seconds: u64,
    #[arg(long, default_value_t = 64 * 1024 * 1024)]
    pub max_document_bytes: u64,
    #[arg(long, default_value_t = 256 * 1024 * 1024)]
    pub max_retained_source_bytes: u64,
    #[arg(short, long, action = ArgAction::Help)]
    help: Option<bool>,
    #[arg(short = 'V', long, action = ArgAction::Version)]
    version: Option<bool>,
}
