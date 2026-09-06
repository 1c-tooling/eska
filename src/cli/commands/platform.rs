//! Installed 1C platform discovery for the effective machine runner.

use std::{path::PathBuf, process::ExitCode};

use clap::{Args, Subcommand, ValueEnum};
use serde::Serialize;

use crate::{
    cli::localization::Localizer,
    config::{GlobalConfigError, RunnerKind, load_global},
    project::build::{Ibcmd, RunnerPreference, ToolOptions, ToolSource},
};

use super::{build::present_tool_error, config::present_error as present_config_error};

#[derive(Debug, Args)]
pub(in crate::cli) struct PlatformArgs {
    #[command(subcommand)]
    command: PlatformCommand,

    #[arg(short, long, action = clap::ArgAction::Help)]
    help: Option<bool>,
}

#[derive(Debug, Subcommand)]
enum PlatformCommand {
    #[command(disable_help_flag = true)]
    List(PlatformListArgs),
}

#[derive(Debug, Args)]
struct PlatformListArgs {
    #[arg(long)]
    ibcmd: Option<PathBuf>,

    #[arg(long)]
    platform_arch: Option<String>,

    #[arg(long)]
    distrobox: Option<String>,

    #[arg(long, value_enum, default_value_t = PlatformOutputFormat::Human)]
    format: PlatformOutputFormat,

    #[arg(short, long, action = clap::ArgAction::Help)]
    help: Option<bool>,
}

#[derive(Clone, Copy, Debug, Default, ValueEnum)]
enum PlatformOutputFormat {
    #[default]
    Human,
    Json,
}

#[derive(Serialize)]
struct PlatformDocument<'a> {
    schema_version: u8,
    platforms: Vec<PlatformEntry<'a>>,
}

#[derive(Serialize)]
struct PlatformEntry<'a> {
    version: &'a str,
    source: String,
}

impl PlatformArgs {
    /// Scan platforms without requiring a project.
    pub(super) fn run(&self, localizer: &Localizer) -> ExitCode {
        match &self.command {
            PlatformCommand::List(args) => args.run(localizer),
        }
    }
}

impl PlatformListArgs {
    /// Print every verified platform visible through the selected runner.
    fn run(&self, localizer: &Localizer) -> ExitCode {
        let options = match tool_options(
            self.ibcmd.clone(),
            self.platform_arch.clone(),
            self.distrobox.clone(),
        ) {
            Ok(options) => options,
            Err(error) => {
                eprintln!("{}", present_config_error(&error, localizer));
                return ExitCode::FAILURE;
            }
        };
        let platforms = match Ibcmd::installed(&options) {
            Ok(platforms) => platforms,
            Err(error) => {
                eprintln!("{}", present_tool_error(&error, localizer));
                return ExitCode::FAILURE;
            }
        };
        match self.format {
            PlatformOutputFormat::Human => {
                if platforms.is_empty() {
                    println!("{}", localizer.text("platform-none"));
                } else {
                    for platform in platforms {
                        println!(
                            "{}\t{}",
                            platform.version().as_str(),
                            source_label(platform.source())
                        );
                    }
                }
            }
            PlatformOutputFormat::Json => {
                let document = PlatformDocument {
                    schema_version: 1,
                    platforms: platforms
                        .iter()
                        .map(|platform| PlatformEntry {
                            version: platform.version().as_str(),
                            source: source_label(platform.source()),
                        })
                        .collect(),
                };
                let Ok(json) = serde_json::to_string_pretty(&document) else {
                    eprintln!("{}", localizer.text("platform-json-error"));
                    return ExitCode::FAILURE;
                };
                println!("{json}");
            }
        }
        ExitCode::SUCCESS
    }
}

/// Merge command-line runner values with the machine-local global config.
pub(super) fn tool_options(
    ibcmd: Option<PathBuf>,
    platform_arch: Option<String>,
    distrobox: Option<String>,
) -> Result<ToolOptions, GlobalConfigError> {
    let (_, config) = load_global()?;
    Ok(
        ToolOptions::new(ibcmd, platform_arch, distrobox).with_machine_defaults(
            runner_preference(config.build.runner),
            config.build.platform_arch,
            config.build.container,
        ),
    )
}

const fn runner_preference(runner: RunnerKind) -> RunnerPreference {
    match runner {
        RunnerKind::Auto => RunnerPreference::Auto,
        RunnerKind::Host => RunnerPreference::Host,
        RunnerKind::Distrobox => RunnerPreference::Distrobox,
    }
}

fn source_label(source: &ToolSource) -> String {
    match source {
        ToolSource::Explicit(path) | ToolSource::Path(path) | ToolSource::Standard(path) => {
            path.to_string_lossy().into_owned()
        }
        ToolSource::Distrobox { container, path } => {
            format!("{container}:{}", path.to_string_lossy())
        }
    }
}

pub(super) fn localize(command: clap::Command, localizer: &Localizer) -> clap::Command {
    command
        .about(localizer.text("platform-about"))
        .override_usage(localizer.text("platform-usage"))
        .mut_arg("help", |argument| argument.help(localizer.text("cli-help")))
        .mut_subcommand("list", |command| {
            command
                .about(localizer.text("platform-list-about"))
                .override_usage(localizer.text("platform-list-usage"))
                .mut_arg("ibcmd", |arg| {
                    arg.help(localizer.text("build-ibcmd-help"))
                        .value_name(localizer.text("build-ibcmd-value"))
                })
                .mut_arg("platform_arch", |arg| {
                    arg.help(localizer.text("build-arch-help"))
                        .value_name(localizer.text("build-arch-value"))
                })
                .mut_arg("distrobox", |arg| {
                    arg.help(localizer.text("build-distrobox-help"))
                        .value_name(localizer.text("build-distrobox-value"))
                })
                .mut_arg("format", |arg| {
                    arg.help(localizer.text("build-format-help"))
                        .value_name(localizer.text("build-format-value"))
                })
                .mut_arg("help", |argument| argument.help(localizer.text("cli-help")))
        })
}
