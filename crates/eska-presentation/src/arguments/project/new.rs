//! Shared project command options and help; metadata and transactions stay with their owner.

use super::artifact::PlatformArgs;
use crate::localization::Localizer;
use clap::{ArgAction, Args};
use std::path::PathBuf;

#[derive(Debug, Args)]
pub struct NewArgs {
    pub path: PathBuf,
    #[arg(long, conflicts_with = "project_type")]
    pub from: Option<PathBuf>,
    #[command(flatten)]
    pub platform: PlatformArgs,
    #[arg(long = "type")]
    pub project_type: Option<String>,
    #[arg(long)]
    pub workflow: Option<String>,
    #[arg(long)]
    pub no_vcs: bool,
    #[arg(short, long, action = ArgAction::Help)]
    pub help: Option<bool>,
}

/// Localize project options without loading a project session.
pub fn localize(command: clap::Command, localizer: &Localizer) -> clap::Command {
    super::artifact::localize(command, localizer)
        .mut_arg("from", |arg| arg.help(localizer.text("artifact-from-help")))
        .about(localizer.text("new-about"))
        .override_usage(localizer.text("new-usage"))
        .help_template(format!(
            "{{about-with-newline}}\n{}: {{usage}}\n\n{}:\n{{positionals}}\n\n{}:\n{{options}}",
            localizer.text("cli-usage"),
            localizer.text("cli-arguments"),
            localizer.text("cli-options")
        ))
        .mut_arg("path", |arg| {
            arg.help(localizer.text("new-path-help"))
                .value_name(localizer.text("cli-project-dir-value"))
        })
        .mut_arg("project_type", |arg| {
            arg.help(localizer.text("new-type-help"))
                .value_name(localizer.text("new-type-value"))
        })
        .mut_arg("workflow", |arg| {
            arg.help(localizer.text("new-workflow-help"))
                .value_name(localizer.text("new-workflow-value"))
        })
        .mut_arg("no_vcs", |arg| arg.help(localizer.text("new-no-vcs-help")))
        .mut_arg("help", |arg| arg.help(localizer.text("cli-help")))
}
