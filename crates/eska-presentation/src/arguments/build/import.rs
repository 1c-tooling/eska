//! Shared native command options and help; execution belongs to eska-build.

use super::artifact::{self, PlatformArgs};
use crate::localization::Localizer;
use clap::{Args, ValueEnum};
use std::path::PathBuf;

#[derive(Debug, Args)]
pub struct ImportArgs {
    pub file: PathBuf,
    #[arg(short = 'p', long)]
    pub project: Option<String>,
    #[arg(long)]
    pub force: bool,
    #[arg(long, conflicts_with = "force")]
    pub dry_run: bool,
    #[arg(long, value_enum, default_value_t = Format::Human)]
    pub format: Format,
    #[command(flatten)]
    pub platform: PlatformArgs,
    #[arg(short, long, action = clap::ArgAction::Help)]
    pub help: Option<bool>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, ValueEnum)]
pub enum Format {
    #[default]
    Human,
    Json,
}

/// Localize every public option while retaining machine-facing flag names.
pub fn localize(command: clap::Command, localizer: &Localizer) -> clap::Command {
    artifact::localize(command, localizer)
        .about(localizer.text("artifact-import-about"))
        .override_usage(localizer.text("artifact-import-usage"))
        .help_template(format!(
            "{{about-with-newline}}\n{}: {{usage}}\n\n{}:\n{{positionals}}\n\n{}:\n{{options}}",
            localizer.text("cli-usage"),
            localizer.text("cli-arguments"),
            localizer.text("cli-options")
        ))
        .mut_arg("file", |arg| {
            arg.help(localizer.text("artifact-file-help"))
                .value_name(localizer.text("artifact-file-value"))
        })
        .mut_arg("project", |arg| {
            arg.help(localizer.text("artifact-project-help"))
                .value_name(localizer.text("version-project-value"))
        })
        .mut_arg("force", |arg| {
            arg.help(localizer.text("artifact-force-help"))
        })
        .mut_arg("dry_run", |arg| {
            arg.help(localizer.text("artifact-dry-run-help"))
        })
        .mut_arg("format", |arg| {
            arg.help(localizer.text("build-format-help"))
        })
        .mut_arg("help", |arg| arg.help(localizer.text("cli-help")))
}
