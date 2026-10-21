//! Shared native command options and help; execution belongs to eska-build.

use crate::localization::Localizer;
use clap::{Args, Subcommand, ValueEnum};
use std::path::PathBuf;

#[derive(Debug, Args)]
pub struct PlatformArgs {
    #[command(subcommand)]
    pub command: PlatformCommand,

    #[arg(short, long, action = clap::ArgAction::Help)]
    pub help: Option<bool>,
}

#[derive(Debug, Subcommand)]
pub enum PlatformCommand {
    #[command(disable_help_flag = true)]
    List(PlatformListArgs),
}

#[derive(Debug, Args)]
pub struct PlatformListArgs {
    #[arg(long)]
    pub ibcmd: Option<PathBuf>,

    #[arg(long)]
    pub platform_arch: Option<String>,

    #[arg(long)]
    pub distrobox: Option<String>,

    #[arg(long, value_enum, default_value_t = PlatformOutputFormat::Human)]
    pub format: PlatformOutputFormat,

    #[arg(short, long, action = clap::ArgAction::Help)]
    pub help: Option<bool>,
}

#[derive(Clone, Copy, Debug, Default, ValueEnum)]
pub enum PlatformOutputFormat {
    #[default]
    Human,
    Json,
}

pub fn localize(command: clap::Command, localizer: &Localizer) -> clap::Command {
    command
        .about(localizer.text("platform-about"))
        .override_usage(localizer.text("platform-usage"))
        .mut_arg("help", |argument| argument.help(localizer.text("cli-help")))
        .mut_subcommand("list", |command| {
            command
                .about(localizer.text("platform-list-about"))
                .override_usage(localizer.text("platform-list-usage"))
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
                    arg.help(localizer.text("build-format-help"))
                        .value_name(localizer.text("build-format-value"))
                })
                .mut_arg("help", |argument| argument.help(localizer.text("cli-help")))
        })
}
