//! Shared native command options and help; execution belongs to eska-build.

use crate::localization::Localizer;
use clap::{Args, Subcommand};

#[derive(Debug, Args)]
pub struct ConfigArgs {
    #[command(subcommand)]
    pub command: ConfigCommand,

    #[arg(short, long, action = clap::ArgAction::Help)]
    pub help: Option<bool>,
}

#[derive(Debug, Subcommand)]
pub enum ConfigCommand {
    #[command(disable_help_flag = true)]
    Init(ConfigInitArgs),
    #[command(disable_help_flag = true)]
    Edit(ConfigEditArgs),
}

#[derive(Debug, Args)]
pub struct ConfigInitArgs {
    #[arg(short, long, action = clap::ArgAction::Help)]
    pub help: Option<bool>,
}

#[derive(Debug, Args)]
pub struct ConfigEditArgs {
    #[arg(short, long, action = clap::ArgAction::Help)]
    pub help: Option<bool>,
}

pub fn localize(command: clap::Command, localizer: &Localizer) -> clap::Command {
    command
        .about(localizer.text("config-about"))
        .override_usage(localizer.text("config-usage"))
        .mut_arg("help", |argument| argument.help(localizer.text("cli-help")))
        .mut_subcommand("init", |command| {
            command
                .about(localizer.text("config-init-about"))
                .override_usage(localizer.text("config-init-usage"))
                .mut_arg("help", |argument| argument.help(localizer.text("cli-help")))
        })
        .mut_subcommand("edit", |command| {
            command
                .about(localizer.text("config-edit-about"))
                .override_usage(localizer.text("config-edit-usage"))
                .mut_arg("help", |argument| argument.help(localizer.text("cli-help")))
        })
}
