//! Minimal new-project selection remains separate from platform and Git command arguments.

use std::path::PathBuf;

use clap::{Args, ValueEnum};

use crate::project_core::{ProjectType, creation::INITIAL_PROJECT_VERSION};

#[derive(Debug, Args)]
pub(in crate::project_cli) struct New {
    pub destination: PathBuf,
    #[arg(long)]
    pub name: String,
    #[arg(long = "type", value_enum, default_value_t = NewType::Configuration)]
    pub project_type: NewType,
    #[arg(long)]
    pub metadata_name: Option<String>,
    #[arg(long, default_value = INITIAL_PROJECT_VERSION)]
    pub initial_version: String,
    #[arg(long)]
    pub platform_version: Option<String>,
}

/// Public option names match project type serialization without introducing Clap into the core.
#[derive(Clone, Copy, Debug, ValueEnum)]
pub(in crate::project_cli) enum NewType {
    Configuration,
    Extension,
    Processing,
    Report,
}

impl From<NewType> for ProjectType {
    /// Convert the closed CLI selection to the shared native artifact profile.
    fn from(value: NewType) -> Self {
        match value {
            NewType::Configuration => Self::Configuration,
            NewType::Extension => Self::Extension,
            NewType::Processing => Self::Processing,
            NewType::Report => Self::Report,
        }
    }
}
