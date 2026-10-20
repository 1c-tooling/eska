//! Project CLI envelopes keep localized sentences out of machine-readable output.

use std::{convert::Infallible, path::Path, process::ExitCode};

use eska_presentation::{
    diagnostics,
    localization::{LocalizationValue, Localizer},
    path_output,
};
use eska_project_core::{
    config::ProjectConfigError,
    project::{
        create::CreationError,
        designer_source::SourceError,
        discovery::{ContextDiscoveryError, DiscoveryError},
        init::InitError,
        onboarding::WorkspaceEnrollmentError,
    },
};
use serde_json::{Value, json};

use crate::{
    cli::OutputFormat,
    ide::{dto, errors},
};

pub struct Failure {
    code: &'static str,
    data: Value,
    message: String,
}

impl Failure {
    /// Keep the category and coordinates separate from its human translation.
    pub const fn new(code: &'static str, data: Value, message: String) -> Self {
        Self {
            code,
            data,
            message,
        }
    }

    /// Every failure emits a stable JSON category, with the translated text only on stderr.
    pub fn emit(self, format: OutputFormat) -> ExitCode {
        if matches!(format, OutputFormat::Json) {
            println!(
                "{}",
                json!({"schema_version":1,"kind":"project","status":"error","error":{"code":self.code,"data":self.data}})
            );
        }
        eprintln!("{}", self.message);
        ExitCode::FAILURE
    }
}

/// Use one path encoding for metadata protocol and standalone project results.
pub fn path(path: &Path) -> Value {
    dto::path(path)
}

/// Preserve detailed shared manifest diagnostics without introducing localized JSON fields.
pub fn discovery(error: &ContextDiscoveryError, localizer: &Localizer) -> Failure {
    Failure::new(
        "project_discovery",
        json!({}),
        diagnostics::present_context_error(error, localizer),
    )
}

/// Reuse the existing machine source failure categories for invalid XML and containment failures.
pub fn source(error: &SourceError, start: &Path, localizer: &Localizer) -> Failure {
    let location = match error {
        SourceError::Io { path, .. }
        | SourceError::OutsideSource { path }
        | SourceError::NotFile { path }
        | SourceError::DescriptorTooLarge { path }
        | SourceError::InvalidRoot { path, .. }
        | SourceError::InvalidMetadata { path, .. }
        | SourceError::Parse { path, .. } => path,
        _ => start,
    };
    let message = path_message(localizer, "project-source-error", location);
    Failure::new("project_source", errors::source(error), message)
}

/// Report rejected configuration before any project directory or manifest is written.
pub fn config(error: ProjectConfigError, manifest: &Path, localizer: &Localizer) -> Failure {
    Failure::new(
        "project_config",
        json!({"path":path(manifest)}),
        diagnostics::present_project_error(
            &DiscoveryError::Config {
                path: manifest.into(),
                source: error,
            },
            localizer,
        ),
    )
}

/// Common project writers preserve precise paths and rollback state for all entry points.
pub fn creation(error: CreationError<Infallible>, localizer: &Localizer) -> Failure {
    let (code, key, location) = match error {
        CreationError::Operation(never) => match never {},
        CreationError::InvalidDestination { path } => {
            ("invalid_destination", "new-destination-invalid", path)
        }
        CreationError::AlreadyExists { path } => {
            ("destination_exists", "new-destination-exists", path)
        }
        CreationError::Io { path, .. } => ("io", "project-write-error", path),
        CreationError::Rollback { path, .. } => {
            ("rollback_incomplete", "project-rollback-error", path)
        }
        CreationError::Template(_) => {
            return Failure::new(
                "manifest_serialize",
                json!({}),
                localizer.text("project-serialize-error"),
            );
        }
        CreationError::Validation(error) => {
            return Failure::new(
                "project_discovery",
                json!({}),
                diagnostics::present_project_error(&error, localizer),
            );
        }
        CreationError::Workspace(error) => return enrollment(&error, localizer),
        CreationError::WorkspaceValidation(error) => return discovery(&error, localizer),
        CreationError::WorkspaceMemberMissing { name } => {
            return Failure::new(
                "member_missing",
                json!({"member":name.as_str()}),
                localizer.format(
                    "project-member-missing",
                    &[("name", LocalizationValue::Text(name.as_str()))],
                ),
            );
        }
        CreationError::WorkspaceRollback { paths, .. } => return rollback(&paths, localizer),
    };
    Failure::new(
        code,
        json!({"path":path(&location)}),
        path_message(localizer, key, &location),
    )
}

