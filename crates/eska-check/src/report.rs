//! Versioned CI reports contain effective upstream diagnostics and explicit completeness.

use std::{
    io::{self, Write},
    path::Path,
};

use eska_analyzer::{
    check::{Failure, Report, Severity},
    process::SUPPORTED_VERSION,
};
use eska_presentation::{
    encoding,
    localization::{LocalizationValue, Localizer},
    path_output::display_path,
};
use eska_project_core::project::{metadata_parser::LoadError, validation::ValidationError};
use serde_json::{Value, json};

/// Reuse the existing reversible native path envelope across tools.
pub fn path(value: &Path) -> Value {
    let (value, encoding) = encoding::json_path(value.as_os_str());
    json!({"value":value,"encoding":encoding})
}

/// Timings are deliberately excluded so reports of identical inputs remain reproducible.
pub fn document(report: &Report) -> Value {
    json!({"schema_version":1,"kind":"check","status":if report.counts.exceeds(&report.policy) { "threshold_exceeded" } else { "passed" },"complete":true,"scope":"full", "context":{"root":path(&report.root),"application":report.application,"limited_context":report.limited_context},"analyzer":{"name":"bsl-analyzer-app","version":SUPPORTED_VERSION},"validation":{"kind":"declared_descriptors","objects":report.declared_objects},"counts":report.counts,"policy":report.policy,"input_fingerprint":report.input_fingerprint,"files":report.files.iter().map(|file|json!({"path":path(&file.path),"diagnostics":file.diagnostics})).collect::<Vec<_>>()})
}

/// No diagnostic array from an incomplete run can be mistaken for a complete clean report.
pub fn failure(error: &Failure) -> Value {
    let data = match error {
        Failure::Io {
            path: value,
            source,
        } => json!({"path":path(value),"io_kind":source.kind().to_string()}),
        Failure::UnsupportedPath { path: value } => json!({"path":path(value)}),
        Failure::PartialPolicy { field } => json!({"field":field}),
        Failure::Structure { root, source } => {
            json!({"root":path(root),"failure":structure(source)})
        }
        Failure::Stream { reason } => json!({"reason":reason}),
        Failure::ProcessExit { exit_code, .. } => json!({"exit_code":exit_code}),
        Failure::Configuration(error) => configuration(error),
        Failure::EmptyInput | Failure::InputsChanged => json!({}),
    };
    json!({"schema_version":1,"kind":"check","status":if error.code()=="cancelled" { "cancelled" } else { "incomplete" },"complete":false,"error":{"code":error.code(),"data":data}})
}

/// Preserve the actionable coordinates of shared preparation errors without linking the project CLI.
fn configuration(error: &eska_analyzer::Error) -> Value {
    use eska_analyzer::Error;
    match error {
        Error::Io {
            path: value,
            source,
        } => json!({"path":path(value),"io_kind":source.kind().to_string()}),
        Error::NonPortablePath { path: value }
        | Error::InputTooLarge { path: value }
        | Error::ManagedConflict { path: value }
        | Error::MigrationRequired { path: value }
        | Error::Busy { path: value }
        | Error::InvalidToml { path: value, .. } => json!({"path":path(value)}),
        Error::InvalidPolicy { field } => json!({"field":field}),
        Error::ContextConflict { current, requested } => {
            json!({"current":current,"requested":requested})
        }
        Error::ProcessStart { binary, source } => {
            json!({"binary":path(binary),"io_kind":source.kind().to_string()})
        }
        Error::AnalyzerVersion { actual } => json!({"actual":actual,"supported":SUPPORTED_VERSION}),
        Error::CheckConfig { exit_code, .. } => json!({"exit_code":exit_code}),
        Error::Context(error) => context(error),
        _ => json!({}),
    }
}

