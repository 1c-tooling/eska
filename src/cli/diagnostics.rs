//! Shared localized presentation of project, configuration and platform errors.

use std::{io, path::Path};

use crate::{
    cli::{
        localization::{LocalizationValue, Localizer},
        path_output,
    },
    config::{
        GlobalConfigError, InvalidMemberPathReason, InvalidSourceReason, ManifestConfigError,
        ProjectConfigError, WorkspaceConfigError,
    },
    project::discovery::{ContextDiscoveryError, DiscoveryError},
    project::{
        InvalidPathReason, ProjectPathError,
        build::{
            BuildSettingsError, InvalidArtifactsDirectoryReason, ManagedInfobaseError, ToolError,
            ToolSource,
        },
        onboarding::WorkspaceEnrollmentError,
        selection::SelectionError,
    },
    vcs::workflow::PolicyError,
};

/// Keep existing consumers plain while new/import explicitly opt into terminal path links.
pub(super) fn present_workspace_enrollment_error(
    error: &WorkspaceEnrollmentError,
    localizer: &Localizer,
) -> String {
    present_workspace_enrollment_error_with_links(error, localizer, false)
}

/// Keep existing consumers plain while new/import explicitly opt into terminal path links.
pub(super) fn present_project_error(error: &DiscoveryError, localizer: &Localizer) -> String {
    present_project_error_with_links(error, localizer, false)
}

/// Keep existing consumers plain while new/import explicitly opt into terminal path links.
pub(super) fn present_context_error(
    error: &ContextDiscoveryError,
    localizer: &Localizer,
) -> String {
    present_context_error_with_links(error, localizer, false)
}

/// Keep existing consumers plain while new/import explicitly opt into terminal path links.
pub(super) fn present_global_config_error(
    error: &GlobalConfigError,
    localizer: &Localizer,
) -> String {
    present_global_config_error_with_links(error, localizer, false)
}

/// Keep existing consumers plain while new/import explicitly opt into terminal path links.
pub(super) fn present_tool_error(error: &ToolError, localizer: &Localizer) -> String {
    present_tool_error_with_links(error, localizer, false)
}