/// Initialization never replaces an existing manifest or an owned source file.
pub fn initialization(error: InitError<Infallible>, localizer: &Localizer) -> Failure {
    let (code, key, location) = match error {
        InitError::Operation(never) => match never {},
        InitError::Io { path, .. } => ("io", "project-write-error", path),
        InitError::InvalidRoot { path } => ("invalid_root", "init-root-invalid", path),
        InitError::ExistingConfig { path } => ("manifest_exists", "init-config-exists", path),
        InitError::InvalidSource { path } => ("invalid_source", "init-source-invalid", path),
        InitError::MissingSource { path } => ("source_missing", "init-source-missing", path),
        InitError::AmbiguousSource { path } => ("source_ambiguous", "init-source-ambiguous", path),
        InitError::MultipleDescriptors { path } => {
            ("descriptor_ambiguous", "init-descriptors-multiple", path)
        }
        InitError::InvalidDescriptor { path } => {
            ("invalid_descriptor", "init-descriptor-invalid", path)
        }
        InitError::InvalidXml { path, .. } => ("xml_invalid", "init-xml-invalid", path),
        InitError::DescriptorTooLarge { path } => {
            ("descriptor_too_large", "init-descriptor-large", path)
        }
        InitError::ChangedSource { path } => ("source_changed", "init-source-changed", path),
        InitError::Config(error) => {
            return Failure::new(
                "project_config",
                json!({}),
                diagnostics::config_message(localizer, Path::new("eska.toml"), &error, false),
            );
        }
        InitError::Serialize(_) => {
            return Failure::new(
                "manifest_serialize",
                json!({}),
                localizer.text("project-serialize-error"),
            );
        }
        InitError::Validation(error) => {
            return Failure::new(
                "project_discovery",
                json!({}),
                diagnostics::present_project_error(&error, localizer),
            );
        }
        InitError::Workspace(error) => return enrollment(&error, localizer),
        InitError::WorkspaceValidation(error) => return discovery(&error, localizer),
        InitError::WorkspaceMemberMissing { name } => {
            return Failure::new(
                "member_missing",
                json!({"member":name.as_str()}),
                localizer.format(
                    "project-member-missing",
                    &[("name", LocalizationValue::Text(name.as_str()))],
                ),
            );
        }
        InitError::Rollback { paths, .. } => return rollback(&paths, localizer),
    };
    Failure::new(
        code,
        json!({"path":path(&location)}),
        path_message(localizer, key, &location),
    )
}

/// Show enrollment failures using the same root-manifest safety diagnostics as the full CLI.
fn enrollment(error: &WorkspaceEnrollmentError, localizer: &Localizer) -> Failure {
    Failure::new(
        "workspace_enrollment",
        json!({}),
        diagnostics::present_workspace_enrollment_error(error, localizer),
    )
}

/// A partial cleanup reports every remaining owned path without hiding the original operation category.
fn rollback(paths: &[std::path::PathBuf], localizer: &Localizer) -> Failure {
    Failure::new(
        "rollback_incomplete",
        json!({"paths":paths.iter().map(|value| path(value)).collect::<Vec<_>>()}),
        localizer.text("project-rollback-incomplete"),
    )
}

/// Keep plain path output escaped and consistent with existing project diagnostics.
pub fn path_message(localizer: &Localizer, key: &str, path: &Path) -> String {
    localizer.format(
        key,
        &[(
            "path",
            LocalizationValue::Text(&path_output::display_path(path)),
        )],
    )
}
