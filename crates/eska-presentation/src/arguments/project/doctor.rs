//! Shared project command options and help; metadata and transactions stay with their owner.

use crate::localization::Localizer;
use clap::{Args, ValueEnum};
use std::path::PathBuf;

#[derive(Debug, Args)]
pub struct DoctorArgs {
    #[arg(long)]
    pub ibcmd: Option<PathBuf>,

    #[arg(long)]
    pub platform_arch: Option<String>,

    #[arg(long)]
    pub distrobox: Option<String>,

    #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
    pub format: OutputFormat,

    #[arg(short = 'p', long)]
    pub project: Vec<String>,

    #[arg(long)]
    pub workspace: bool,

    #[arg(short, long, action = clap::ArgAction::Help)]
    pub help: Option<bool>,
}

#[derive(Clone, Copy, Debug, Default, ValueEnum)]
pub enum OutputFormat {
    #[default]
    Human,
    Json,
}

/// Localize command help without changing option names or machine-facing values.
pub fn localize(command: clap::Command, localizer: &Localizer) -> clap::Command {
    command
        .about(localizer.text("doctor-about"))
        .override_usage(localizer.text("doctor-usage"))
        .mut_arg("ibcmd", |arg| {
            arg.help(localizer.text("build-ibcmd-help"))
                .value_name(localizer.text("build-ibcmd-value"))
        })
        .mut_arg("platform_arch", |arg| {
            arg.help(localizer.text("build-arch-help"))
                .value_name(localizer.text("build-arch-value"))
        })
        .mut_arg("distrobox", |arg| {
            arg.help(localizer.text("build-distrobox-help"))
                .value_name(localizer.text("build-distrobox-value"))
        })
        .mut_arg("format", |arg| {
            arg.help(localizer.text("doctor-format-help"))
                .value_name(localizer.text("doctor-format-value"))
        })
        .mut_arg("project", |arg| {
            arg.help(localizer.text("doctor-project-help"))
                .value_name(localizer.text("doctor-project-value"))
        })
        .mut_arg("workspace", |arg| {
            arg.help(localizer.text("doctor-workspace-help"))
        })
        .mut_arg("help", |arg| arg.help(localizer.text("cli-help")))
}
