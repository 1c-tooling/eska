//! Explicit attachment selects existing authoring and optional independently approved support.

use std::path::PathBuf;

use clap::Args;

#[derive(Debug, Args)]
pub(in crate::project_cli) struct Init {
    #[arg(id = "init_path", default_value = ".")]
    pub path: PathBuf,
    #[arg(long = "source", id = "init_source", default_value = "src")]
    pub source: String,
    #[arg(long)]
    pub name: String,
    #[arg(long)]
    pub accepted_base_sha256: Option<String>,
    #[arg(long)]
    pub platform_version: Option<String>,
}