/// Present enrollment failures with an explicit output-stream link policy.
pub(super) fn present_workspace_enrollment_error_with_links(
    error: &WorkspaceEnrollmentError,
    localizer: &Localizer,
    hyperlinks: bool,
) -> String {
    match error {
        WorkspaceEnrollmentError::Io { path, source } => {
            io_message(localizer, path, source, hyperlinks)
        }
        WorkspaceEnrollmentError::Config(_)
        | WorkspaceEnrollmentError::TomlEdit(_)
        | WorkspaceEnrollmentError::MembersNotArray => {
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

/// Present project failures with stream-specific filesystem links.
pub(super) fn present_project_error_with_links(
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
pub(super) fn present_context_error_with_links(
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

/// Present machine-local config failures shared by config, build, platform and patch commands.
pub(super) fn present_global_config_error_with_links(
    error: &GlobalConfigError,
    localizer: &Localizer,
    hyperlinks: bool,
) -> String {
    match error {
        GlobalConfigError::LocationUnavailable => localizer.text("config-location-error"),
        GlobalConfigError::Io { path, source } | GlobalConfigError::Replace { path, source } => {
            localizer.format(
                "config-io-error",
                &[
                    (
                        "path",
                        LocalizationValue::Text(&path_output::render(path, hyperlinks)),
                    ),
                    ("reason", LocalizationValue::Text(&source.to_string())),
                ],
            )
        }
        GlobalConfigError::Invalid { path, source } => localizer.format(
            "config-invalid",
            &[
                (
                    "path",
                    LocalizationValue::Text(&path_output::render(path, hyperlinks)),
                ),
                ("reason", LocalizationValue::Text(&source.to_string())),
            ],
        ),
        GlobalConfigError::DistroboxContainerMissing { path } => path_message(
            localizer,
            "config-distrobox-container-missing",
            path,
            hyperlinks,
        ),
        GlobalConfigError::HostContainerUnexpected { path } => path_message(
            localizer,
            "config-host-container-unexpected",
            path,
            hyperlinks,
        ),
        GlobalConfigError::Editor { source } => localizer.format(
            "config-editor-error",
            &[("reason", LocalizationValue::Text(&source.to_string()))],
        ),
        GlobalConfigError::EditorFailed => localizer.text("config-editor-failed"),
    }
}

/// Present platform discovery and exact-version failures shared by platform consumers.
pub(super) fn present_tool_error_with_links(
    error: &ToolError,
    localizer: &Localizer,
    hyperlinks: bool,
) -> String {
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
            &[(
                "path",
                LocalizationValue::Text(&path_output::render(path, hyperlinks)),
            )],
        ),
        ToolError::DistroboxContainerRequired => {
            localizer.text("build-distrobox-container-required")
        }
        ToolError::Scan { path, source } => localizer.format(
            "platform-scan-error",
            &[
                (
                    "path",
                    LocalizationValue::Text(&path_output::render(path, hyperlinks)),
                ),
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
                (
                    "path",
                    LocalizationValue::Text(&path_output::render(standard, hyperlinks)),
                ),
            ],
        ),
        ToolError::Run(error) => localizer.format(
            "build-ibcmd-run-error",
            &[("reason", LocalizationValue::Text(&error.to_string()))],
        ),
        ToolError::VersionCommandFailed { source, stderr } => localizer.format(
            "build-version-command-error",
            &[
                (
                    "source",
                    LocalizationValue::Text(&tool_source(source, hyperlinks)),
                ),
                ("reason", LocalizationValue::Text(stderr)),
            ],
        ),
        ToolError::VersionUnreadable(source) => localizer.format(
            "build-version-unreadable",
            &[(
                "source",
                LocalizationValue::Text(&tool_source(source, hyperlinks)),
            )],
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
                (
                    "source",
                    LocalizationValue::Text(&tool_source(source, hyperlinks)),
                ),
            ],
        ),
    }
}

/// Present managed build-infobase failures shared by build and clean commands.
pub(super) fn present_managed_infobase_error(
    error: &ManagedInfobaseError,
    localizer: &Localizer,
) -> String {
    match error {
        ManagedInfobaseError::Io { path, source } => localizer.format(
            "managed-infobase-io",
            &[
                ("path", LocalizationValue::Text(&path.to_string_lossy())),
                ("reason", LocalizationValue::Text(&source.to_string())),
            ],
        ),
        ManagedInfobaseError::Locked { path } => localizer.format(
            "managed-infobase-locked",
            &[("path", LocalizationValue::Text(&path.to_string_lossy()))],
        ),
        ManagedInfobaseError::Unowned { path } => localizer.format(
            "managed-infobase-unowned",
            &[("path", LocalizationValue::Text(&path.to_string_lossy()))],
        ),
        ManagedInfobaseError::OutsideScope { root, scope } => localizer.format(
            "managed-infobase-outside-scope",
            &[
                ("path", LocalizationValue::Text(&root.to_string_lossy())),
                ("scope", LocalizationValue::Text(&scope.to_string_lossy())),
            ],
        ),
        ManagedInfobaseError::StateSerialize(_) => localizer.text("managed-infobase-state"),
    }
}

/// Keep a runner source readable while linking its explicitly reported path.
fn tool_source(source: &ToolSource, hyperlinks: bool) -> String {
    match source {
        ToolSource::Explicit(path) | ToolSource::Path(path) | ToolSource::Standard(path) => {
            path_output::render(path, hyperlinks)
        }
        ToolSource::Distrobox { container, path } => {
            format!("{container}:{}", path_output::render(path, hyperlinks))
        }
    }
}

/// Carry link policy through nested project configuration diagnostics.
fn config_message(
    localizer: &Localizer,
    path: &Path,
    error: &ProjectConfigError,
    hyperlinks: bool,
) -> String {
    let manifest_path = path;
    match error {
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
fn io_message(localizer: &Localizer, path: &Path, error: &io::Error, hyperlinks: bool) -> String {
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
fn path_message(localizer: &Localizer, key: &str, path: &Path, hyperlinks: bool) -> String {
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

#[cfg(test)]
mod tests {
    use std::{io, path::PathBuf};

    use super::{
        present_context_error, present_context_error_with_links, present_project_error,
        present_project_error_with_links,
    };
    use crate::{
        cli::localization::{Locale, Localizer},
        config::{InvalidSourceReason, ProjectConfigError},
        project::discovery::{ContextDiscoveryError, DiscoveryError},
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

    /// Nested discovery errors link every structured path without changing other consumers.
    #[test]
    fn links_both_paths_in_workspace_discovery_failures() {
        let root = std::env::current_dir().unwrap();
        let error = ContextDiscoveryError::UnlistedProject {
            project: root.join("Проект #1"),
            workspace: root.join("workspace root"),
        };
        for locale in [Locale::RuRu, Locale::EnUs] {
            let localizer = Localizer::try_new(locale).unwrap();
            let plain = present_context_error(&error, &localizer);
            assert!(!plain.contains('\x1b'));
            assert!(plain.contains("Проект #1") && plain.contains("workspace root"));
            let linked = present_context_error_with_links(&error, &localizer, true);
            assert_eq!(linked.matches("\x1b]8;;file:").count(), 2);
            assert!(linked.contains("%20%231"));
            assert!(linked.contains("workspace%20root"));
            assert_eq!(
                present_context_error_with_links(&error, &localizer, false),
                plain
            );
        }
    }

    /// A config path belongs to the manifest directory even when CLI runs elsewhere.
    #[test]
    fn config_path_links_use_the_manifest_directory() {
        let manifest = std::env::current_dir().unwrap().join("elsewhere/eska.toml");
        let error = DiscoveryError::Config {
            path: manifest,
            source: ProjectConfigError::InvalidSource {
                path: PathBuf::from("nested/../outside"),
                reason: InvalidSourceReason::ContainsParentTraversal,
            },
        };
        for locale in [Locale::RuRu, Locale::EnUs] {
            let localizer = Localizer::try_new(locale).unwrap();
            let message = present_project_error_with_links(&error, &localizer, true);
            let target_suffix = if cfg!(windows) {
                "/elsewhere/outside\x1b\\"
            } else {
                "/elsewhere/nested/../outside\x1b\\"
            };
            assert!(message.contains(target_suffix), "{message}");
            assert!(message.contains("\x1b\\nested/../outside\x1b]8;;\x1b\\"));
        }
    }
}
