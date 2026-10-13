//! Explicit external-module migration preview and publication at the CLI boundary.

use std::process::ExitCode;

use clap::Args;
use serde::Serialize;
use serde_json::json;

use super::{
    LocalizationValue, Localizer, OutputFormat, ProjectSelection, ProjectVersion, VersionError,
    fail, relative_path, version, write_json_result,
};

#[derive(Debug, Args)]
pub(super) struct MigrationArgs {
    #[arg(long)]
    initial_version: Option<String>,
    #[arg(long)]
    write: bool,
    #[arg(short, long, action = clap::ArgAction::Help)]
    help: Option<bool>,
}

#[derive(Serialize)]
struct MigrationDocument<'a> {
    schema_version: u8,
    operation: &'static str,
    applied: bool,
    source: String,
    creates_module: bool,
    version: String,
    version_scheme: &'static str,
    edits: Vec<EditDocument<'a>>,
}

#[derive(Serialize)]
struct EditDocument<'a> {
    kind: &'static str,
    byte_start: usize,
    byte_end: usize,
    original: &'a str,
    replacement: &'a str,
}

/// One selected external project owns the plan; even a preview never implies a workspace write.
pub(super) fn run(
    selection: &ProjectSelection<'_>,
    arguments: &MigrationArgs,
    format: OutputFormat,
    localizer: &Localizer,
) -> ExitCode {
    let Some(selected) = selection.projects().first() else {
        eprintln!("{}", localizer.text("version-selection-empty"));
        return ExitCode::FAILURE;
    };
    let initial = match arguments
        .initial_version
        .as_deref()
        .map(ProjectVersion::parse)
        .transpose()
    {
        Ok(initial) => initial,
        Err(error) => return failure(&error, format, localizer),
    };
    let plan = match version::plan_migration(selected.project(), initial.as_ref()) {
        Ok(plan) => plan,
        Err(error) => return failure(&error, format, localizer),
    };
    if arguments.write
        && let Err(error) = plan.publish()
    {
        return failure(&error, format, localizer);
    }
    let document = MigrationDocument {
        schema_version: 1,
        operation: "version-migration",
        applied: arguments.write && !plan.edits().is_empty(),
        source: relative_path(selected.project().root(), plan.path()),
        creates_module: plan.creates_module(),
        version: plan.version().to_string(),
        version_scheme: plan.version().scheme().as_str(),
        edits: plan
            .edits()
            .iter()
            .map(|edit| EditDocument {
                kind: edit.kind.as_str(),
                byte_start: edit.range.start,
                byte_end: edit.range.end,
                original: &plan.original()[edit.range.clone()],
                replacement: &edit.replacement,
            })
            .collect(),
    };
    match format {
        OutputFormat::Json => write_json_result(&document, localizer),
        OutputFormat::Human => {
            let key = if plan.edits().is_empty() {
                "version-migrate-empty"
            } else if arguments.write {
                "version-migrate-written"
            } else {
                "version-migrate-preview"
            };
            println!(
                "{}",
                localizer.format(
                    key,
                    &[
                        ("path", LocalizationValue::Text(&document.source)),
                        ("version", LocalizationValue::Text(&document.version)),
                    ]
                )
            );
            for edit in &document.edits {
                let label =
                    localizer.text(&format!("version-edit-{}", edit.kind.replace('_', "-")));
                println!(
                    "{}",
                    localizer.format(
                        "version-migrate-edit",
                        &[
                            ("kind", LocalizationValue::Text(&label)),
                            (
                                "range",
                                LocalizationValue::Text(&format!(
                                    "{}..{}",
                                    edit.byte_start, edit.byte_end
                                ))
                            ),
                        ]
                    )
                );
                println!("{}", edit.replacement);
            }
            ExitCode::SUCCESS
        }
    }
}

/// Preserve locale-independent diagnostics for the new migration JSON protocol.
fn failure(error: &VersionError, format: OutputFormat, localizer: &Localizer) -> ExitCode {
    if matches!(format, OutputFormat::Json) {
        let document = json!({"schema_version":1,"operation":"version-migration","status":"error","error":{
            "code":error.code(),"path":error.path().map(|path| path.to_string_lossy()),
            "range":error.range().map(|range| json!({"byte_start":range.start,"byte_end":range.end})),
            "remediation":error.remediation(),
        }});
        if write_json_result(&document, localizer) == ExitCode::FAILURE {
            return ExitCode::FAILURE;
        }
    }
    fail(error, localizer)
}

/// Help belongs to the host locale, while option names remain stable.
pub(super) fn localize(command: clap::Command, localizer: &Localizer) -> clap::Command {
    command
        .about(localizer.text("version-migrate-about"))
        .override_usage(localizer.text("version-migrate-usage"))
        .mut_arg("initial_version", |argument| {
            argument
                .help(localizer.text("version-migrate-initial-help"))
                .value_name(localizer.text("version-migrate-initial-value"))
        })
        .mut_arg("write", |argument| {
            argument.help(localizer.text("version-migrate-write-help"))
        })
        .mut_arg("help", |argument| argument.help(localizer.text("cli-help")))
}
