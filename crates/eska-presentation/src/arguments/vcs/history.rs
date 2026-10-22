//! Shared VCS argument metadata; repository execution stays in its process owner.

use crate::localization::Localizer;
use clap::{Args, ValueEnum};

const DEFAULT_LIMIT: usize = 20;
const MAX_LIMIT: usize = 1_000;

#[derive(Debug, Args)]
pub struct HistoryArgs {
    #[arg(
        short = 'n',
        long,
        default_value_t = DEFAULT_LIMIT,
        value_parser = clap::builder::RangedU64ValueParser::<usize>::new()
            .range(1..=MAX_LIMIT as u64)
    )]
    pub limit: usize,

    #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
    pub format: OutputFormat,

    #[arg(short, long, action = clap::ArgAction::Help)]
    pub help: Option<bool>,
}

#[derive(Clone, Copy, Debug, Default, ValueEnum)]
pub enum OutputFormat {
    #[default]
    Human,
    Json,
}

/// Apply localized help after the bootstrap locale has been parsed.
#[must_use]
pub fn localize(command: clap::Command, localizer: &Localizer) -> clap::Command {
    command
        .about(localizer.text("history-about"))
        .override_usage(localizer.text("history-usage"))
        .mut_arg("limit", |argument| {
            argument
                .help(localizer.text("history-limit-help"))
                .value_name(localizer.text("history-limit-value"))
        })
        .mut_arg("format", |argument| {
            argument
                .help(localizer.text("history-format-help"))
                .value_name(localizer.text("history-format-value"))
        })
        .mut_arg("help", |argument| argument.help(localizer.text("cli-help")))
}
