//! Shared native command options and help; execution belongs to eska-build.

use clap::Args;

use crate::localization::Localizer;

#[derive(Debug, Args)]
pub struct CleanArgs {
    #[arg(short = 'p', long)]
    pub project: Vec<String>,

    #[arg(long)]
    pub workspace: bool,

    #[arg(short, long, action = clap::ArgAction::Help)]
    pub help: Option<bool>,
}

/// Localize native options without invoking the runtime owner.
pub fn localize(command: clap::Command, localizer: &Localizer) -> clap::Command {
    command
        .about(localizer.text("clean-about"))
        .override_usage(localizer.text("clean-usage"))
        .mut_arg("project", |arg| {
            arg.help(localizer.text("clean-project-help"))
                .value_name(localizer.text("clean-project-value"))
        })
        .mut_arg("workspace", |arg| {
            arg.help(localizer.text("clean-workspace-help"))
        })
        .mut_arg("help", |arg| arg.help(localizer.text("cli-help")))
}
