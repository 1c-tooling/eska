//! Localized project-version CLI with a stable machine-readable contract.

use std::{path::Path, process::ExitCode};

use clap::{Args, Subcommand, ValueEnum};
use serde::Serialize;

mod migration;

use crate::{
    cli::{
        diagnostics,
        localization::{LocalizationValue, Localizer},
    },
    project::{
        discovery,
        selection::{
            ProjectSelection, SelectedProject, SelectionError, SelectionIntent, select_projects,
        },
        version::{self, BumpKind, ProjectVersion, VersionError, VersionScheme},
    },
};

#[derive(Debug, Args)]
pub(in crate::cli) struct VersionArgs {
    #[command(subcommand)]
    command: Option<VersionCommand>,

    #[arg(long, global = true, value_enum, default_value_t = OutputFormat::Human)]
    format: OutputFormat,

    #[arg(short = 'p', long, global = true)]
    project: Vec<String>,

    #[arg(long, global = true)]
    workspace: bool,

    #[arg(short, long, action = clap::ArgAction::Help)]
    help: Option<bool>,
}

#[derive(Debug, Subcommand)]
enum VersionCommand {
    #[command(disable_help_flag = true)]
    Bump(VersionBumpArgs),
    #[command(disable_help_flag = true)]
    Migrate(migration::MigrationArgs),
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
    #[serde(skip_serializing_if = "Option::is_none")]
    version_scheme: Option<&'static str>,
    descriptor: &'a str,
}

#[derive(Serialize)]
struct BumpDocument<'a> {
    schema_version: u8,
    operation: &'static str,
    bump: &'static str,
    previous_version: String,
    current_version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    version_scheme: Option<&'static str>,
    descriptor: &'a str,
}

#[derive(Serialize)]
struct WorkspaceVersionDocument {
    schema_version: u8,
    projects: Vec<WorkspaceVersionEntry>,
}

#[derive(Serialize)]
struct WorkspaceVersionEntry {
    name: String,
    #[serde(rename = "type")]
    project_type: &'static str,
    version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    version_scheme: Option<&'static str>,
    descriptor: String,
}

impl VersionArgs {
    /// Discover the project and either inspect or narrowly update its version.
    pub(super) fn run(&self, project_dir: &Path, localizer: &Localizer) -> ExitCode {
        let context = match discovery::discover_context(project_dir) {
            Ok(context) => context,
            Err(error) => {
                eprintln!("{}", diagnostics::present_context_error(&error, localizer));
                return ExitCode::FAILURE;
            }
        };
        let intent = if self.command.is_some() {
            SelectionIntent::SingleMutation
        } else {
            SelectionIntent::ReadOnly
        };
        let selection = match select_projects(&context, &self.project, self.workspace, intent) {
            Ok(selection) => selection,
            Err(error) => return fail_selection(&error, localizer),
        };
        match &self.command {
            None => inspect_selection(&selection, self.format, localizer),
            Some(VersionCommand::Bump(arguments)) => bump_selection(
                &selection,
                BumpKind::from(arguments.kind),
                self.format,
                localizer,
            ),
            Some(VersionCommand::Migrate(arguments)) => {
                migration::run(&selection, arguments, self.format, localizer)
            }
        }
    }
}

fn inspect_selection(
    selection: &ProjectSelection<'_>,
    format: OutputFormat,
    localizer: &Localizer,
) -> ExitCode {
    if !selection.is_aggregate() {
        let Some(selected) = selection.projects().first().copied() else {
            eprintln!("{}", localizer.text("version-selection-empty"));
            return ExitCode::FAILURE;
        };
        return inspect_one(selected, format, localizer);
    }

    let mut projects = Vec::with_capacity(selection.projects().len());
    for selected in selection.projects() {
        let Some(name) = selected.name() else {
            eprintln!("{}", localizer.text("version-selection-empty"));
            return ExitCode::FAILURE;
        };
        let info = match version::inspect(selected.project()) {
            Ok(info) => info,
            Err(error) => return fail_member(name.as_str(), &error, localizer),
        };
        projects.push(WorkspaceVersionEntry {
            name: name.as_str().to_owned(),
            project_type: selected.project().configuration().project_type().as_str(),
            version: info.version.to_string(),
            version_scheme: Some(info.version.scheme().as_str()),
            descriptor: relative_path(selected.project().root(), &info.path),
        });
    }
    match format {
        OutputFormat::Human => {
            for project in &projects {
                println!("{}: {}", project.name, project.version);
            }
            ExitCode::SUCCESS
        }
        OutputFormat::Json => {
            let extended = projects
                .iter()
                .any(|project| project.version_scheme == Some(VersionScheme::ThreePart.as_str()));
            if !extended {
                for project in &mut projects {
                    project.version_scheme = None;
                }
            }
            let document = WorkspaceVersionDocument {
                schema_version: if extended { 2 } else { 1 },
                projects,
            };
            write_json_result(&document, localizer)
        }
    }
}

fn inspect_one(
    selected: SelectedProject<'_>,
    format: OutputFormat,
    localizer: &Localizer,
) -> ExitCode {
    let info = match version::inspect(selected.project()) {
        Ok(info) => info,
        Err(error) => return fail(&error, localizer),
    };
    let descriptor = relative_path(selected.project().root(), &info.path);
    match format {
        OutputFormat::Human => {
            println!("{}", info.version);
            ExitCode::SUCCESS
        }
        OutputFormat::Json => {
            let (schema_version, version_scheme) = json_scheme(&info.version);
            let document = VersionDocument {
                schema_version,
                version: info.version.to_string(),
                version_scheme,
                descriptor: &descriptor,
            };
            write_json_result(&document, localizer)
        }
    }
}

