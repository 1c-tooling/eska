//! Shared project command options and help; metadata and transactions stay with their owner.

use crate::localization::Localizer;
use clap::{Args, Subcommand, ValueEnum};
use std::path::PathBuf;

#[derive(Debug, Args)]
pub struct MetadataArgs {
    #[command(subcommand)]
    pub command: MetadataCommand,
    #[arg(short = 'p', long, global = true)]
    pub project: Option<String>,
    #[arg(long, global = true, value_enum, default_value_t = OutputFormat::Json)]
    pub format: OutputFormat,
    #[arg(short, long, action = clap::ArgAction::Help)]
    pub help: Option<bool>,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum OutputFormat {
    Json,
}

#[derive(Debug, Subcommand)]
pub enum MetadataCommand {
    #[command(disable_help_flag = true)]
    Inspect(InspectArgs),
    #[command(disable_help_flag = true)]
    Types(InputArgs),
    #[command(disable_help_flag = true)]
    Choices(InputArgs),
    #[command(disable_help_flag = true)]
    Check(InputArgs),
    #[command(disable_help_flag = true)]
    Apply(InputArgs),
    #[command(disable_help_flag = true)]
    RenamePreview(InputArgs),
    #[command(disable_help_flag = true)]
    RenameApply(InputArgs),
    #[command(disable_help_flag = true)]
    RenameRecovery(InputArgs),
}

#[derive(Debug, Args)]
pub struct InspectArgs {
    #[arg(long)]
    pub object: Option<String>,
    #[arg(short, long, action = clap::ArgAction::Help)]
    pub help: Option<bool>,
}

#[derive(Debug, Args)]
pub struct InputArgs {
    pub input: PathBuf,
    #[arg(short, long, action = clap::ArgAction::Help)]
    pub help: Option<bool>,
}

/// Localize help while keeping every machine response independent of --lang.
#[must_use]
pub fn localize(mut command: clap::Command, locale: &Localizer) -> clap::Command {
    command = command
        .about(locale.text("metadata-about"))
        .mut_arg("project", |arg| {
            arg.help(locale.text("metadata-project-help"))
        })
        .mut_arg("format", |arg| {
            arg.help(locale.text("metadata-format-help"))
        })
        .mut_arg("help", |arg| arg.help(locale.text("cli-help")));
    for name in [
        "inspect",
        "types",
        "choices",
        "check",
        "apply",
        "rename-preview",
        "rename-apply",
        "rename-recovery",
    ] {
        command = command.mut_subcommand(name, |command| {
            let command = command
                .about(locale.text(&format!("metadata-{name}-about")))
                .mut_arg("help", |arg| arg.help(locale.text("cli-help")));
            if name == "inspect" {
                command.mut_arg("object", |arg| {
                    arg.help(locale.text("metadata-object-help"))
                })
            } else {
                command.mut_arg("input", |arg| arg.help(locale.text("metadata-input-help")))
            }
        });
    }
    command
}
