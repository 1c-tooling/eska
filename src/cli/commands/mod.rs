//! Command registration and dispatch. Each handler owns its arguments and help.

use std::{path::Path, process::ExitCode};

use clap::Subcommand;

use super::localization::Localizer;

mod init;
mod new;
mod validate;

#[derive(Debug, Subcommand)]
pub(super) enum Commands {
    #[command(disable_help_flag = true)]
    New(new::NewArgs),
    #[command(disable_help_flag = true)]
    Init(init::InitArgs),
}

pub(super) fn run(
    command: Option<&Commands>,
    project_dir: &Path,
    localizer: &Localizer,
) -> ExitCode {
    match command {
        Some(Commands::New(args)) => args.run(project_dir, localizer),
        Some(Commands::Init(args)) => args.run(project_dir, localizer),
        None => validate::run(project_dir, localizer),
    }
}

pub(super) fn localize(command: clap::Command, localizer: &Localizer) -> clap::Command {
    command
        .mut_subcommand("new", |command| new::localize(command, localizer))
        .mut_subcommand("init", |command| init::localize(command, localizer))
}
