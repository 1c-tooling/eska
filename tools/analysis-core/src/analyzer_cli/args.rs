//! Process arguments select one project; standard initialization does not silently override it.

use std::path::PathBuf;

use crate::tool_profile::Profile;
use clap::{ArgAction, Parser};
use eska::tool_cli::OutputFormat;

#[derive(Debug, Parser)]
#[command(
    name = "eska-analyzer",
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
    #[arg(long, value_enum, default_value_t = Profile::Syntax)]
    pub profile: Profile,
    #[arg(long)]
    pub accepted_base_sha256: Option<String>,
    #[arg(long, default_value_t = 300)]
    pub timeout_seconds: u64,
    #[arg(long, default_value_t = 64 * 1024 * 1024)]
    pub max_document_bytes: u64,
    #[arg(long, default_value_t = 256 * 1024 * 1024)]
    pub max_retained_source_bytes: u64,
    #[arg(long, conflicts_with_all = ["project_dir", "project", "profile", "accepted_base_sha256", "timeout_seconds", "max_document_bytes", "max_retained_source_bytes"])]
    pub capabilities: bool,
    #[arg(long, value_enum, default_value_t = OutputFormat::Human, requires = "capabilities")]
    pub format: OutputFormat,
    #[arg(short, long, action = ArgAction::Help)]
    help: Option<bool>,
    #[arg(short = 'V', long, action = ArgAction::Version)]
    version: Option<bool>,
}
