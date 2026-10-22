//! Shared VCS argument metadata; repository execution stays in its process owner.

use crate::localization::Localizer;
use clap::Args;

#[derive(Debug, Args)]
pub struct SaveArgs {
    #[arg(short, long, value_name = "MESSAGE")]
    pub message: Option<String>,

    #[arg(short = 'p', long)]
    pub project: Vec<String>,

    #[arg(long)]
    pub workspace: bool,

    #[arg(long)]
    pub dry_run: bool,

    #[arg(short, long, action = clap::ArgAction::Help)]
    pub help: Option<bool>,
}

/// Apply localized help text after clap has parsed the bootstrap locale.
#[must_use]
pub fn localize(command: clap::Command, localizer: &Localizer) -> clap::Command {
    command
        .about(localizer.text("save-about"))
        .override_usage(localizer.text("save-usage"))
        .mut_arg("message", |argument| {
            argument
                .help(localizer.text("save-message-help"))
                .value_name(localizer.text("save-message-value"))
        })
        .mut_arg("project", |argument| {
            argument
                .help(localizer.text("save-project-help"))
                .value_name(localizer.text("save-project-value"))
        })
        .mut_arg("workspace", |argument| {
            argument.help(localizer.text("save-workspace-help"))
        })
        .mut_arg("dry_run", |argument| {
            argument.help(localizer.text("save-dry-run-help"))
        })
        .mut_arg("help", |argument| argument.help(localizer.text("cli-help")))
}
