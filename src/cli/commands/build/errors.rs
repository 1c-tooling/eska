//! Localized build errors and stable machine-facing error classifications.

use std::io;

use crate::{
    cli::{
        diagnostics,
        localization::{LocalizationValue, Localizer},
    },
    project::{
        build::{
            BuildError, BuildSettingsError, BuildStage, ManagedInfobaseError, ManifestError,
            PlanError, RunError, ToolError,
        },
        selection::SelectionError,
    },
};

pub(super) enum BuildExecutionError {
    Build(BuildError),
    Output(io::Error),
}

/// Present execution or output failures through localized diagnostics.
pub(super) fn execution_error_message(
    error: &BuildExecutionError,
    localizer: &Localizer,
) -> String {
    match error {
        BuildExecutionError::Build(error) => present_streamed_build_error(error, localizer),
        BuildExecutionError::Output(error) => localizer.format(
            "build-output-write-error",
            &[("reason", LocalizationValue::Text(&error.to_string()))],
        ),
    }
}

/// Render output-plan errors without changing their stable domain representation.
pub(super) fn present_plan_error(error: &PlanError, localizer: &Localizer) -> String {
    let (key, path) = match error {
        PlanError::ProjectNameMissing => return localizer.text("build-project-name-missing"),
        PlanError::PlatformVersionMissing => {
            return localizer.text("build-platform-version-missing");
        }
        PlanError::InvalidOutput { path } => ("build-output-invalid", path),
        PlanError::UnexpectedExtension { path, expected } => {
            return localizer.format(
                "build-output-extension",
                &[
                    ("path", LocalizationValue::Text(&path.to_string_lossy())),
                    ("extension", LocalizationValue::Text(expected)),
                ],
            );
        }
        PlanError::OutputCollision { path } => ("build-output-collision", path),
        PlanError::BaseConfigurationUnsupported => {
            return localizer.text("build-base-configuration-unsupported");
        }
    };
    localizer.format(
        key,
        &[("path", LocalizationValue::Text(&path.to_string_lossy()))],
    )
}

/// Localize the one-run platform version validation failure.
pub(super) fn present_platform_version_error(
    error: &BuildSettingsError,
    localizer: &Localizer,
) -> String {
    match error {
        BuildSettingsError::InvalidPlatformVersion { value } => localizer.format(
            "build-platform-version-invalid",
            &[("value", LocalizationValue::Text(value))],
        ),
        BuildSettingsError::InvalidArtifactsDirectory { .. } => {
            localizer.text("build-platform-version-error")
        }
    }
}

/// Render build execution failures and identify the failing stage.
fn present_build_error(error: &BuildError, localizer: &Localizer) -> String {
    match error {
        BuildError::OutputParentMissing(path)
        | BuildError::InvalidExistingOutput(path)
        | BuildError::ArtifactMissing(path)
        | BuildError::ArtifactEmpty(path) => localizer.format(
            match error {
                BuildError::InvalidExistingOutput(_) => "build-output-existing-invalid",
                BuildError::ArtifactMissing(_) => "build-artifact-missing",
                BuildError::ArtifactEmpty(_) => "build-artifact-empty",
                _ => "build-output-invalid",
            },
            &[("path", LocalizationValue::Text(&path.to_string_lossy()))],
        ),
        BuildError::ConfiguredOutputOutsideProject { root, output } => localizer.format(
            "build-output-outside-project",
            &[
                ("root", LocalizationValue::Text(&root.to_string_lossy())),
                ("path", LocalizationValue::Text(&output.to_string_lossy())),
            ],
        ),
        BuildError::CreateDirectory { path, source }
        | BuildError::CreateWorkspace { path, source }
        | BuildError::Publish { path, source }
        | BuildError::Restore { path, source }
        | BuildError::DescriptorDirectory { path, source }
        | BuildError::DescriptorRead { path, source } => localizer.format(
            "build-filesystem-error",
            &[
                ("path", LocalizationValue::Text(&path.to_string_lossy())),
                ("reason", LocalizationValue::Text(&source.to_string())),
            ],
        ),
        BuildError::Run { stage, source } => localizer.format(
            match source {
                RunError::Interrupted => "build-interrupted",
                _ => "build-process-error",
            },
            &[
                (
                    "stage",
                    LocalizationValue::Text(&stage_name(*stage, localizer)),
                ),
                ("reason", LocalizationValue::Text(&format!("{source:?}"))),
            ],
        ),
        BuildError::CommandFailed { stage, stderr } => localizer.format(
            "build-command-failed",
            &[
                (
                    "stage",
                    LocalizationValue::Text(&stage_name(*stage, localizer)),
                ),
                ("reason", LocalizationValue::Text(stderr)),
            ],
        ),
        BuildError::DescriptorInvalid { path } => localizer.format(
            "build-descriptor-invalid",
            &[("path", LocalizationValue::Text(&path.to_string_lossy()))],
        ),
        BuildError::DescriptorMissing(path) => localizer.format(
            "build-descriptor-missing",
            &[("path", LocalizationValue::Text(&path.to_string_lossy()))],
        ),
        BuildError::DescriptorsMultiple(path) => localizer.format(
            "build-descriptors-multiple",
            &[("path", LocalizationValue::Text(&path.to_string_lossy()))],
        ),
        BuildError::BaseConfigurationInvalid(path) => localizer.format(
            "build-base-configuration-invalid",
            &[("path", LocalizationValue::Text(&path.to_string_lossy()))],
        ),
        BuildError::BaseConfigurationRead { path, source } => localizer.format(
            "build-base-configuration-read",
            &[
                ("path", LocalizationValue::Text(&path.to_string_lossy())),
                ("reason", LocalizationValue::Text(&source.to_string())),
            ],
        ),
        BuildError::ManagedInfobase(error) => {
            diagnostics::present_managed_infobase_error(error, localizer)
        }
        BuildError::Manifest(error) => present_manifest_error(error, localizer),
    }
}

