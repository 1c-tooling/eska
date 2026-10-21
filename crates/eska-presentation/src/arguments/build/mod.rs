//! Shared native command options and help; execution belongs to eska-build.

use crate::localization::Localizer;
use clap::{Args, ValueEnum};
use std::path::PathBuf;

#[derive(Debug, Default, Args)]
pub struct BuildArgs {
    #[arg(long)]
    pub output: Option<PathBuf>,

    #[arg(long)]
    pub base_configuration: Option<PathBuf>,

    #[arg(long)]
    pub recreate_infobase: bool,

    #[arg(long)]
    pub ibcmd: Option<PathBuf>,

    #[arg(long)]
    pub platform_arch: Option<String>,

    #[arg(long)]
    pub distrobox: Option<String>,

    #[arg(long, conflicts_with = "select_platform")]
    pub platform_version: Option<String>,

    #[arg(long, conflicts_with = "platform_version")]
    pub select_platform: bool,

    #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
    pub format: OutputFormat,

    #[arg(short = 'p', long)]
    pub project: Vec<String>,

    #[arg(long)]
    pub workspace: bool,

    #[command(flatten)]
    pub mode: BuildModeArgs,

    #[arg(short, long, action = clap::ArgAction::Help)]
    pub help: Option<bool>,
}

#[derive(Debug, Default, Args)]
pub struct BuildModeArgs {
    #[arg(long)]
    pub dry_run: bool,

    #[arg(long, conflicts_with = "dry_run")]
    pub manifest: bool,
}

#[derive(Clone, Copy, Debug, Default, ValueEnum)]
pub enum OutputFormat {
    #[default]
    Human,
    Json,
}

/// Attach localized help to the build command and its arguments.
pub fn localize(command: clap::Command, localizer: &Localizer) -> clap::Command {
    command
        .about(localizer.text("build-about"))
        .override_usage(localizer.text("build-usage"))
        .mut_arg("output", |arg| {
            arg.help(localizer.text("build-output-help"))
                .value_name(localizer.text("build-output-value"))
        })
        .mut_arg("base_configuration", |arg| {
            arg.help(localizer.text("build-base-configuration-help"))
                .value_name(localizer.text("build-base-configuration-value"))
        })
        .mut_arg("recreate_infobase", |arg| {
            arg.help(localizer.text("build-recreate-infobase-help"))
        })
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
        .mut_arg("platform_version", |arg| {
            arg.help(localizer.text("build-platform-version-help"))
                .value_name(localizer.text("build-platform-version-value"))
        })
        .mut_arg("select_platform", |arg| {
            arg.help(localizer.text("build-select-platform-help"))
        })
        .mut_arg("format", |arg| {
            arg.help(localizer.text("build-format-help"))
                .value_name(localizer.text("build-format-value"))
        })
        .mut_arg("project", |arg| {
            arg.help(localizer.text("build-project-help"))
                .value_name(localizer.text("build-project-value"))
        })
        .mut_arg("workspace", |arg| {
            arg.help(localizer.text("build-workspace-help"))
        })
        .mut_arg("dry_run", |arg| {
            arg.help(localizer.text("build-dry-run-help"))
        })
        .mut_arg("manifest", |arg| {
            arg.help(localizer.text("build-manifest-help"))
        })
        .mut_arg("help", |arg| arg.help(localizer.text("cli-help")))
}

impl BuildArgs {
    /// The shell needs VCS only when execution will actually publish a provenance manifest.
    #[must_use]
    pub const fn needs_provenance(&self) -> bool {
        self.mode.manifest && !self.mode.dry_run
    }
}

pub mod artifact;
pub mod clean;
pub mod config;
pub mod import;
pub mod patch;
pub mod platform;
