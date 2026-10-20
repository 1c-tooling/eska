//! Shared localized presentation of project, configuration and platform errors.

use crate::{
    cli::{
        localization::{LocalizationValue, Localizer},
        path_output,
    },
    config::GlobalConfigError,
    project::build::{ManagedInfobaseError, ToolError, ToolSource},
};
use eska_presentation::diagnostics::path_message;
pub(super) use eska_presentation::diagnostics::{
    present_context_error, present_context_error_with_links, present_project_error,
    present_project_error_with_links, present_selection_error, present_workspace_enrollment_error,
    present_workspace_enrollment_error_with_links,
};

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
