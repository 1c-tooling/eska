//! Shared project command options and help; metadata and transactions stay with their owner.

use crate::localization::Localizer;
use clap::{Args, Subcommand, ValueEnum};
use eska_project_core::project::version::BumpKind;

#[derive(Debug, Args)]
pub struct VersionArgs {
    #[command(subcommand)]
    pub command: Option<VersionCommand>,

    #[arg(long, global = true, value_enum, default_value_t = OutputFormat::Human)]
    pub format: OutputFormat,

    #[arg(short = 'p', long, global = true)]
    pub project: Vec<String>,

    #[arg(long, global = true)]
    pub workspace: bool,

    #[arg(short, long, action = clap::ArgAction::Help)]
    pub help: Option<bool>,
}

#[derive(Debug, Subcommand)]
pub enum VersionCommand {
    #[command(disable_help_flag = true)]
    Bump(VersionBumpArgs),
}

#[derive(Debug, Args)]
pub struct VersionBumpArgs {
    #[arg(value_enum)]
    pub kind: Bump,

    #[arg(short, long, action = clap::ArgAction::Help)]
    pub help: Option<bool>,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum Bump {
    Patch,
    Minor,
    Major,
}

#[derive(Clone, Copy, Debug, Default, ValueEnum)]
pub enum OutputFormat {
    #[default]
    Human,
    Json,
}

/// Render localized help for the command and its mutating subcommand.
#[must_use]
pub fn localize(command: clap::Command, localizer: &Localizer) -> clap::Command {
    command
        .about(localizer.text("version-about"))
        .override_usage(localizer.text("version-usage"))
        .mut_arg("format", |argument| {
            argument
                .help(localizer.text("version-format-help"))
                .value_name(localizer.text("version-format-value"))
        })
        .mut_arg("project", |argument| {
            argument
                .help(localizer.text("version-project-help"))
                .value_name(localizer.text("version-project-value"))
        })
        .mut_arg("workspace", |argument| {
            argument.help(localizer.text("version-workspace-help"))
        })
        .mut_arg("help", |argument| argument.help(localizer.text("cli-help")))
        .mut_subcommand("bump", |command| {
            command
                .about(localizer.text("version-bump-about"))
                .override_usage(localizer.text("version-bump-usage"))
                .mut_arg("kind", |argument| {
                    argument
                        .help(localizer.text("version-bump-kind-help"))
                        .value_name(localizer.text("version-bump-kind-value"))
                })
                .mut_arg("help", |argument| argument.help(localizer.text("cli-help")))
        })
}

impl From<Bump> for BumpKind {
    /// Keep clap-specific values out of the project layer.
    fn from(value: Bump) -> Self {
        match value {
            Bump::Patch => Self::Patch,
            Bump::Minor => Self::Minor,
            Bump::Major => Self::Major,
        }
    }
}