/// Render source-snapshot and manifest failures without adding localized text to core.
fn present_manifest_error(error: &ManifestError, localizer: &Localizer) -> String {
    match error {
        ManifestError::SnapshotIo { path, source } => localizer.format(
            "build-manifest-filesystem-error",
            &[
                ("path", LocalizationValue::Text(&path.to_string_lossy())),
                ("reason", LocalizationValue::Text(&source.to_string())),
            ],
        ),
        ManifestError::SnapshotEntryUnsupported(path) => localizer.format(
            "build-manifest-entry-unsupported",
            &[("path", LocalizationValue::Text(&path.to_string_lossy()))],
        ),
        ManifestError::SnapshotProject(_) => localizer.text("build-manifest-snapshot-invalid"),
        ManifestError::ArtifactRead { path, source } | ManifestError::Write { path, source } => {
            localizer.format(
                "build-manifest-filesystem-error",
                &[
                    ("path", LocalizationValue::Text(&path.to_string_lossy())),
                    ("reason", LocalizationValue::Text(&source.to_string())),
                ],
            )
        }
        ManifestError::Serialize(_) => localizer.text("build-manifest-serialize-error"),
    }
}

/// Avoid repeating process output that the streaming renderer has already emitted.
fn present_streamed_build_error(error: &BuildError, localizer: &Localizer) -> String {
    if let BuildError::CommandFailed { stage, .. } = error {
        localizer.format(
            "build-command-failed-streamed",
            &[(
                "stage",
                LocalizationValue::Text(&stage_name(*stage, localizer)),
            )],
        )
    } else {
        present_build_error(error, localizer)
    }
}

/// Localize the fixed pipeline stage while retaining enum-based control flow.
fn stage_name(stage: BuildStage, localizer: &Localizer) -> String {
    localizer.text(match stage {
        BuildStage::CreateInfobase => "build-stage-create-infobase",
        BuildStage::ImportSources => "build-stage-import-sources",
    })
}

/// Map selector failures to stable machine-facing codes.
pub(super) const fn selection_error_code(error: &SelectionError) -> &'static str {
    match error {
        SelectionError::InvalidName(_) => "project-name-invalid",
        SelectionError::DuplicateSelector { .. } => "project-selector-duplicate",
        SelectionError::UnknownProject { .. } => "project-not-found",
        SelectionError::WorkspaceSelectorForStandalone => "workspace-selector-for-standalone",
        SelectionError::ConflictingSelectors => "project-selectors-conflict",
        SelectionError::ExplicitProjectRequired => "project-selector-required",
        SelectionError::SingleProjectRequired => "single-project-required",
    }
}

/// Map immutable plan failures to stable machine-facing codes.
pub(super) const fn plan_error_code(error: &PlanError) -> &'static str {
    match error {
        PlanError::ProjectNameMissing => "project-name-missing",
        PlanError::PlatformVersionMissing => "platform-version-missing",
        PlanError::InvalidOutput { .. } => "output-invalid",
        PlanError::UnexpectedExtension { .. } => "output-extension-invalid",
        PlanError::OutputCollision { .. } => "output-collision",
        PlanError::BaseConfigurationUnsupported => "base-configuration-unsupported",
    }
}