/// Keep application selection errors actionable without exposing Rust debug representations.
fn context(error: &eska_project_core::application::ContextError) -> Value {
    use eska_project_core::application::ContextError;
    match error {
        ContextError::UnknownApplication {
            requested,
            available,
        } => json!({"reason":"unknown_application","requested":requested,"available":available}),
        ContextError::AmbiguousApplication { available } => {
            json!({"reason":"ambiguous_application","available":available})
        }
        ContextError::ApplicationDeclarationsMissing { members } => {
            json!({"reason":"application_declarations_missing","members":members})
        }
        ContextError::EmptyWorkspace => json!({"reason":"empty_workspace"}),
        ContextError::Discovery(_) => json!({"reason":"project_discovery"}),
        ContextError::Source(_) => json!({"reason":"source"}),
        ContextError::Settings(_) => json!({"reason":"application_settings"}),
    }
}

/// Structural failures retain object identity or descriptor location when it is available.
fn structure(error: &ValidationError) -> Value {
    match error {
        ValidationError::Unsupported(id) => json!({"reason":"unsupported","object_id":id.as_str()}),
        ValidationError::Load(LoadError::Parse { path: value, .. }) => {
            json!({"reason":"parse","path":path(value)})
        }
        ValidationError::Load(LoadError::MissingDescriptor(id)) => {
            json!({"reason":"missing_descriptor","object_id":id.as_str()})
        }
        ValidationError::Load(LoadError::ObjectNotFound(id)) => {
            json!({"reason":"object_not_found","object_id":id.as_str()})
        }
        ValidationError::Load(_) | ValidationError::Source(_) => json!({"reason":"source"}),
        ValidationError::TooManyDescriptors => json!({"reason":"descriptor_limit"}),
        ValidationError::Cancelled => json!({"reason":"cancelled"}),
    }
}

/// Human summaries use localized labels; each finding retains the original upstream message.
pub fn human(writer: &mut impl Write, report: &Report, localizer: &Localizer) -> io::Result<()> {
    let status = localizer.text(if report.counts.exceeds(&report.policy) {
        "check-threshold-exceeded"
    } else {
        "check-passed"
    });
    writeln!(writer, "{status}")?;
    writeln!(
        writer,
        "{}",
        localizer.format(
            "check-summary",
            &[
                (
                    "files",
                    LocalizationValue::Text(&report.files.len().to_string())
                ),
                (
                    "errors",
                    LocalizationValue::Text(&report.counts.errors.to_string())
                ),
                (
                    "warnings",
                    LocalizationValue::Text(&report.counts.warnings.to_string())
                ),
                (
                    "information",
                    LocalizationValue::Text(&report.counts.information.to_string())
                ),
                (
                    "hints",
                    LocalizationValue::Text(&report.counts.hints.to_string())
                )
            ]
        )
    )?;
    for file in &report.files {
        for diagnostic in &file.diagnostics {
            let severity = localizer.text(match diagnostic.severity {
                Severity::Error => "check-severity-error",
                Severity::Warning => "check-severity-warning",
                Severity::Information => "check-severity-information",
                Severity::Hint => "check-severity-hint",
            });
            writeln!(
                writer,
                "{}:{}:{}: {} {}: {}",
                display_path(&file.path),
                diagnostic.start_line.saturating_add(1),
                diagnostic.start_column.saturating_add(1),
                severity,
                diagnostic.code,
                diagnostic.message
            )?;
        }
    }
    Ok(())
}

/// Localized explanations and upstream process logs stay on stderr, outside stable JSON fields.
pub fn human_error(error: &Failure, localizer: &Localizer) {
    if let Failure::Configuration(error) = error {
        if matches!(error.code(), "context" | "cancelled" | "timeout") {
            eprintln!(
                "{}",
                localizer.text(&format!("check-error-{}", error.code()))
            );
        } else {
            eprintln!(
                "{}",
                localizer.text(&format!("analyzer-error-{}", error.code()))
            );
        }
        if let eska_analyzer::Error::CheckConfig { stdout, stderr, .. } = error {
            eprintln!("{stdout}{stderr}");
        }
    } else {
        eprintln!(
            "{}",
            localizer.text(&format!("check-error-{}", error.code()))
        );
        if let Failure::ProcessExit { stderr, .. } = error {
            eprintln!("{stderr}");
        }
    }
}
