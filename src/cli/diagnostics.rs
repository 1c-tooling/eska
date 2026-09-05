//! Shared localized presentation of project discovery and configuration errors.

use std::{io, path::Path};

use crate::{
    cli::localization::{LocalizationValue, Localizer},
    config::{InvalidSourceReason, ProjectConfigError},
    project::discovery::DiscoveryError,
    project::{InvalidPathReason, ProjectPathError},
    vcs::workflow::PolicyError,
};

pub(super) fn present_project_error(error: &DiscoveryError, localizer: &Localizer) -> String {
    match error {
        DiscoveryError::NotFound { start } => path_message(localizer, "project-not-found", start),
        DiscoveryError::Io { path, source } => io_message(localizer, path, source),
        DiscoveryError::StartNotDirectory { path } => {
            path_message(localizer, "project-start-not-directory", path)
        }
        DiscoveryError::ConfigNotFile { path } => {
            path_message(localizer, "project-config-not-file", path)
        }
        DiscoveryError::SourceNotDirectory { path } => {
            path_message(localizer, "project-source-not-directory", path)
        }
        DiscoveryError::Config { path, source } => config_message(localizer, path, source),
    }
}

fn config_message(localizer: &Localizer, path: &Path, error: &ProjectConfigError) -> String {
    match error {
        ProjectConfigError::InvalidWorkflow(error) => workflow_message(localizer, path, error),
        ProjectConfigError::UnknownWorkflow { value } => {
            value_message(localizer, "project-workflow-unknown", path, value)
        }
        ProjectConfigError::Io { path, source } => io_message(localizer, path, source),
        // Parser/OS diagnostics are not translated. Do not leak their English
        // Display output into localized project diagnostics.
        ProjectConfigError::Toml(_) => path_message(localizer, "project-config-invalid", path),
        ProjectConfigError::UnknownProjectType { value } => {
            value_message(localizer, "project-type-unknown", path, value)
        }
        ProjectConfigError::UnknownSourceFormat { value } => {
            value_message(localizer, "project-format-unknown", path, value)
        }
        ProjectConfigError::InvalidSource { path, reason } => {
            let key = match reason {
                InvalidSourceReason::Empty => "project-path-empty",
                InvalidSourceReason::Absolute => "project-path-relative-required",
                InvalidSourceReason::ContainsParentTraversal => "project-path-parent-traversal",
            };
            path_message(localizer, key, path)
        }
        ProjectConfigError::ProjectPath(error) => match error {
            ProjectPathError::InvalidPath { path, reason, .. } => {
                let key = match reason {
                    InvalidPathReason::NotAbsolute => "project-path-absolute-required",
                    InvalidPathReason::ContainsParentTraversal => "project-path-parent-traversal",
                };
                path_message(localizer, key, path)
            }
            ProjectPathError::SourceOutsideRoot { root, source } => localizer.format(
                "project-source-outside-root",
                &[
                    ("root", LocalizationValue::Text(&root.to_string_lossy())),
                    ("source", LocalizationValue::Text(&source.to_string_lossy())),
                ],
            ),
        },
    }
}

fn workflow_message(localizer: &Localizer, path: &Path, error: &PolicyError) -> String {
    match error {
        PolicyError::InvalidValue { field, value } => localizer.format(
            "project-workflow-value-invalid",
            &[
                ("path", LocalizationValue::Text(&path.to_string_lossy())),
                ("field", LocalizationValue::Text(field.as_str())),
                ("value", LocalizationValue::Text(value)),
            ],
        ),
        PolicyError::MissingField { field } => localizer.format(
            "project-workflow-field-missing",
            &[
                ("path", LocalizationValue::Text(&path.to_string_lossy())),
                ("field", LocalizationValue::Text(field.as_str())),
            ],
        ),
        error => path_message(
            localizer,
            match error {
                PolicyError::ExtendsRequiresCustom => "project-workflow-custom-required",
                PolicyError::CustomBase => "project-workflow-custom-base",
                PolicyError::PublishRequired => "project-workflow-publish-required",
                PolicyError::IntegrationRequiredForDeletion => {
                    "project-workflow-integration-required"
                }
                _ => "project-workflow-invalid",
            },
            path,
        ),
    }
}

fn io_message(localizer: &Localizer, path: &Path, error: &io::Error) -> String {
    let reason = localizer.text(match error.kind() {
        io::ErrorKind::NotFound => "project-io-not-found",
        io::ErrorKind::PermissionDenied => "project-io-permission-denied",
        io::ErrorKind::InvalidData => "project-io-invalid-data",
        io::ErrorKind::NotADirectory => "project-io-not-directory",
        _ => "project-io-other",
    });
    localizer.format(
        "project-io-error",
        &[
            ("path", LocalizationValue::Text(&path.to_string_lossy())),
            ("reason", LocalizationValue::Text(&reason)),
        ],
    )
}

fn path_message(localizer: &Localizer, key: &str, path: &Path) -> String {
    localizer.format(
        key,
        &[("path", LocalizationValue::Text(&path.to_string_lossy()))],
    )
}

fn value_message(localizer: &Localizer, key: &str, path: &Path, value: &str) -> String {
    localizer.format(
        key,
        &[
            ("path", LocalizationValue::Text(&path.to_string_lossy())),
            ("value", LocalizationValue::Text(value)),
        ],
    )
}

#[cfg(test)]
mod tests {
    use std::{io, path::PathBuf};

    use super::present_project_error;
    use crate::{
        cli::localization::{Locale, Localizer},
        project::discovery::DiscoveryError,
    };

    #[test]
    fn localizes_io_reasons_without_relying_on_os_messages_or_permissions() {
        for (kind, ru, en) in [
            (
                io::ErrorKind::PermissionDenied,
                "доступ запрещён",
                "permission denied",
            ),
            (
                io::ErrorKind::InvalidData,
                "не является корректным текстом UTF-8",
                "not valid UTF-8 text",
            ),
            (
                io::ErrorKind::NotADirectory,
                "компонент пути не является каталогом",
                "a path component is not a directory",
            ),
            (
                io::ErrorKind::Other,
                "ошибка файловой системы",
                "filesystem error",
            ),
        ] {
            let error = DiscoveryError::Io {
                path: PathBuf::from("fixture/eska.toml"),
                source: io::Error::new(kind, "unlocalized operating system message"),
            };
            for (locale, expected) in [(Locale::RuRu, ru), (Locale::EnUs, en)] {
                let localizer = Localizer::try_new(locale).expect("valid locale");
                let message = present_project_error(&error, &localizer);
                assert!(message.contains(expected), "{message}");
                assert!(message.contains("fixture/eska.toml"), "{message}");
                assert!(!message.contains("unlocalized operating system message"));
            }
        }
    }
}