fn bump_selection(
    selection: &ProjectSelection<'_>,
    kind: BumpKind,
    format: OutputFormat,
    localizer: &Localizer,
) -> ExitCode {
    let Some(selected) = selection.projects().first().copied() else {
        eprintln!("{}", localizer.text("version-selection-empty"));
        return ExitCode::FAILURE;
    };
    let outcome = match version::bump(selected.project(), kind) {
        Ok(outcome) => outcome,
        Err(error) => return fail(&error, localizer),
    };
    let descriptor = relative_path(selected.project().root(), &outcome.path);
    match format {
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
            let (schema_version, version_scheme) = json_scheme(&outcome.current);
            let document = BumpDocument {
                schema_version,
                operation: "bump",
                bump: outcome.kind.as_str(),
                previous_version: outcome.previous.to_string(),
                current_version: outcome.current.to_string(),
                version_scheme,
                descriptor: &descriptor,
            };
            if !write_json(&document, localizer) {
                return ExitCode::FAILURE;
            }
        }
    }
    ExitCode::SUCCESS
}

/// Retain the exact legacy response and identify the new numeric profile explicitly.
const fn json_scheme(version: &ProjectVersion) -> (u8, Option<&'static str>) {
    match version.scheme() {
        VersionScheme::FourPart => (1, None),
        VersionScheme::ThreePart => (2, Some(VersionScheme::ThreePart.as_str())),
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
        .mut_arg("project", |argument| {
            argument
                .help(localizer.text("version-project-help"))
                .value_name(localizer.text("version-project-value"))
        })
        .mut_arg("workspace", |argument| {
            argument.help(localizer.text("version-workspace-help"))
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
        .mut_subcommand("migrate", |command| migration::localize(command, localizer))
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

fn write_json_result(document: &impl Serialize, localizer: &Localizer) -> ExitCode {
    if write_json(document, localizer) {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

fn fail_selection(error: &SelectionError, localizer: &Localizer) -> ExitCode {
    let message = match error {
        SelectionError::InvalidName(error) => localizer.format(
            "version-project-name-invalid",
            &[("name", LocalizationValue::Text(error.value()))],
        ),
        SelectionError::DuplicateSelector { name } => localizer.format(
            "version-project-duplicate",
            &[("name", LocalizationValue::Text(name.as_str()))],
        ),
        SelectionError::UnknownProject { name } => localizer.format(
            "version-project-unknown",
            &[("name", LocalizationValue::Text(name.as_str()))],
        ),
        SelectionError::WorkspaceSelectorForStandalone => {
            localizer.text("version-selector-standalone")
        }
        SelectionError::ConflictingSelectors => localizer.text("version-selector-conflict"),
        SelectionError::ExplicitProjectRequired => localizer.text("version-bump-project-required"),
        SelectionError::SingleProjectRequired => {
            localizer.text("version-bump-single-project-required")
        }
    };
    eprintln!("{message}");
    ExitCode::FAILURE
}

fn fail_member(name: &str, error: &VersionError, localizer: &Localizer) -> ExitCode {
    let detail = version_error_message(error, localizer);
    eprintln!(
        "{}",
        localizer.format(
            "version-member-error",
            &[
                ("name", LocalizationValue::Text(name)),
                ("reason", LocalizationValue::Text(&detail)),
            ],
        )
    );
    ExitCode::FAILURE
}

/// Map locale-independent domain failures to concise user diagnostics.
fn fail(error: &VersionError, localizer: &Localizer) -> ExitCode {
    eprintln!("{}", version_error_message(error, localizer));
    ExitCode::FAILURE
}

/// Reuse version diagnostics when the same explicit policy blocks a native build.
pub(in crate::cli) fn version_error_message(error: &VersionError, localizer: &Localizer) -> String {
    match error {
        VersionError::SourceFormatUnsupported => localizer.text("source-designer-required"),
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
        VersionError::SchemeMismatch { expected, actual } => localizer.format(
            "version-scheme-mismatch",
            &[
                ("expected", LocalizationValue::Text(expected.as_str())),
                ("actual", LocalizationValue::Text(actual.as_str())),
            ],
        ),
        VersionError::ModuleContract { .. }
        | VersionError::ModuleMissing { .. }
        | VersionError::ModuleProtected { .. }
        | VersionError::ModuleAmbiguous { .. }
        | VersionError::Publication { .. }
        | VersionError::MigrationUnsupported => contract_error_message(error, localizer),
    }
}

/// Show a source coordinate and an actionable repair for new external-version failures.
fn contract_error_message(error: &VersionError, localizer: &Localizer) -> String {
    let reason = localizer.text(&format!(
        "version-reason-{}",
        error.code().replace('_', "-")
    ));
    let hint = localizer.text(&format!(
        "version-fix-{}",
        error.remediation().replace('_', "-")
    ));
    let path = error
        .path()
        .map_or_else(String::new, |path| path.to_string_lossy().into_owned());
    let range = error.range().map_or_else(String::new, |range| {
        format!("{}..{}", range.start, range.end)
    });
    localizer.format(
        "version-contract-error",
        &[
            ("path", LocalizationValue::Text(&path)),
            ("range", LocalizationValue::Text(&range)),
            ("reason", LocalizationValue::Text(&reason)),
            ("hint", LocalizationValue::Text(&hint)),
        ],
    )
}

/// Format a diagnostic containing one filesystem path.
fn path_message(localizer: &Localizer, key: &str, path: &str) -> String {
    localizer.format(key, &[("path", LocalizationValue::Text(path))])
}
