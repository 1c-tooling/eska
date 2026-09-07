//! Shared localized presentation of project, configuration and platform errors.

use std::{io, path::Path};

use crate::{
    cli::localization::{LocalizationValue, Localizer},
    config::{
        GlobalConfigError, InvalidMemberPathReason, InvalidSourceReason, ManifestConfigError,
        ProjectConfigError, WorkspaceConfigError,
    },
    project::discovery::{ContextDiscoveryError, DiscoveryError},
    project::{
        InvalidPathReason, ProjectPathError,
        build::{BuildSettingsError, InvalidArtifactsDirectoryReason, ToolError, ToolSource},
        selection::SelectionError,
    },
    vcs::workflow::PolicyError,
};

pub(super) fn present_selection_error(error: &SelectionError, localizer: &Localizer) -> String {
    match error {
        SelectionError::InvalidName(error) => localizer.format(
            "project-selector-name-invalid",
            &[("name", LocalizationValue::Text(error.value()))],
        ),
        SelectionError::DuplicateSelector { name } => localizer.format(
            "project-selector-duplicate",
            &[("name", LocalizationValue::Text(name.as_str()))],
        ),
        SelectionError::UnknownProject { name } => localizer.format(
            "project-selector-unknown",
            &[("name", LocalizationValue::Text(name.as_str()))],
        ),
        SelectionError::WorkspaceSelectorForStandalone => {
            localizer.text("project-selector-standalone")
        }
        SelectionError::ConflictingSelectors => localizer.text("project-selector-conflict"),
        SelectionError::ExplicitProjectRequired | SelectionError::SingleProjectRequired => {
            localizer.text("project-selector-single-required")
        }
    }
}

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

pub(super) fn present_context_error(
    error: &ContextDiscoveryError,
    localizer: &Localizer,
) -> String {
    match error {
        ContextDiscoveryError::Project(error) => present_project_error(error, localizer),
        ContextDiscoveryError::Manifest { path, source } => {
            manifest_message(localizer, path, source)
        }
        ContextDiscoveryError::MemberNotDirectory { path } => {
            path_message(localizer, "workspace-member-not-directory", path)
        }
        ContextDiscoveryError::MemberOutsideWorkspace { workspace, member } => localizer.format(
            "workspace-member-outside-root",
            &[
                (
                    "workspace",
                    LocalizationValue::Text(&workspace.to_string_lossy()),
                ),
                ("member", LocalizationValue::Text(&member.to_string_lossy())),
            ],
        ),
        ContextDiscoveryError::DuplicateMember { path } => {
            path_message(localizer, "workspace-member-duplicate", path)
        }
        ContextDiscoveryError::NestedMembers { first, second } => localizer.format(
            "workspace-members-nested",
            &[
                ("first", LocalizationValue::Text(&first.to_string_lossy())),
                ("second", LocalizationValue::Text(&second.to_string_lossy())),
            ],
        ),
        ContextDiscoveryError::MemberManifestNotProject { path } => {
            path_message(localizer, "workspace-member-project-required", path)
        }
        ContextDiscoveryError::MemberNameMissing { path } => {
            path_message(localizer, "workspace-member-name-required", path)
        }
        ContextDiscoveryError::DuplicateName { name } => localizer.format(
            "workspace-member-name-duplicate",
            &[("name", LocalizationValue::Text(name.as_str()))],
        ),
        ContextDiscoveryError::MemberWorkflowUnsupported { path } => {
            path_message(localizer, "workspace-member-workflow-unsupported", path)
        }
        ContextDiscoveryError::MemberArtifactsDirectoryUnsupported { path } => path_message(
            localizer,
            "workspace-member-artifacts-directory-unsupported",
            path,
        ),
        ContextDiscoveryError::UnlistedProject { project, workspace } => localizer.format(
            "workspace-project-unlisted",
            &[
                (
                    "project",
                    LocalizationValue::Text(&project.to_string_lossy()),
                ),
                (
                    "workspace",
                    LocalizationValue::Text(&workspace.to_string_lossy()),
                ),
            ],
        ),
        ContextDiscoveryError::ResolvedBuild(_) => localizer.text("workspace-build-invalid"),
    }
}

fn manifest_message(localizer: &Localizer, path: &Path, error: &ManifestConfigError) -> String {
    match error {
        ManifestConfigError::Io { path, source } => io_message(localizer, path, source),
        ManifestConfigError::Toml(_) => path_message(localizer, "project-config-invalid", path),
        ManifestConfigError::KindMissing => path_message(localizer, "manifest-kind-missing", path),
        ManifestConfigError::KindAmbiguous => {
            path_message(localizer, "manifest-kind-ambiguous", path)
        }
        ManifestConfigError::Project(error) => config_message(localizer, path, error),
        ManifestConfigError::Workspace(error) => workspace_config_message(localizer, path, error),
    }
}

fn workspace_config_message(
    localizer: &Localizer,
    manifest_path: &Path,
    error: &WorkspaceConfigError,
) -> String {
    match error {
        WorkspaceConfigError::Io { path, source } => io_message(localizer, path, source),
        WorkspaceConfigError::Toml(_) => {
            path_message(localizer, "workspace-config-invalid", manifest_path)
        }
        WorkspaceConfigError::InvalidBuild(_) => {
            path_message(localizer, "workspace-build-invalid", manifest_path)
        }
        WorkspaceConfigError::InvalidWorkflow(error) => {
            config_message(localizer, manifest_path, error)
        }
        WorkspaceConfigError::InvalidMemberPath { path, reason } => {
            let key = match reason {
                InvalidMemberPathReason::Empty => "workspace-member-path-empty",
                InvalidMemberPathReason::Absolute => "workspace-member-path-relative-required",
                InvalidMemberPathReason::ContainsParentTraversal => {
                    "workspace-member-path-parent-traversal"
                }
            };
            path_message(localizer, key, path)
        }
    }
}

