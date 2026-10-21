//! Standalone VCS parsing; repository execution and source safety use one shared implementation.

mod changes;
pub mod commands;
mod diagnostics;
mod shelves;

pub use eska_presentation::{encoding, localization};

use clap::{CommandFactory, FromArgMatches, Parser, Subcommand};
use eska_presentation::{
    arguments::bootstrap_lang,
    localization::{Localizer, resolve_locale_from_environment},
};
use std::{ffi::OsString, path::PathBuf, process::ExitCode};

#[derive(Debug, Parser)]
#[command(
    name = "eska-vcs",
    disable_help_flag = true,
    disable_version_flag = true,
    disable_help_subcommand = true
)]
struct Cli {
    #[arg(long, global = true)]
    lang: Option<String>,
    #[arg(long, global = true, default_value = ".")]
    project_dir: PathBuf,
    #[command(subcommand)]
    command: Commands,
    #[arg(short,long,action=clap::ArgAction::Help)]
    help: Option<bool>,
    #[arg(short='V',long,action=clap::ArgAction::Version)]
    version: Option<bool>,
}

#[derive(Debug, Subcommand)]
enum Commands {
    #[command(name = "__bridge", hide = true)]
    Bridge(crate::bridge::BridgeArgs),
    #[command(disable_help_flag = true)]
    Clone(commands::clone::CloneArgs),
    #[command(disable_help_flag = true)]
    Diff(commands::diff::DiffArgs),
    #[command(disable_help_flag = true)]
    Finish(commands::finish::FinishArgs),
    #[command(disable_help_flag = true)]
    History(commands::history::HistoryArgs),
    #[command(disable_help_flag = true)]
    Save(commands::save::SaveArgs),
    #[command(disable_help_flag = true)]
    Shelve(commands::shelves::ShelveArgs),
    #[command(disable_help_flag = true)]
    Unshelve(commands::shelves::UnshelveArgs),
    #[command(disable_help_flag = true)]
    Shelves(commands::shelves::ShelvesArgs),
    #[command(disable_help_flag = true)]
    Start(commands::start::StartArgs),
    #[command(disable_help_flag = true)]
    Status(commands::status::StatusArgs),
    #[command(disable_help_flag = true)]
    Switch(commands::switch::SwitchArgs),
}

/// Start one VCS command without locating a platform, project server or analyzer.
#[must_use]
pub fn run() -> ExitCode {
    let args: Vec<OsString> = std::env::args_os().collect();
    let locale = resolve_locale_from_environment(bootstrap_lang(&args).as_deref());
    let localizer = match Localizer::try_new(locale) {
        Ok(localizer) => localizer,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::FAILURE;
        }
    };
    let matches = localized(&localizer).get_matches_from(args);
    let cli = Cli::from_arg_matches(&matches).unwrap_or_else(|error| error.exit());
    match cli.command {
        Commands::Bridge(args) => args.run(&cli.project_dir),
        Commands::Clone(args) => commands::clone::run(&args, &cli.project_dir, &localizer),
        Commands::Diff(args) => commands::diff::run(&args, &cli.project_dir, &localizer),
        Commands::Finish(_) => commands::finish::run(&cli.project_dir, &localizer),
        Commands::History(args) => commands::history::run(&args, &cli.project_dir, &localizer),
        Commands::Save(args) => commands::save::run(&args, &cli.project_dir, &localizer),
        Commands::Shelve(args) => commands::shelves::shelve(&args, &cli.project_dir, &localizer),
        Commands::Unshelve(args) => {
            commands::shelves::unshelve(&args, &cli.project_dir, &localizer)
        }
        Commands::Shelves(args) => commands::shelves::list(&args, &cli.project_dir, &localizer),
        Commands::Start(args) => commands::start::run(&args, &cli.project_dir, &localizer),
        Commands::Status(args) => commands::status::run(&args, &cli.project_dir, &localizer),
        Commands::Switch(args) => commands::switch::run(&args, &cli.project_dir, &localizer),
    }
}

/// Localize inherited command help while deriving the actual standalone executable path.
fn localized(localizer: &Localizer) -> clap::Command {
    let command = Cli::command()
        .version(env!("CARGO_PKG_VERSION"))
        .about(localizer.text("vcs-tool-about"))
        .help_template(format!(
            "{{name}} {{version}}\n{{about-with-newline}}\n{}: {{usage}}\n\n{}:\n{{subcommands}}\n{}:\n{{options}}",
            localizer.text("cli-usage"),
            localizer.text("cli-commands"),
            localizer.text("cli-options"),
        ))
        .mut_arg("lang", |arg| arg.help(localizer.text("cli-lang-help")))
        .mut_arg("project_dir", |arg| arg.help(localizer.text("check-project-dir-help")))
        .mut_arg("help", |arg| arg.help(localizer.text("cli-help")))
        .mut_arg("version", |arg| arg.help(localizer.text("cli-version")))
        .mut_subcommand("clone", |command| {
            commands::clone::localize(command, localizer)
        })
        .mut_subcommand("diff", |command| {
            commands::diff::localize(command, localizer)
        })
        .mut_subcommand("finish", |command| {
            commands::finish::localize(command, localizer)
        })
        .mut_subcommand("history", |command| {
            commands::history::localize(command, localizer)
        })
        .mut_subcommand("save", |command| {
            commands::save::localize(command, localizer)
        })
        .mut_subcommand("shelve", |command| {
            commands::shelves::localize(command, localizer)
        })
        .mut_subcommand("unshelve", |command| {
            commands::shelves::localize(command, localizer)
        })
        .mut_subcommand("shelves", |command| {
            commands::shelves::localize(command, localizer)
        })
        .mut_subcommand("start", |command| {
            commands::start::localize(command, localizer)
        })
        .mut_subcommand("status", |command| {
            commands::status::localize(command, localizer)
        })
        .mut_subcommand("switch", |command| {
            commands::switch::localize(command, localizer)
        });
    command.mut_subcommands(|command| standalone_help(command, localizer))
}

/// Explicit legacy usage strings belong only to the old CLI's help, never this executable.
fn standalone_help(command: clap::Command, localizer: &Localizer) -> clap::Command {
    let arguments = if command.get_positionals().next().is_some() {
        format!("\n{}:\n{{positionals}}\n", localizer.text("cli-arguments"))
    } else {
        String::new()
    };
    let commands = if command.get_subcommands().next().is_some() {
        format!("\n{}:\n{{subcommands}}\n", localizer.text("cli-commands"))
    } else {
        String::new()
    };
    let template = format!(
        "{{about-with-newline}}\n{}: {{usage}}\n{arguments}{commands}\n{}:\n{{options}}",
        localizer.text("cli-usage"),
        localizer.text("cli-options")
    );
    command
        .override_usage(clap::builder::Resettable::Reset)
        .help_template(template)
        .mut_subcommands(|command| standalone_help(command, localizer))
}
