//! Command registration and dispatch. Each handler owns its arguments and help.

use std::{path::Path, process::ExitCode};

use clap::Subcommand;

use super::localization::Localizer;

mod build;
mod clone;
mod diff;
mod finish;
mod history;
mod init;
mod new;
mod save;
mod start;
mod status;
mod switch;
mod validate;

#[derive(Debug, Subcommand)]
pub(super) enum Commands {
    #[command(disable_help_flag = true)]
    Build(build::BuildArgs),
    #[command(disable_help_flag = true)]
    Clone(clone::CloneArgs),
    #[command(disable_help_flag = true)]
    New(new::NewArgs),
    #[command(disable_help_flag = true)]
    Init(init::InitArgs),
    #[command(disable_help_flag = true)]
    Diff(diff::DiffArgs),
    #[command(disable_help_flag = true)]
    Finish(finish::FinishArgs),
    #[command(disable_help_flag = true)]
    History(history::HistoryArgs),
    #[command(disable_help_flag = true)]
    Save(save::SaveArgs),
    #[command(disable_help_flag = true)]
    Start(start::StartArgs),
    #[command(disable_help_flag = true)]
    Status(status::StatusArgs),
    #[command(disable_help_flag = true)]
    Switch(switch::SwitchArgs),
}

pub(super) fn run(
    command: Option<&Commands>,
    project_dir: &Path,
    localizer: &Localizer,
) -> ExitCode {
    match command {
        Some(Commands::Build(args)) => args.run(project_dir, localizer),
        Some(Commands::Clone(args)) => args.run(project_dir, localizer),
        Some(Commands::New(args)) => args.run(project_dir, localizer),
        Some(Commands::Init(args)) => args.run(project_dir, localizer),
        Some(Commands::Diff(args)) => args.run(project_dir, localizer),
        Some(Commands::Finish(_)) => finish::FinishArgs::run(project_dir, localizer),
        Some(Commands::History(args)) => args.run(project_dir, localizer),
        Some(Commands::Save(args)) => args.run(project_dir, localizer),
        Some(Commands::Start(args)) => args.run(project_dir, localizer),
        Some(Commands::Status(args)) => args.run(project_dir, localizer),
        Some(Commands::Switch(args)) => args.run(project_dir, localizer),
        None => validate::run(project_dir, localizer),
    }
}

pub(super) fn localize(command: clap::Command, localizer: &Localizer) -> clap::Command {
    command
        .mut_subcommand("build", |command| build::localize(command, localizer))
        .mut_subcommand("clone", |command| clone::localize(command, localizer))
        .mut_subcommand("new", |command| new::localize(command, localizer))
        .mut_subcommand("init", |command| init::localize(command, localizer))
        .mut_subcommand("diff", |command| diff::localize(command, localizer))
        .mut_subcommand("finish", |command| finish::localize(command, localizer))
        .mut_subcommand("history", |command| history::localize(command, localizer))
        .mut_subcommand("save", |command| save::localize(command, localizer))
        .mut_subcommand("start", |command| start::localize(command, localizer))
        .mut_subcommand("status", |command| status::localize(command, localizer))
        .mut_subcommand("switch", |command| switch::localize(command, localizer))
}
