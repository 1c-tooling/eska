//! Deterministic standalone project CLI; the shared core owns every domain operation.

use std::{
    ffi::OsString,
    path::{Path, PathBuf},
    process::ExitCode,
};

use clap::{ArgAction, Args, CommandFactory, FromArgMatches, Parser, Subcommand, ValueEnum};
use eska_presentation::{
    arguments::bootstrap_lang,
    localization::{Localizer, resolve_locale_from_environment},
};

use crate::{context, operations};

#[derive(Debug, Parser)]
#[command(
    name = "eska-project",
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
    command: Option<Commands>,
    #[arg(short, long, action = ArgAction::Help)]
    help: Option<bool>,
    #[arg(short = 'V', long, action = ArgAction::Version)]
    version: Option<bool>,
}

#[derive(Debug, Subcommand)]
enum Commands {
    #[command(name = "__shell-new", hide = true, disable_help_flag = true)]
    ShellNew(crate::onboarding::new::NewArgs),
    #[command(name = "__shell-init", hide = true, disable_help_flag = true)]
    ShellInit(crate::onboarding::init::InitArgs),
    #[command(name = "__shell-validate", hide = true)]
    ShellValidate,
    #[command(disable_help_flag = true)]
    Serve(ServeArgs),
    #[command(disable_help_flag = true)]
    AnalyzerConfig(crate::analyzer_config::ConfigArgs),
    #[command(disable_help_flag = true)]
    Validate(ValidateArgs),
    #[command(disable_help_flag = true)]
    Context(ContextArgs),
    #[command(disable_help_flag = true)]
    New(NewArgs),
    #[command(disable_help_flag = true)]
    Init(InitArgs),
    #[command(disable_help_flag = true)]
    Metadata(crate::metadata::MetadataArgs),
    #[command(disable_help_flag = true)]
    Version(crate::version::VersionArgs),
}

#[derive(Debug, Args)]
struct ServeArgs {
    #[arg(long, hide = true)]
    legacy_identity: bool,
    #[arg(long, required = true)]
    stdio: bool,
    #[arg(short,long,action=ArgAction::Help)]
    help: Option<bool>,
}

#[derive(Clone, Copy, Debug, Default, ValueEnum)]
pub enum OutputFormat {
    #[default]
    Human,
    Json,
}

#[derive(Debug, Default, Args)]
pub struct ValidateArgs {
    #[arg(short = 'p', long)]
    pub project: Vec<String>,
    #[arg(long)]
    pub workspace: bool,
    #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
    pub format: OutputFormat,
    #[arg(short, long, action = ArgAction::Help)]
    help: Option<bool>,
}

#[derive(Debug, Args)]
pub struct ContextArgs {
    #[arg(long)]
    pub application: Option<String>,
    #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
    pub format: OutputFormat,
    #[arg(short, long, action = ArgAction::Help)]
    help: Option<bool>,
}

#[derive(Debug, Args)]
pub struct NewArgs {
    pub path: PathBuf,
    #[arg(long = "type", required = true)]
    pub project_type: String,
    #[arg(long, default_value = "src")]
    pub source: PathBuf,
    #[arg(long)]
    pub platform_version: Option<String>,
    #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
    pub format: OutputFormat,
    #[arg(short, long, action = ArgAction::Help)]
    help: Option<bool>,
}

#[derive(Debug, Args)]
pub struct InitArgs {
    #[arg(default_value = ".")]
    pub path: PathBuf,
    #[arg(long)]
    pub source: Option<PathBuf>,
    #[arg(long)]
    pub name: Option<String>,
    #[arg(long)]
    pub platform_version: Option<String>,
    #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
    pub format: OutputFormat,
    #[arg(short, long, action = ArgAction::Help)]
    help: Option<bool>,
}

/// Run standalone operations without locating the full wrapper, Git, a platform or an analyzer.
#[must_use]
pub fn run() -> ExitCode {
    let args: Vec<OsString> = std::env::args_os().collect();
    let requested = bootstrap_lang(&args);
    let locale = resolve_locale_from_environment(requested.as_deref());
    let localizer = match Localizer::try_new(locale) {
        Ok(localizer) => localizer,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::FAILURE;
        }
    };
    let matches = localized(&localizer).get_matches_from(args);
    let cli = Cli::from_arg_matches(&matches).unwrap_or_else(|error| error.exit());
    match cli.command.as_ref() {
        Some(Commands::ShellNew(args)) => args.run(&cli.project_dir, &localizer),
        Some(Commands::ShellInit(args)) => args.run(&cli.project_dir, &localizer),
        Some(Commands::ShellValidate) => shell_validate(&cli.project_dir, &localizer),
        Some(Commands::Serve(args)) => {
            if args.stdio {
                if args.legacy_identity {
                    crate::ide::run()
                } else {
                    crate::ide::serve()
                }
            } else {
                ExitCode::from(2)
            }
        }
        Some(Commands::AnalyzerConfig(args)) => args.run(&cli.project_dir, &localizer),
        Some(Commands::Metadata(args)) => args.run(&cli.project_dir, &localizer),
        Some(Commands::Version(args)) => args.run(&cli.project_dir, &localizer),
        Some(Commands::Context(args)) => context::run(&cli.project_dir, args, &localizer),
        Some(Commands::New(args)) => operations::new(&cli.project_dir, args, &localizer),
        Some(Commands::Init(args)) => operations::init(&cli.project_dir, args, &localizer),
        Some(Commands::Validate(args)) => operations::validate(&cli.project_dir, args, &localizer),
        None => operations::validate(&cli.project_dir, &ValidateArgs::default(), &localizer),
    }
}

