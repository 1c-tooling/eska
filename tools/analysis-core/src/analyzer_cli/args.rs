//! Process arguments select one project; standard initialization does not silently override it.

use std::path::PathBuf;

use clap::{ArgAction, Parser};

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
    #[arg(long, default_value_t = 64 * 1024 * 1024)]
    pub max_document_bytes: u64,
    #[arg(long, default_value_t = 256 * 1024 * 1024)]
    pub max_retained_source_bytes: u64,
    #[arg(short, long, action = ArgAction::Help)]
    help: Option<bool>,
    #[arg(short = 'V', long, action = ArgAction::Version)]
    version: Option<bool>,
}
