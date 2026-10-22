//! Pure check option metadata shared by the optional shell and the standalone analysis owner.

use crate::localization::Localizer;
use clap::{Args, ValueEnum};
use std::path::PathBuf;

#[derive(Debug, Args)]
pub struct CheckArgs {
    #[arg(long)]
    pub application: Option<String>,
    #[arg(long)]
    pub analyzer: Option<PathBuf>,
    #[arg(long, default_value_t=600, value_parser=clap::value_parser!(u64).range(1..))]
    pub timeout: u64,
    #[arg(long, value_enum, default_value_t=Format::Human)]
    pub format: Format,
    #[arg(long)]
    pub report: Option<PathBuf>,
    #[arg(short, long, action=clap::ArgAction::Help)]
    help: Option<bool>,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum Format {
    Human,
    Json,
}

/// Keep both entry points' analysis flags and descriptions identical without invoking a component.
#[must_use]
pub fn localize(mut command: clap::Command, localizer: &Localizer) -> clap::Command {
    command = command.about(localizer.text("check-tool-about"));
    for (name, key) in [
        ("application", "project-application-help"),
        ("analyzer", "analyzer-binary-help"),
        ("timeout", "analyzer-timeout-help"),
        ("format", "version-format-help"),
        ("report", "check-report-help"),
        ("help", "cli-help"),
    ] {
        command = command.mut_arg(name, |arg| arg.help(localizer.text(key)));
    }
    command
}
