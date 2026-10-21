//! Shared VCS argument metadata; repository execution stays in its process owner.

use crate::localization::Localizer;
use clap::{Args, ValueEnum};

#[derive(Debug, Args)]
pub struct StatusArgs {
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

/// Attach localized arguments without changing the command contract.
#[must_use]
pub fn localize(command: clap::Command, localizer: &Localizer) -> clap::Command {
    command
        .about(localizer.text("status-about"))
        .override_usage(localizer.text("status-usage"))
        .mut_arg("format", |argument| {
            argument
                .help(localizer.text("status-format-help"))
                .value_name(localizer.text("status-format-value"))
        })
        .mut_arg("project", |argument| {
            argument
                .help(localizer.text("status-project-help"))
                .value_name(localizer.text("status-project-value"))
        })
        .mut_arg("workspace", |argument| {
            argument.help(localizer.text("status-workspace-help"))
        })
        .mut_arg("help", |argument| argument.help(localizer.text("cli-help")))
}
