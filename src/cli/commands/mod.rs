//! Command registration and dispatch. Each handler owns its arguments and help.

use crate::component::{self, Component, Invocation};
use std::{ffi::OsString, path::Path, process::ExitCode};

use clap::Subcommand;

use super::localization::Localizer;

mod build;
mod clean;
mod clone;
mod config;
mod diff;
mod doctor;
mod finish;
mod history;
mod ide;
mod import;
mod init;
mod metadata;
mod new;
mod patch;
mod platform;
mod save;
mod shelves;
mod start;
mod status;
mod switch;
mod update;
mod version;

#[derive(Debug, Subcommand)]
pub(super) enum Commands {
    #[command(disable_help_flag = true)]
    Update(update::UpdateArgs),
    #[command(disable_help_flag = true)]
    Metadata(metadata::MetadataArgs),
    #[command(disable_help_flag = true)]
    Ide(ide::IdeArgs),
    #[command(disable_help_flag = true)]
    Build(build::BuildArgs),
    #[command(disable_help_flag = true)]
    Clean(clean::CleanArgs),
    #[command(disable_help_flag = true)]
    Clone(clone::CloneArgs),
    #[command(disable_help_flag = true)]
    Config(config::ConfigArgs),
    #[command(disable_help_flag = true)]
    New(new::NewArgs),
    #[command(disable_help_flag = true)]
    Init(init::InitArgs),
    #[command(disable_help_flag = true)]
    Import(import::ImportArgs),
    #[command(disable_help_flag = true)]
    Platform(platform::PlatformArgs),
    #[command(disable_help_flag = true)]
    Patch(patch::PatchArgs),
    #[command(disable_help_flag = true)]
    Diff(diff::DiffArgs),
    #[command(disable_help_flag = true)]
    Doctor(doctor::DoctorArgs),
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
    #[command(disable_help_flag = true)]
    Shelve(shelves::ShelveArgs),
    #[command(disable_help_flag = true)]
    Unshelve(shelves::UnshelveArgs),
    #[command(disable_help_flag = true)]
    Shelves(shelves::ShelvesArgs),
    #[command(disable_help_flag = true)]
    Version(version::VersionArgs),
}

pub(super) fn run(
    command: Option<&Commands>,
    project_dir: &Path,
    localizer: &Localizer,
    process_args: &[OsString],
) -> ExitCode {
    match command {
        Some(Commands::Update(args)) => args.run(localizer),
        Some(Commands::Metadata(_) | Commands::Version(_)) => component::run(
            Component::Project,
            process_args,
            Invocation::Direct,
            localizer,
        ),
        Some(Commands::Ide(_)) => component::run(
            Component::Project,
            process_args,
            Invocation::LegacyIde,
            localizer,
        ),
        Some(
            Commands::Shelve(_)
            | Commands::Unshelve(_)
            | Commands::Shelves(_)
            | Commands::Clone(_)
            | Commands::Diff(_)
            | Commands::Finish(_)
            | Commands::History(_)
            | Commands::Save(_)
            | Commands::Start(_)
            | Commands::Status(_)
            | Commands::Switch(_),
        ) => component::run(Component::Vcs, process_args, Invocation::Direct, localizer),
        Some(
            Commands::Clean(_)
            | Commands::Config(_)
            | Commands::Platform(_)
            | Commands::Import(_)
            | Commands::Patch(_),
        ) => component::run(
            Component::Build,
            process_args,
            Invocation::Direct,
            localizer,
        ),
        Some(Commands::Build(args)) => component::run(
            Component::Build,
            process_args,
            Invocation::Build {
                provenance: args.needs_provenance(),
            },
            localizer,
        ),
        Some(Commands::New(_)) => component::run(
            Component::Project,
            process_args,
            Invocation::LegacyNew,
            localizer,
        ),
        Some(Commands::Init(_)) => component::run(
            Component::Project,
            process_args,
            Invocation::LegacyInit,
            localizer,
        ),
        Some(Commands::Doctor(args)) => args.run(project_dir, localizer),
        None => component::run(
            Component::Project,
            process_args,
            Invocation::LegacyValidate,
            localizer,
        ),
    }
}

pub(super) fn localize(command: clap::Command, localizer: &Localizer) -> clap::Command {
    command
        .mut_subcommand("metadata", |command| metadata::localize(command, localizer))
        .mut_subcommand("update", |command| update::localize(command, localizer))
        .mut_subcommand("ide", |command| ide::localize(command, localizer))
        .mut_subcommand("build", |command| build::localize(command, localizer))
        .mut_subcommand("clean", |command| clean::localize(command, localizer))
        .mut_subcommand("clone", |command| clone::localize(command, localizer))
        .mut_subcommand("config", |command| config::localize(command, localizer))
        .mut_subcommand("new", |command| new::localize(command, localizer))
        .mut_subcommand("init", |command| init::localize(command, localizer))
        .mut_subcommand("import", |command| import::localize(command, localizer))
        .mut_subcommand("platform", |command| platform::localize(command, localizer))
        .mut_subcommand("patch", |command| patch::localize(command, localizer))
        .mut_subcommand("diff", |command| diff::localize(command, localizer))
        .mut_subcommand("doctor", |command| doctor::localize(command, localizer))
        .mut_subcommand("finish", |command| finish::localize(command, localizer))
        .mut_subcommand("history", |command| history::localize(command, localizer))
        .mut_subcommand("save", |command| save::localize(command, localizer))
        .mut_subcommand("start", |command| start::localize(command, localizer))
        .mut_subcommand("status", |command| status::localize(command, localizer))
        .mut_subcommand("switch", |command| switch::localize(command, localizer))
        .mut_subcommand("shelve", |command| shelves::localize(command, localizer))
        .mut_subcommand("unshelve", |command| shelves::localize(command, localizer))
        .mut_subcommand("shelves", |command| shelves::localize(command, localizer))
        .mut_subcommand("version", |command| version::localize(command, localizer))
}
