//! Shared VCS argument metadata; repository execution stays in its process owner.

use crate::localization::Localizer;
use clap::{Args, ValueEnum};

#[derive(Debug, Args)]
pub struct DiffArgs {
    #[arg(value_name = "REVISION", num_args = 0..=2)]
    pub revisions: Vec<String>,

    #[arg(long, requires = "revisions")]
    pub since_branch_point: bool,

    #[arg(long, conflicts_with = "format")]
    pub raw: bool,

    #[arg(long)]
    pub semantic: bool,

    #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
    pub format: OutputFormat,

    #[command(flatten)]
    pub selectors: DiffSelectors,

    #[arg(short, long, action = clap::ArgAction::Help)]
    pub help: Option<bool>,
}

#[derive(Debug, Args)]
pub struct DiffSelectors {
    #[arg(short = 'p', long)]
    pub project: Vec<String>,

    #[arg(long)]
    pub workspace: bool,
}

#[derive(Clone, Copy, Debug, Default, ValueEnum)]
pub enum OutputFormat {
    #[default]
    Human,
    Json,
}

/// Apply localized help text after clap has parsed the bootstrap locale.
#[must_use]
pub fn localize(command: clap::Command, localizer: &Localizer) -> clap::Command {
    command
        .about(localizer.text("diff-about"))
        .override_usage(localizer.text("diff-usage"))
        .mut_arg("revisions", |argument| {
            argument
                .help(localizer.text("diff-revisions-help"))
                .value_name(localizer.text("diff-revisions-value"))
        })
        .mut_arg("since_branch_point", |argument| {
            argument.help(localizer.text("diff-since-branch-point-help"))
        })
        .mut_arg("raw", |argument| {
            argument.help(localizer.text("diff-raw-help"))
        })
        .mut_arg("semantic", |argument| {
            argument.help(localizer.text("diff-semantic-help"))
        })
        .mut_arg("format", |argument| {
            argument
                .help(localizer.text("diff-format-help"))
                .value_name(localizer.text("diff-format-value"))
        })
        .mut_arg("project", |argument| {
            argument
                .help(localizer.text("diff-project-help"))
                .value_name(localizer.text("diff-project-value"))
        })
        .mut_arg("workspace", |argument| {
            argument.help(localizer.text("diff-workspace-help"))
        })
        .mut_arg("help", |argument| argument.help(localizer.text("cli-help")))
}
