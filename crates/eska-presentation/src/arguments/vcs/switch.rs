//! Shared VCS argument metadata; repository execution stays in its process owner.

use super::shelves::OutputFormat;
use crate::localization::Localizer;
use clap::{ArgGroup, Args};

#[derive(Debug, Args)]
#[command(group(
    ArgGroup::new("target")
        .required(true)
        .args(["task", "base"])
))]
pub struct SwitchArgs {
    pub task: Option<String>,

    #[arg(long)]
    pub base: bool,

    #[arg(long)]
    pub dry_run: bool,

    #[arg(long, value_enum, default_value = "human")]
    pub format: OutputFormat,

    #[arg(short, long, action = clap::ArgAction::Help)]
    pub help: Option<bool>,
}

/// Localize help without changing command and option identifiers.
#[must_use]
pub fn localize(command: clap::Command, localizer: &Localizer) -> clap::Command {
    command
        .about(localizer.text("switch-about"))
        .override_usage(localizer.text("switch-usage"))
        .mut_arg("task", |argument| {
            argument
                .help(localizer.text("switch-task-help"))
                .value_name(localizer.text("switch-task-value"))
        })
        .mut_arg("base", |argument| {
            argument.help(localizer.text("switch-base-help"))
        })
        .mut_arg("format", |argument| {
            argument.help(localizer.text("shelf-format-help"))
        })
        .mut_arg("dry_run", |argument| {
            argument.help(localizer.text("shelf-dry-run-help"))
        })
        .mut_arg("help", |argument| argument.help(localizer.text("cli-help")))
}