/// Present machine-local config failures shared by config, build, platform and patch commands.
pub(super) fn present_global_config_error(
    error: &GlobalConfigError,
    localizer: &Localizer,
) -> String {
    match error {
        GlobalConfigError::LocationUnavailable => localizer.text("config-location-error"),
        GlobalConfigError::Io { path, source } | GlobalConfigError::Replace { path, source } => {
            localizer.format(
                "config-io-error",
                &[
                    ("path", LocalizationValue::Text(&path.to_string_lossy())),
                    ("reason", LocalizationValue::Text(&source.to_string())),
                ],
            )
        }
        GlobalConfigError::Invalid { path, source } => localizer.format(
            "config-invalid",
            &[
                ("path", LocalizationValue::Text(&path.to_string_lossy())),
                ("reason", LocalizationValue::Text(&source.to_string())),
            ],
        ),
        GlobalConfigError::DistroboxContainerMissing { path } => {
            path_message(localizer, "config-distrobox-container-missing", path)
        }
        GlobalConfigError::HostContainerUnexpected { path } => {
            path_message(localizer, "config-host-container-unexpected", path)
        }
        GlobalConfigError::Editor { source } => localizer.format(
            "config-editor-error",
            &[("reason", LocalizationValue::Text(&source.to_string()))],
        ),
        GlobalConfigError::EditorFailed => localizer.text("config-editor-failed"),
    }
}

/// Present platform discovery and exact-version failures shared by platform consumers.
pub(super) fn present_tool_error(error: &ToolError, localizer: &Localizer) -> String {
    match error {
        ToolError::InvalidArchitecture(value) => localizer.format(
            "build-arch-invalid",
            &[("value", LocalizationValue::Text(value))],
        ),
        ToolError::InvalidContainer(value) => localizer.format(
            "build-distrobox-invalid",
            &[("value", LocalizationValue::Text(value))],
        ),
        ToolError::InvalidExecutable(path) => localizer.format(
            "build-ibcmd-invalid",
            &[("path", LocalizationValue::Text(&path.to_string_lossy()))],
        ),
        ToolError::DistroboxContainerRequired => {
            localizer.text("build-distrobox-container-required")
        }
        ToolError::Scan { path, source } => localizer.format(
            "platform-scan-error",
            &[
                ("path", LocalizationValue::Text(&path.to_string_lossy())),
                ("reason", LocalizationValue::Text(&source.to_string())),
            ],
        ),
        ToolError::ScanCommandFailed { container, stderr } => localizer.format(
            "platform-scan-command-error",
            &[
                ("container", LocalizationValue::Text(container)),
                ("reason", LocalizationValue::Text(stderr)),
            ],
        ),
        ToolError::NotFound { expected, standard } => localizer.format(
            "build-ibcmd-missing",
            &[
                ("version", LocalizationValue::Text(expected.as_str())),
                ("path", LocalizationValue::Text(&standard.to_string_lossy())),
            ],
        ),
        ToolError::Run(error) => localizer.format(
            "build-ibcmd-run-error",
            &[("reason", LocalizationValue::Text(&error.to_string()))],
        ),
        ToolError::VersionCommandFailed { source, stderr } => localizer.format(
            "build-version-command-error",
            &[
                ("source", LocalizationValue::Text(&tool_source(source))),
                ("reason", LocalizationValue::Text(stderr)),
            ],
        ),
        ToolError::VersionUnreadable(source) => localizer.format(
            "build-version-unreadable",
            &[("source", LocalizationValue::Text(&tool_source(source)))],
        ),
        ToolError::VersionMismatch {
            expected,
            actual,
            source,
        } => localizer.format(
            "build-version-mismatch",
            &[
                ("expected", LocalizationValue::Text(expected.as_str())),
                ("actual", LocalizationValue::Text(actual.as_str())),
                ("source", LocalizationValue::Text(&tool_source(source))),
            ],
        ),
    }
}

fn tool_source(source: &ToolSource) -> String {
    match source {
        ToolSource::Explicit(path) | ToolSource::Path(path) | ToolSource::Standard(path) => {
            path.to_string_lossy().into_owned()
        }
        ToolSource::Distrobox { container, path } => {
            format!("{container}:{}", path.to_string_lossy())
        }
    }
}

fn config_message(localizer: &Localizer, path: &Path, error: &ProjectConfigError) -> String {
    match error {
        ProjectConfigError::InvalidName(error) => localizer.format(
            "project-name-invalid",
            &[
                ("path", LocalizationValue::Text(&path.to_string_lossy())),
                ("value", LocalizationValue::Text(error.value())),
            ],
        ),
        ProjectConfigError::InvalidBuild(error) => match error {
            BuildSettingsError::InvalidPlatformVersion { value } => localizer.format(
                "project-build-version-invalid",
                &[
                    ("path", LocalizationValue::Text(&path.to_string_lossy())),
                    ("value", LocalizationValue::Text(value)),
                ],
            ),
            BuildSettingsError::InvalidArtifactsDirectory { path, reason } => {
                let key = match reason {
                    InvalidArtifactsDirectoryReason::Empty => "project-build-path-empty",
                    InvalidArtifactsDirectoryReason::Absolute => {
                        "project-build-path-relative-required"
                    }
                    InvalidArtifactsDirectoryReason::ContainsParentTraversal => {
                        "project-build-path-parent-traversal"
                    }
                };
                path_message(localizer, key, path)
            }
        },
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
