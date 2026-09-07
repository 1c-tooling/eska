//! Localized project-version CLI with a stable machine-readable contract.

use std::{path::Path, process::ExitCode};

use clap::{Args, Subcommand, ValueEnum};
use serde::Serialize;

use crate::{
    cli::{
        diagnostics,
        localization::{LocalizationValue, Localizer},
    },
    project::{
        discovery,
        version::{self, BumpKind, VersionError},
    },
};

#[derive(Debug, Args)]
pub(in crate::cli) struct VersionArgs {
    #[command(subcommand)]
    command: Option<VersionCommand>,

    #[arg(long, global = true, value_enum, default_value_t = OutputFormat::Human)]
    format: OutputFormat,

    #[arg(short, long, action = clap::ArgAction::Help)]
    help: Option<bool>,
}

#[derive(Debug, Subcommand)]
enum VersionCommand {
    #[command(disable_help_flag = true)]
    Bump(VersionBumpArgs),
}

#[derive(Debug, Args)]
struct VersionBumpArgs {
    #[arg(value_enum)]
    kind: Bump,

    #[arg(short, long, action = clap::ArgAction::Help)]
    help: Option<bool>,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum Bump {
    Patch,
    Minor,
    Major,
}

impl From<Bump> for BumpKind {
    /// Keep clap-specific values out of the project layer.
    fn from(value: Bump) -> Self {
        match value {
            Bump::Patch => Self::Patch,
            Bump::Minor => Self::Minor,
            Bump::Major => Self::Major,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, ValueEnum)]
enum OutputFormat {
    #[default]
    Human,
    Json,
}

#[derive(Serialize)]
struct VersionDocument<'a> {
    schema_version: u8,
    version: String,
    descriptor: &'a str,
}

#[derive(Serialize)]
struct BumpDocument<'a> {
    schema_version: u8,
    operation: &'static str,
    bump: &'static str,
    previous_version: String,
    current_version: String,
    descriptor: &'a str,
}

impl VersionArgs {
    /// Discover the project and either inspect or narrowly update its version.
    pub(super) fn run(&self, project_dir: &Path, localizer: &Localizer) -> ExitCode {
        let project = match discovery::discover(project_dir) {
            Ok(project) => project,
            Err(error) => {
                eprintln!("{}", diagnostics::present_project_error(&error, localizer));
                return ExitCode::FAILURE;
            }
        };
        match &self.command {
            None => match version::inspect(&project) {
                Ok(info) => {
                    let descriptor = relative_path(project.root(), &info.path);
                    match self.format {
                        OutputFormat::Human => println!("{}", info.version),
                        OutputFormat::Json => {
                            let document = VersionDocument {
                                schema_version: 1,
                                version: info.version.to_string(),
                                descriptor: &descriptor,
                            };
                            if !write_json(&document, localizer) {
                                return ExitCode::FAILURE;
                            }
                        }
                    }
                    ExitCode::SUCCESS
                }
                Err(error) => fail(&error, localizer),
            },
            Some(VersionCommand::Bump(arguments)) => {
                let kind = BumpKind::from(arguments.kind);
                match version::bump(&project, kind) {
                    Ok(outcome) => {
                        let descriptor = relative_path(project.root(), &outcome.path);
                        match self.format {
                            OutputFormat::Human => println!(
                                "{}",
                                localizer.format(
                                    "version-bumped",
                                    &[
                                        (
                                            "previous",
                                            LocalizationValue::Text(&outcome.previous.to_string()),
                                        ),
                                        (
                                            "current",
                                            LocalizationValue::Text(&outcome.current.to_string()),
                                        ),
                                        ("path", LocalizationValue::Text(&descriptor)),
                                    ],
                                )
                            ),
                            OutputFormat::Json => {
                                let document = BumpDocument {
                                    schema_version: 1,
                                    operation: "bump",
                                    bump: outcome.kind.as_str(),
                                    previous_version: outcome.previous.to_string(),
                                    current_version: outcome.current.to_string(),
                                    descriptor: &descriptor,
                                };
                                if !write_json(&document, localizer) {
                                    return ExitCode::FAILURE;
                                }
                            }
                        }
                        ExitCode::SUCCESS
                    }
                    Err(error) => fail(&error, localizer),
                }
            }
        }
    }
}

/// Render localized help for the command and its mutating subcommand.
pub(super) fn localize(command: clap::Command, localizer: &Localizer) -> clap::Command {
    command
        .about(localizer.text("version-about"))
        .override_usage(localizer.text("version-usage"))
        .mut_arg("format", |argument| {
            argument
                .help(localizer.text("version-format-help"))
                .value_name(localizer.text("version-format-value"))
        })
        .mut_arg("help", |argument| argument.help(localizer.text("cli-help")))
        .mut_subcommand("bump", |command| {
            command
                .about(localizer.text("version-bump-about"))
                .override_usage(localizer.text("version-bump-usage"))
                .mut_arg("kind", |argument| {
                    argument
                        .help(localizer.text("version-bump-kind-help"))
                        .value_name(localizer.text("version-bump-kind-value"))
                })
                .mut_arg("help", |argument| argument.help(localizer.text("cli-help")))
        })
}

/// Return a stable project-relative descriptor path for presentation.
fn relative_path(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .into_owned()
}

/// Serialize one versioned JSON document to stdout.
fn write_json(document: &impl Serialize, localizer: &Localizer) -> bool {
    let Ok(json) = serde_json::to_string_pretty(document) else {
        eprintln!("{}", localizer.text("version-json-error"));
        return false;
    };
    println!("{json}");
    true
}

/// Map locale-independent domain failures to concise user diagnostics.
fn fail(error: &VersionError, localizer: &Localizer) -> ExitCode {
    let message = match error {
        VersionError::Io { path, source } => localizer.format(
            "version-io-error",
            &[
                ("path", LocalizationValue::Text(&path.to_string_lossy())),
                ("reason", LocalizationValue::Text(&source.to_string())),
            ],
        ),
        VersionError::DescriptorTooLarge { path } => path_message(
            localizer,
            "version-descriptor-large",
            &path.to_string_lossy(),
        ),
        VersionError::InvalidXml { path, .. } => {
            path_message(localizer, "version-xml-invalid", &path.to_string_lossy())
        }
        VersionError::DescriptorMissing { path } => path_message(
            localizer,
            "version-descriptor-missing",
            &path.to_string_lossy(),
        ),
        VersionError::DescriptorAmbiguous { path } => path_message(
            localizer,
            "version-descriptor-ambiguous",
            &path.to_string_lossy(),
        ),
        VersionError::VersionMissing { path } => {
            path_message(localizer, "version-value-missing", &path.to_string_lossy())
        }
        VersionError::VersionAmbiguous { path } => path_message(
            localizer,
            "version-value-ambiguous",
            &path.to_string_lossy(),
        ),
        VersionError::InvalidVersion { value } => localizer.format(
            "version-value-invalid",
            &[("value", LocalizationValue::Text(value))],
        ),
        VersionError::Overflow { kind } => localizer.format(
            "version-overflow",
            &[("kind", LocalizationValue::Text(kind.as_str()))],
        ),
    };
    eprintln!("{message}");
    ExitCode::FAILURE
}

/// Format a diagnostic containing one filesystem path.
fn path_message(localizer: &Localizer, key: &str, path: &str) -> String {
    localizer.format(key, &[("path", LocalizationValue::Text(path))])
}
