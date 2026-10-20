//! Standalone platform command parsing; source replacement remains an orchestration operation.

pub mod commands;
pub mod diagnostics;
pub mod platform;

pub use eska_presentation::{encoding, localization, path_output};
pub use eska_terminal::{interactive, process_output};

use clap::{CommandFactory, FromArgMatches, Parser, Subcommand};
use eska_build_runtime::build::GitProvenance;
use eska_presentation::{
    arguments::bootstrap_lang,
    localization::{Localizer, resolve_locale_from_environment},
};
use std::{
    ffi::OsString,
    path::{Path, PathBuf},
    process::ExitCode,
};

#[derive(Debug, Parser)]
#[command(
    name = "eska-build",
    disable_help_flag = true,
    disable_version_flag = true,
    disable_help_subcommand = true
)]
struct Cli {
    #[arg(long, global = true, hide = true)]
    vcs_binary: Option<PathBuf>,
    #[arg(long, global = true)]
    lang: Option<String>,
    #[arg(long, global = true, default_value = ".")]
    project_dir: PathBuf,
    #[command(subcommand)]
    command: Option<Commands>,
    #[arg(short,long,action=clap::ArgAction::Help)]
    help: Option<bool>,
    #[arg(short='V',long,action=clap::ArgAction::Version)]
    version: Option<bool>,
}

#[derive(Debug, Subcommand)]
enum Commands {
    #[command(disable_help_flag = true)]
    Build(commands::build::BuildArgs),
    #[command(disable_help_flag = true)]
    Unpack(commands::unpack::UnpackArgs),
    #[command(disable_help_flag = true)]
    Clean(commands::clean::CleanArgs),
    #[command(disable_help_flag = true)]
    Platform(commands::platform::PlatformArgs),
    #[command(disable_help_flag = true)]
    Config(commands::config::ConfigArgs),
}

/// Parse standalone help before touching the project or locating a 1C installation.
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
    let vcs_binary = match cli
        .vcs_binary
        .as_deref()
        .map(|path| eska_tool_runtime::resolve_at(eska_tool_runtime::Component::Vcs, path))
        .transpose()
    {
        Ok(binary) => binary,
        Err(error) => {
            eprintln!(
                "{}",
                eska_presentation::components::present(&error, &localizer)
            );
            return ExitCode::FAILURE;
        }
    };
    let provenance = |root: &Path| query_provenance(vcs_binary.as_deref(), root);
    match cli.command {
        Some(Commands::Build(args)) => args.run(&cli.project_dir, &localizer, &provenance),
        Some(Commands::Unpack(args)) => args.run(&cli.project_dir, &localizer),
        Some(Commands::Clean(args)) => args.run(&cli.project_dir, &localizer),
        Some(Commands::Platform(args)) => args.run(&localizer),
        Some(Commands::Config(args)) => args.run(&localizer),
        None => {
            commands::build::BuildArgs::default().run(&cli.project_dir, &localizer, &provenance)
        }
    }
}

/// Standalone builds keep provenance unknown; an explicitly selected VCS must answer correctly.
fn query_provenance(
    binary: Option<&Path>,
    root: &Path,
) -> Result<GitProvenance, eska_tool_runtime::Error> {
    let Some(binary) = binary else {
        return Ok(GitProvenance::unavailable());
    };
    let value = eska_tool_runtime::query(
        eska_tool_runtime::Component::Vcs,
        binary,
        "provenance",
        root,
        &[],
    )?;
    serde_json::from_value(value).map_err(|_| eska_tool_runtime::Error::InvalidResponse)
}

/// Reuse command help while allowing clap to generate the actual standalone executable paths.
fn localized(localizer: &Localizer) -> clap::Command {
    let command=Cli::command().version(env!("CARGO_PKG_VERSION")).about(localizer.text("build-tool-about"))
        .help_template(format!("{{name}} {{version}}\n{{about-with-newline}}\n{}: {{usage}}\n\n{}:\n{{subcommands}}\n{}:\n{{options}}",localizer.text("cli-usage"),localizer.text("cli-commands"),localizer.text("cli-options")))
        .mut_arg("lang",|arg|arg.help(localizer.text("cli-lang-help")))
        .mut_arg("project_dir",|arg|arg.help(localizer.text("check-project-dir-help")))
        .mut_arg("help",|arg|arg.help(localizer.text("cli-help")))
        .mut_arg("version",|arg|arg.help(localizer.text("cli-version")))
        .mut_subcommand("build",|command|commands::build::localize(command,localizer))
        .mut_subcommand("unpack", |command| commands::unpack::localize(command, localizer))
        .mut_subcommand("clean",|command|commands::clean::localize(command,localizer))
        .mut_subcommand("platform",|command|commands::platform::localize(command,localizer))
        .mut_subcommand("config",|command|commands::config::localize(command,localizer));
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
