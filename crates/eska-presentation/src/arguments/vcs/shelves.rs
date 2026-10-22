//! Shared VCS argument metadata; repository execution stays in its process owner.

use crate::localization::Localizer;
use clap::{Args, ValueEnum};

#[derive(Debug, Args)]
pub struct ShelveArgs {
    #[command(flatten)]
    pub options: MutationOptions,
}

#[derive(Debug, Args)]
pub struct UnshelveArgs {
    pub id: Option<String>,
    #[command(flatten)]
    pub options: MutationOptions,
}

#[derive(Debug, Args)]
pub struct MutationOptions {
    #[arg(long)]
    pub dry_run: bool,
    #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
    pub format: OutputFormat,
    #[arg(short, long, action = clap::ArgAction::Help)]
    pub help: Option<bool>,
}

#[derive(Debug, Args)]
pub struct ShelvesArgs {
    #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
    pub format: OutputFormat,
    #[arg(short, long, action = clap::ArgAction::Help)]
    pub help: Option<bool>,
}

/// Attach localized help only to the arguments supported by each shelf command.
#[must_use]
pub fn localize(command: clap::Command, localizer: &Localizer) -> clap::Command {
    let name = command.get_name().to_owned();
    let mut command = command
        .about(localizer.text(&format!("{name}-about")))
        .mut_arg("format", |arg| {
            arg.help(localizer.text("shelf-format-help"))
        })
        .mut_arg("help", |arg| arg.help(localizer.text("cli-help")));
    if name != "shelves" {
        command = command.mut_arg("dry_run", |arg| {
            arg.help(localizer.text("shelf-dry-run-help"))
        });
    }
    if name == "unshelve" {
        command = command.mut_arg("id", |arg| arg.help(localizer.text("shelf-id-help")));
    }
    command
}

#[derive(Clone, Copy, Debug, Default, ValueEnum)]
pub enum OutputFormat {
    #[default]
    Human,
    Json,
}
