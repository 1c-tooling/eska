//! Shared VCS argument metadata; repository execution stays in its process owner.

use crate::localization::Localizer;
use clap::Args;

#[derive(Debug, Args)]
pub struct StartArgs {
    pub task: String,

    #[arg(short, long, action = clap::ArgAction::Help)]
    pub help: Option<bool>,
}

/// Attach localized arguments without changing the command contract.
#[must_use]
pub fn localize(command: clap::Command, localizer: &Localizer) -> clap::Command {
    command
        .about(localizer.text("start-about"))
        .override_usage(localizer.text("start-usage"))
        .mut_arg("task", |argument| {
            argument
                .help(localizer.text("start-task-help"))
                .value_name(localizer.text("start-task-value"))
        })
        .mut_arg("help", |argument| argument.help(localizer.text("cli-help")))
}