/// Map tool discovery failures to stable machine-facing codes.
pub(super) const fn tool_error_code(error: &ToolError) -> &'static str {
    match error {
        ToolError::InvalidArchitecture(_) => "platform-architecture-invalid",
        ToolError::InvalidContainer(_) => "distrobox-container-invalid",
        ToolError::InvalidExecutable(_) => "ibcmd-executable-invalid",
        ToolError::DistroboxContainerRequired => "distrobox-container-required",
        ToolError::Scan { .. } | ToolError::ScanCommandFailed { .. } => "platform-scan-failed",
        ToolError::NotFound { .. } => "ibcmd-not-found",
        ToolError::Run(_) => "ibcmd-run-failed",
        ToolError::VersionCommandFailed { .. } => "ibcmd-version-command-failed",
        ToolError::VersionUnreadable(_) => "ibcmd-version-unreadable",
        ToolError::VersionMismatch { .. } => "ibcmd-version-mismatch",
    }
}

/// Map execution and output failures to stable machine-facing codes.
pub(super) const fn execution_error_code(error: &BuildExecutionError) -> &'static str {
    let BuildExecutionError::Build(error) = error else {
        return "output-write";
    };
    match error {
        BuildError::OutputParentMissing(_) => "output-parent-missing",
        BuildError::ConfiguredOutputOutsideProject { .. } => "output-outside-scope",
        BuildError::InvalidExistingOutput(_) => "output-existing-invalid",
        BuildError::CreateDirectory { .. } => "create-directory",
        BuildError::CreateWorkspace { .. } => "create-workspace",
        BuildError::Run {
            source: RunError::Interrupted,
            ..
        } => "interrupted",
        BuildError::Run { .. } => "process-error",
        BuildError::CommandFailed { .. } => "command-failed",
        BuildError::ArtifactMissing(_) => "artifact-missing",
        BuildError::ArtifactEmpty(_) => "artifact-empty",
        BuildError::Publish { .. } => "publish",
        BuildError::Restore { .. } => "restore",
        BuildError::DescriptorDirectory { .. } => "descriptor-directory",
        BuildError::DescriptorRead { .. } => "descriptor-read",
        BuildError::DescriptorInvalid { .. } => "descriptor-invalid",
        BuildError::DescriptorMissing(_) => "descriptor-missing",
        BuildError::DescriptorsMultiple(_) => "descriptors-multiple",
        BuildError::BaseConfigurationRead { .. } => "base-configuration-read",
        BuildError::BaseConfigurationInvalid(_) => "base-configuration-invalid",
        BuildError::ManagedInfobase(ManagedInfobaseError::Io { .. }) => "infobase-io",
        BuildError::ManagedInfobase(ManagedInfobaseError::Locked { .. }) => "infobase-locked",
        BuildError::ManagedInfobase(ManagedInfobaseError::Unowned { .. }) => "infobase-unowned",
        BuildError::ManagedInfobase(ManagedInfobaseError::OutsideScope { .. }) => {
            "infobase-outside-scope"
        }
        BuildError::ManagedInfobase(ManagedInfobaseError::StateSerialize(_)) => "infobase-state",
        BuildError::Manifest(ManifestError::SnapshotIo { .. }) => "snapshot-io",
        BuildError::Manifest(ManifestError::SnapshotEntryUnsupported(_)) => {
            "snapshot-entry-unsupported"
        }
        BuildError::Manifest(ManifestError::SnapshotProject(_)) => "snapshot-invalid",
        BuildError::Manifest(ManifestError::ArtifactRead { .. }) => "artifact-checksum",
        BuildError::Manifest(ManifestError::Write { .. }) => "manifest-write",
        BuildError::Manifest(ManifestError::Serialize(_)) => "manifest-serialize",
    }
}

/// Identify the failing stage without parsing localized diagnostics.
pub(super) const fn execution_error_stage(error: &BuildExecutionError) -> Option<&'static str> {
    match error {
        BuildExecutionError::Build(
            BuildError::Run { stage, .. } | BuildError::CommandFailed { stage, .. },
        ) => Some(match stage {
            BuildStage::CreateInfobase => "create-infobase",
            BuildStage::ImportSources => "import-sources",
        }),
        BuildExecutionError::Build(BuildError::Manifest(
            ManifestError::ArtifactRead { .. }
            | ManifestError::Write { .. }
            | ManifestError::Serialize(_),
        )) => Some("manifest"),
        BuildExecutionError::Build(BuildError::Manifest(_)) => Some("snapshot"),
        BuildExecutionError::Build(_) | BuildExecutionError::Output(_) => None,
    }
}

/// Stop aggregate execution only after an explicit process interruption.
pub(super) const fn execution_was_interrupted(error: &BuildExecutionError) -> bool {
    matches!(
        error,
        BuildExecutionError::Build(BuildError::Run {
            source: RunError::Interrupted,
            ..
        })
    )
}
