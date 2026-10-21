//! Shared VCS argument metadata; repository execution stays in its process owner.

use crate::localization::Localizer;
use clap::Args;

#[derive(Debug, Args)]
pub struct FinishArgs {
    #[arg(short, long, action = clap::ArgAction::Help)]
    pub help: Option<bool>,
}

/// Attach localized arguments without changing the command contract.
#[must_use]
pub fn localize(command: clap::Command, localizer: &Localizer) -> clap::Command {
    command
        .about(localizer.text("finish-about"))
        .override_usage(localizer.text("finish-usage"))
        .mut_arg("help", |argument| argument.help(localizer.text("cli-help")))
}
