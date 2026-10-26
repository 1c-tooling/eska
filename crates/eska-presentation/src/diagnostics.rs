//! Shared presentation of project manifests, selectors and filesystem failures.

use std::{io, path::Path};

pub mod application;

use crate::{
    localization::{LocalizationValue, Localizer},
    path_output,
};
use eska_project_core::{
    config::{
        InvalidMemberPathReason, InvalidSourceReason, ManifestConfigError, ProjectConfigError,
        WorkspaceConfigError,
    },
    project::{
        InvalidPathReason, ProjectPathError,
        build::{BuildSettingsError, InvalidArtifactsDirectoryReason},
        discovery::{ContextDiscoveryError, DiscoveryError},
        onboarding::WorkspaceEnrollmentError,
        selection::SelectionError,
    },
    workflow::PolicyError,
};

/// Keep existing consumers plain while new/import explicitly opt into terminal path links.
#[must_use]
pub fn present_workspace_enrollment_error(
    error: &WorkspaceEnrollmentError,
    localizer: &Localizer,
) -> String {
    present_workspace_enrollment_error_with_links(error, localizer, false)
}

/// Keep existing consumers plain while new/import explicitly opt into terminal path links.
#[must_use]
pub fn present_project_error(error: &DiscoveryError, localizer: &Localizer) -> String {
    present_project_error_with_links(error, localizer, false)
}

/// Keep existing consumers plain while new/import explicitly opt into terminal path links.
#[must_use]
pub fn present_context_error(error: &ContextDiscoveryError, localizer: &Localizer) -> String {
    present_context_error_with_links(error, localizer, false)
}

/// Present enrollment failures with an explicit output-stream link policy.
#[must_use]
pub fn present_workspace_enrollment_error_with_links(
    error: &WorkspaceEnrollmentError,
    localizer: &Localizer,
    hyperlinks: bool,
) -> String {
    match error {
        WorkspaceEnrollmentError::ApplicationRequired { available } => localizer.format(
            "workspace-onboarding-application-required",
            &[("names", LocalizationValue::Text(&available.join(", ")))],
        ),
        WorkspaceEnrollmentError::InvalidApplication { name } => localizer.format(
            "workspace-onboarding-application-invalid",
            &[("name", LocalizationValue::Text(name))],
        ),
        WorkspaceEnrollmentError::ConfigurationAlreadySet { application } => localizer.format(
            "workspace-onboarding-configuration-set",
            &[("name", LocalizationValue::Text(application))],
        ),
        WorkspaceEnrollmentError::Io { path, source } => {
            io_message(localizer, path, source, hyperlinks)
        }
        WorkspaceEnrollmentError::Config(_)
        | WorkspaceEnrollmentError::TomlEdit(_)
        | WorkspaceEnrollmentError::InvalidComposition => {
            localizer.text("workspace-onboarding-config-invalid")
        }
        WorkspaceEnrollmentError::NonUtf8MemberPath { path } => path_message(
            localizer,
            "workspace-onboarding-path-utf8",
            path,
            hyperlinks,
        ),
        WorkspaceEnrollmentError::OutsideWorkspace { workspace, member } => localizer.format(
            "workspace-member-outside-root",
            &[
                (
                    "workspace",
                    LocalizationValue::Text(&path_output::render(workspace, hyperlinks)),
                ),
                (
                    "member",
                    LocalizationValue::Text(&path_output::render(member, hyperlinks)),
                ),
            ],
        ),
        WorkspaceEnrollmentError::DuplicateMember { path } => {
            path_message(localizer, "workspace-member-duplicate", path, hyperlinks)
        }
        WorkspaceEnrollmentError::NestedMembers { first, second } => localizer.format(
            "workspace-members-nested",
            &[
                (
                    "first",
                    LocalizationValue::Text(&path_output::render(first, hyperlinks)),
                ),
                (
                    "second",
                    LocalizationValue::Text(&path_output::render(second, hyperlinks)),
                ),
            ],
        ),
        WorkspaceEnrollmentError::DuplicateName { name } => localizer.format(
            "workspace-member-name-duplicate",
            &[("name", LocalizationValue::Text(name.as_str()))],
        ),
        WorkspaceEnrollmentError::ManifestChanged { path } => path_message(
            localizer,
            "workspace-onboarding-manifest-changed",
            path,
            hyperlinks,
        ),
    }
}

