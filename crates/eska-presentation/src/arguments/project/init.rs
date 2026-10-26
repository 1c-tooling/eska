//! Shared project command options and help; metadata and transactions stay with their owner.

use super::artifact::{self, PlatformArgs};
use crate::localization::Localizer;
use clap::{ArgAction, Args};
use std::path::PathBuf;

#[derive(Debug, Args)]
pub struct InitArgs {
    #[arg(default_value = ".")]
    pub path: PathBuf,
    #[arg(long)]
    pub application: Option<String>,
    #[arg(long)]
    pub source: Option<PathBuf>,
    #[arg(long)]
    pub name: Option<String>,
    #[arg(long)]
    pub workflow: Option<String>,
    #[arg(long)]
    pub no_vcs: bool,
    #[command(flatten)]
    pub platform: PlatformArgs,
    #[arg(short, long, action = ArgAction::Help)]
    pub help: Option<bool>,
}

/// Localize project options without loading a project session.
pub fn localize(command: clap::Command, localizer: &Localizer) -> clap::Command {
    artifact::localize(command, localizer)
        .mut_arg("application", |arg| {
            arg.help(localizer.text("workspace-onboarding-application-help"))
        })
        .mut_arg("platform_version", |arg| {
            arg.help(localizer.text("init-platform-version-help"))
        })
        .about(localizer.text("init-about"))
        .override_usage(localizer.text("init-usage"))
        .help_template(format!(
            "{{about-with-newline}}\n{}: {{usage}}\n\n{}:\n{{positionals}}\n\n{}:\n{{options}}",
            localizer.text("cli-usage"),
            localizer.text("cli-arguments"),
            localizer.text("cli-options")
        ))
        .mut_arg("path", |arg| {
            arg.help(localizer.text("init-path-help"))
                .value_name(localizer.text("cli-project-dir-value"))
                .hide_default_value(true)
        })
        .mut_arg("source", |arg| {
            arg.help(localizer.text("init-source-help"))
                .value_name(localizer.text("cli-project-dir-value"))
        })
        .mut_arg("name", |arg| {
            arg.help(localizer.text("init-name-help"))
                .value_name(localizer.text("init-name-value"))
        })
        .mut_arg("workflow", |arg| {
            arg.help(localizer.text("new-workflow-help"))
                .value_name(localizer.text("new-workflow-value"))
        })
        .mut_arg("no_vcs", |arg| arg.help(localizer.text("new-no-vcs-help")))
        .mut_arg("help", |arg| arg.help(localizer.text("cli-help")))
}