/// Localize help before parsing so the selected language also covers early command help.
fn localized(localizer: &Localizer) -> clap::Command {
    Cli::command().version(env!("CARGO_PKG_VERSION")).about(localizer.text("project-tool-about"))
        .help_template(format!("{{name}} {{version}}\n{{about-with-newline}}\n{}: {{usage}}\n\n{}:\n{{subcommands}}\n{}:\n{{options}}", localizer.text("cli-usage"), localizer.text("cli-commands"), localizer.text("cli-options")))
        .mut_arg("lang", |arg| arg.help(localizer.text("cli-lang-help")).value_name(localizer.text("cli-lang-value")))
        .mut_arg("project_dir", |arg| arg.help(localizer.text("cli-project-dir-help")).value_name(localizer.text("cli-project-dir-value")))
        .mut_arg("help", |arg| arg.help(localizer.text("cli-help")))
        .mut_arg("version", |arg| arg.help(localizer.text("cli-version")))
        .mut_subcommands(|command| match command.get_name() {
            "analyzer-config"=>crate::analyzer_config::localize(command,localizer),
            "metadata" => standalone_help(crate::metadata::localize(command, localizer)),
            "version" => standalone_help(crate::version::localize(command, localizer)),
            _ => localize_operation(command, localizer),
        })
}

/// Shared legacy commands retain their existing help; standalone usage comes from actual clap paths.
fn standalone_help(command: clap::Command) -> clap::Command {
    command
        .override_usage(clap::builder::Resettable::Reset)
        .mut_subcommands(standalone_help)
}

/// Give project-specific commands the same localized flags and value names as the full CLI.
fn localize_operation(mut command: clap::Command, localizer: &Localizer) -> clap::Command {
    let about = match command.get_name() {
        "new" => "project-new-about",
        "init" => "project-init-about",
        "context" => "project-context-about",
        "serve" => "project-serve-about",
        _ => "project-validate-about",
    };
    command = command.about(localizer.text(about)).help_template(format!(
        "{{about-with-newline}}\n{}: {{usage}}\n\n{}:\n{{options}}",
        localizer.text("cli-usage"),
        localizer.text("cli-options")
    ));
    for (name, help, value) in [
        ("help", "cli-help", None),
        ("stdio", "ide-stdio-help", None),
        (
            "format",
            "version-format-help",
            Some("version-format-value"),
        ),
        (
            "application",
            "project-application-help",
            Some("project-application-value"),
        ),
        (
            "project_type",
            "project-new-type-help",
            Some("new-type-value"),
        ),
        ("source", "init-source-help", Some("cli-project-dir-value")),
        ("name", "init-name-help", Some("init-name-value")),
        (
            "platform_version",
            "project-platform-help",
            Some("build-platform-version-value"),
        ),
        (
            "project",
            "version-project-help",
            Some("version-project-value"),
        ),
        ("workspace", "version-workspace-help", None),
        ("path", "project-path-help", Some("cli-project-dir-value")),
    ] {
        if command
            .get_arguments()
            .any(|argument| argument.get_id() == name)
        {
            command = command.mut_arg(name, |arg| {
                let arg = arg.help(localizer.text(help));
                match value {
                    Some(key) => arg.value_name(localizer.text(key)),
                    None => arg,
                }
            });
        }
    }
    command
}

/// Resolve a command path relative to the explicitly selected discovery directory.
pub fn command_path(start: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.into()
    } else {
        start.join(path)
    }
}

/// Preserve the quiet manifest-only default of the delegating shell.
fn shell_validate(start: &Path, localizer: &Localizer) -> ExitCode {
    match eska_project_core::project::discovery::discover_context(start) {
        Ok(_) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!(
                "{}",
                eska_presentation::diagnostics::present_context_error(&error, localizer)
            );
            ExitCode::FAILURE
        }
    }
}