/// Render invalid selectors without changing stable member identifiers.
#[must_use]
pub fn present_selection_error(error: &SelectionError, localizer: &Localizer) -> String {
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

/// Present project failures with stream-specific filesystem links.
#[must_use]
pub fn present_project_error_with_links(
    error: &DiscoveryError,
    localizer: &Localizer,
    hyperlinks: bool,
) -> String {
    match error {
        DiscoveryError::NotFound { start } => {
            path_message(localizer, "project-not-found", start, hyperlinks)
        }
        DiscoveryError::Io { path, source } => io_message(localizer, path, source, hyperlinks),
        DiscoveryError::StartNotDirectory { path } => {
            path_message(localizer, "project-start-not-directory", path, hyperlinks)
        }
        DiscoveryError::ConfigNotFile { path } => {
            path_message(localizer, "project-config-not-file", path, hyperlinks)
        }
        DiscoveryError::SourceNotDirectory { path } => {
            path_message(localizer, "project-source-not-directory", path, hyperlinks)
        }
        DiscoveryError::Config { path, source } => {
            config_message(localizer, path, source, hyperlinks)
        }
    }
}

/// Retain links through nested workspace and project discovery failures.
#[must_use]
pub fn present_context_error_with_links(
    error: &ContextDiscoveryError,
    localizer: &Localizer,
    hyperlinks: bool,
) -> String {
    match error {
        ContextDiscoveryError::Project(error) => {
            present_project_error_with_links(error, localizer, hyperlinks)
        }
        ContextDiscoveryError::Manifest { path, source } => {
            manifest_message(localizer, path, source, hyperlinks)
        }
        ContextDiscoveryError::MemberNotDirectory { path } => path_message(
            localizer,
            "workspace-member-not-directory",
            path,
            hyperlinks,
        ),
        ContextDiscoveryError::MemberOutsideWorkspace { workspace, member } => localizer.format(
            "workspace-member-outside-root",
            &[
                (
                    "workspace",
                    LocalizationValue::Text(&path_output::render(workspace, hyperlinks)),
                ),
                (
                    "member",
                    LocalizationValue::Text(&path_output::render(member, hyperlinks)),
                ),
            ],
        ),
        ContextDiscoveryError::DuplicateMember { path } => {
            path_message(localizer, "workspace-member-duplicate", path, hyperlinks)
        }
        ContextDiscoveryError::NestedMembers { first, second } => localizer.format(
            "workspace-members-nested",
            &[
                (
                    "first",
                    LocalizationValue::Text(&path_output::render(first, hyperlinks)),
                ),
                (
                    "second",
                    LocalizationValue::Text(&path_output::render(second, hyperlinks)),
                ),
            ],
        ),
        ContextDiscoveryError::MemberManifestNotProject { path } => path_message(
            localizer,
            "workspace-member-project-required",
            path,
            hyperlinks,
        ),
        ContextDiscoveryError::MemberNameMissing { path } => path_message(
            localizer,
            "workspace-member-name-required",
            path,
            hyperlinks,
        ),
        ContextDiscoveryError::DuplicateName { name } => localizer.format(
            "workspace-member-name-duplicate",
            &[("name", LocalizationValue::Text(name.as_str()))],
        ),
        ContextDiscoveryError::MemberWorkflowUnsupported { path } => path_message(
            localizer,
            "workspace-member-workflow-unsupported",
            path,
            hyperlinks,
        ),
        ContextDiscoveryError::MemberArtifactsDirectoryUnsupported { path } => path_message(
            localizer,
            "workspace-member-artifacts-directory-unsupported",
            path,
            hyperlinks,
        ),
        ContextDiscoveryError::UnlistedProject { project, workspace } => localizer.format(
            "workspace-project-unlisted",
            &[
                (
                    "project",
                    LocalizationValue::Text(&path_output::render(project, hyperlinks)),
                ),
                (
                    "workspace",
                    LocalizationValue::Text(&path_output::render(workspace, hyperlinks)),
                ),
            ],
        ),
        ContextDiscoveryError::ResolvedBuild(_) => localizer.text("workspace-build-invalid"),
    }
}

/// Render manifest diagnostics using the caller's output-stream policy.
fn manifest_message(
    localizer: &Localizer,
    path: &Path,
    error: &ManifestConfigError,
    hyperlinks: bool,
) -> String {
    match error {
        ManifestConfigError::Io { path, source } => io_message(localizer, path, source, hyperlinks),
        ManifestConfigError::Toml(_) => {
            path_message(localizer, "project-config-invalid", path, hyperlinks)
        }
        ManifestConfigError::KindMissing => {
            path_message(localizer, "manifest-kind-missing", path, hyperlinks)
        }
        ManifestConfigError::KindAmbiguous => {
            path_message(localizer, "manifest-kind-ambiguous", path, hyperlinks)
        }
        ManifestConfigError::Project(error) => config_message(localizer, path, error, hyperlinks),
        ManifestConfigError::Workspace(error) => {
            workspace_config_message(localizer, path, error, hyperlinks)
        }
    }
}

/// Carry link policy through workspace configuration diagnostics.
fn workspace_config_message(
    localizer: &Localizer,
    manifest_path: &Path,
    error: &WorkspaceConfigError,
    hyperlinks: bool,
) -> String {
    match error {
        WorkspaceConfigError::InvalidAnalysis(error) => {
            application::present(localizer, manifest_path, error, hyperlinks)
        }
        WorkspaceConfigError::Io { path, source } => {
            io_message(localizer, path, source, hyperlinks)
        }
        WorkspaceConfigError::Toml(_) => path_message(
            localizer,
            "workspace-config-invalid",
            manifest_path,
            hyperlinks,
        ),
        WorkspaceConfigError::InvalidBuild(_) => path_message(
            localizer,
            "workspace-build-invalid",
            manifest_path,
            hyperlinks,
        ),
        WorkspaceConfigError::InvalidWorkflow(error) => {
            config_message(localizer, manifest_path, error, hyperlinks)
        }
        WorkspaceConfigError::InvalidMemberPath { path, reason } => {
            let key = match reason {
                InvalidMemberPathReason::Empty => "workspace-member-path-empty",
                InvalidMemberPathReason::Absolute => "workspace-member-path-relative-required",
                InvalidMemberPathReason::ContainsParentTraversal => {
                    "workspace-member-path-parent-traversal"
                }
            };
            configured_path_message(localizer, key, path, manifest_path, hyperlinks)
        }
    }
}

/// Carry link policy through nested project configuration diagnostics.
pub fn config_message(
    localizer: &Localizer,
    path: &Path,
    error: &ProjectConfigError,
    hyperlinks: bool,
) -> String {
    let manifest_path = path;
    match error {
        ProjectConfigError::InvalidAnalysis(error) => {
            application::present(localizer, path, error, hyperlinks)
        }
        ProjectConfigError::InvalidName(error) => localizer.format(
            "project-name-invalid",
            &[
                (
                    "path",
                    LocalizationValue::Text(&path_output::render(path, hyperlinks)),
                ),
                ("value", LocalizationValue::Text(error.value())),
            ],
        ),
        ProjectConfigError::InvalidBuild(error) => match error {
            BuildSettingsError::InvalidPlatformVersion { value } => localizer.format(
                "project-build-version-invalid",
                &[
                    (
                        "path",
                        LocalizationValue::Text(&path_output::render(path, hyperlinks)),
                    ),
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
                configured_path_message(localizer, key, path, manifest_path, hyperlinks)
            }
        },
        ProjectConfigError::InvalidWorkflow(error) => {
            workflow_message(localizer, path, error, hyperlinks)
        }
        ProjectConfigError::UnknownWorkflow { value } => value_message(
            localizer,
            "project-workflow-unknown",
            path,
            value,
            hyperlinks,
        ),
        ProjectConfigError::Io { path, source } => io_message(localizer, path, source, hyperlinks),
        // Parser/OS diagnostics are not translated. Do not leak their English
        // Display output into localized project diagnostics.
        ProjectConfigError::Toml(_) => {
            path_message(localizer, "project-config-invalid", path, hyperlinks)
        }
        ProjectConfigError::UnknownProjectType { value } => {
            value_message(localizer, "project-type-unknown", path, value, hyperlinks)
        }
        ProjectConfigError::UnknownSourceFormat { value } => {
            value_message(localizer, "project-format-unknown", path, value, hyperlinks)
        }
        ProjectConfigError::InvalidSource { path, reason } => {
            let key = match reason {
                InvalidSourceReason::Empty => "project-path-empty",
                InvalidSourceReason::Absolute => "project-path-relative-required",
                InvalidSourceReason::ContainsParentTraversal => "project-path-parent-traversal",
            };
            configured_path_message(localizer, key, path, manifest_path, hyperlinks)
        }
        ProjectConfigError::ProjectPath(error) => match error {
            ProjectPathError::InvalidPath { path, reason, .. } => {
                let key = match reason {
                    InvalidPathReason::NotAbsolute => "project-path-absolute-required",
                    InvalidPathReason::ContainsParentTraversal => "project-path-parent-traversal",
                };
                configured_path_message(localizer, key, path, manifest_path, hyperlinks)
            }
            ProjectPathError::SourceOutsideRoot { root, source } => localizer.format(
                "project-source-outside-root",
                &[
                    (
                        "root",
                        LocalizationValue::Text(&path_output::render(root, hyperlinks)),
                    ),
                    (
                        "source",
                        LocalizationValue::Text(&path_output::render(source, hyperlinks)),
                    ),
                ],
            ),
        },
    }
}

/// Render a workflow failure against its configuration path.
fn workflow_message(
    localizer: &Localizer,
    path: &Path,
    error: &PolicyError,
    hyperlinks: bool,
) -> String {
    match error {
        PolicyError::InvalidValue { field, value } => localizer.format(
            "project-workflow-value-invalid",
            &[
                (
                    "path",
                    LocalizationValue::Text(&path_output::render(path, hyperlinks)),
                ),
                ("field", LocalizationValue::Text(field.as_str())),
                ("value", LocalizationValue::Text(value)),
            ],
        ),
        PolicyError::MissingField { field } => localizer.format(
            "project-workflow-field-missing",
            &[
                (
                    "path",
                    LocalizationValue::Text(&path_output::render(path, hyperlinks)),
                ),
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
            hyperlinks,
        ),
    }
}

/// Localize OS failure kinds without losing the affected path.
#[must_use]
pub fn io_message(
    localizer: &Localizer,
    path: &Path,
    error: &io::Error,
    hyperlinks: bool,
) -> String {
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
            (
                "path",
                LocalizationValue::Text(&path_output::render(path, hyperlinks)),
            ),
            ("reason", LocalizationValue::Text(&reason)),
        ],
    )
}

/// Link the complete path argument independently of surrounding punctuation.
#[must_use]
pub fn path_message(localizer: &Localizer, key: &str, path: &Path, hyperlinks: bool) -> String {
    localizer.format(
        key,
        &[(
            "path",
            LocalizationValue::Text(&path_output::render(path, hyperlinks)),
        )],
    )
}

/// Resolve a config value against its manifest directory, retaining the original label.
fn configured_path_message(
    localizer: &Localizer,
    key: &str,
    path: &Path,
    manifest: &Path,
    hyperlinks: bool,
) -> String {
    let root = manifest.parent().unwrap_or_else(|| Path::new("."));
    localizer.format(
        key,
        &[(
            "path",
            LocalizationValue::Text(&path_output::render_link(
                path,
                &root.join(path),
                hyperlinks,
            )),
        )],
    )
}

/// Keep invalid configuration values separate from the linked manifest path.
fn value_message(
    localizer: &Localizer,
    key: &str,
    path: &Path,
    value: &str,
    hyperlinks: bool,
) -> String {
    localizer.format(
        key,
        &[
            (
                "path",
                LocalizationValue::Text(&path_output::render(path, hyperlinks)),
            ),
            ("value", LocalizationValue::Text(value)),
        ],
    )
}
