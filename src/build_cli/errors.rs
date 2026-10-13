//! Native domain errors become stable codes, paths and fields at the process boundary.

use std::path::Path;

use serde_json::json;

use crate::{
    build_core::{
        BuildError, BuildStage, ManagedInfobaseError, ManifestError, PlanError, RunError,
        ToolError, machine::GlobalConfigError,
    },
    tool_cli::Failure,
};

/// Retain the concrete plan constraint rather than parsing a localized error string.
pub(super) fn plan(error: PlanError) -> Failure {
    match error {
        PlanError::SourcePreparationRequired => Failure::new("source_preparation_required"),
        PlanError::ProjectNameMissing => Failure::new("project_name_missing"),
        PlanError::PlatformVersionMissing => Failure::new("platform_version_missing"),
        PlanError::InvalidOutput { path } => at("build_output_invalid", &path),
        PlanError::UnexpectedExtension { path, expected } => Failure {
            diagnostics: json!({"expected_extension":expected}),
            ..at("build_output_extension_invalid", &path)
        },
        PlanError::OutputCollision { path } => at("build_output_collision", &path),
        PlanError::BaseConfigurationUnsupported => {
            Failure::new("build_base_configuration_unsupported")
        }
    }
}

/// Discovery reports runner/version constraints without copying OS-localized messages to JSON.
pub(super) fn tool(error: ToolError) -> Failure {
    match error {
        ToolError::InvalidArchitecture(value) => {
            Failure::new("platform_architecture_invalid").at(value)
        }
        ToolError::InvalidContainer(value) => Failure::new("platform_container_invalid").at(value),
        ToolError::InvalidExecutable(path) => at("platform_executable_invalid", &path),
        ToolError::DistroboxContainerRequired => Failure::new("platform_container_required"),
        ToolError::Scan { path, .. } => at("platform_scan_failed", &path),
        ToolError::ScanCommandFailed { container, .. } => {
            Failure::new("platform_scan_failed").at(container)
        }
        ToolError::NotFound { expected, standard } => Failure {
            diagnostics: json!({"expected_version":expected.as_str()}),
            ..at("platform_not_found", &standard)
        },
        ToolError::Run(_) => Failure::new("platform_launch_failed"),
        ToolError::VersionCommandFailed { .. } => Failure::new("platform_version_command_failed"),
        ToolError::VersionUnreadable(_) => Failure::new("platform_version_unreadable"),
        ToolError::VersionMismatch {
            expected, actual, ..
        } => Failure {
            diagnostics: json!({"expected_version":expected.as_str(),"actual_version":actual.as_str()}),
            ..Failure::new("platform_version_mismatch")
        },
    }
}

/// Machine configuration is read by both CLIs through the same strict schema and location policy.
pub(super) fn machine(error: GlobalConfigError) -> Failure {
    match error {
        GlobalConfigError::LocationUnavailable => {
            Failure::new("machine_config_location_unavailable")
        }
        GlobalConfigError::Io { path, .. } | GlobalConfigError::Replace { path, .. } => {
            at("machine_config_io", &path)
        }
        GlobalConfigError::Invalid { path, .. } => at("machine_config_invalid", &path),
        GlobalConfigError::DistroboxContainerMissing { path } => {
            at("machine_config_container_missing", &path)
        }
        GlobalConfigError::HostContainerUnexpected { path } => {
            at("machine_config_container_unexpected", &path)
        }
        GlobalConfigError::Editor { .. } | GlobalConfigError::EditorFailed => {
            Failure::new("machine_config_editor_failed")
        }
    }
}

/// Preserve source failures, build stages and publication paths on the unsuccessful native route.
pub(super) fn build(error: BuildError) -> Failure {
    match error {
        BuildError::SourceValidation(error) => Failure::from(error),
        BuildError::Run { stage, source } => Failure {
            diagnostics: json!({"stage":stage_name(stage)}),
            ..run(&source)
        },
        BuildError::CommandFailed { stage, .. } => Failure {
            diagnostics: json!({"stage":stage_name(stage)}),
            ..Failure::new("platform_command_failed")
        },
        BuildError::OutputParentMissing(path) => at("build_output_parent_missing", &path),
        BuildError::ConfiguredOutputOutsideProject { output, .. } => {
            at("build_output_outside_project", &output)
        }
        BuildError::InvalidExistingOutput(path) => at("build_output_invalid", &path),
        BuildError::CreateDirectory { path, .. } => at("build_directory_create_failed", &path),
        BuildError::CreateWorkspace { path, .. } => at("build_workspace_create_failed", &path),
        BuildError::ArtifactMissing(path) => at("build_artifact_missing", &path),
        BuildError::ArtifactEmpty(path) => at("build_artifact_empty", &path),
        BuildError::Publish { path, .. } => at("build_publication_failed", &path),
        BuildError::Restore { path, .. } => at("build_restoration_failed", &path),
        BuildError::DescriptorDirectory { path, .. } | BuildError::DescriptorRead { path, .. } => {
            at("build_source_read_failed", &path)
        }
        BuildError::DescriptorInvalid { path } => at("build_descriptor_invalid", &path),
        BuildError::DescriptorMissing(path) => at("build_descriptor_missing", &path),
        BuildError::DescriptorsMultiple(path) => at("build_descriptors_ambiguous", &path),
        BuildError::BaseConfigurationRead { path, .. } => {
            at("build_base_configuration_read_failed", &path)
        }
        BuildError::BaseConfigurationInvalid(path) => at("build_base_configuration_invalid", &path),
        BuildError::ManagedInfobase(error) => infobase(error),
        BuildError::Manifest(error) => manifest(error),
    }
}

/// Cancellation remains distinct from a native command's negative exit code.
const fn run(error: &RunError) -> Failure {
    Failure::new(match error {
        RunError::Interrupted => "platform_interrupted",
        RunError::SignalHandler(_) => "platform_signal_handler_failed",
        RunError::Io(_) => "platform_launch_failed",
        RunError::MissingPipe => "platform_pipe_missing",
        RunError::ReaderPanicked => "platform_output_reader_failed",
    })
}

/// Managed-cache ownership and lock failures retain their actionable paths.
fn infobase(error: ManagedInfobaseError) -> Failure {
    match error {
        ManagedInfobaseError::Io { path, .. } => at("build_infobase_io", &path),
        ManagedInfobaseError::Locked { path } => at("build_infobase_locked", &path),
        ManagedInfobaseError::Unowned { path } => at("build_infobase_unowned", &path),
        ManagedInfobaseError::OutsideScope { root, .. } => {
            at("build_infobase_outside_scope", &root)
        }
        ManagedInfobaseError::StateSerialize(_) => Failure::new("build_infobase_state_invalid"),
    }
}

/// Artifact and snapshot errors retain machine codes; the platform event list is attached separately.
fn manifest(error: ManifestError) -> Failure {
    match error {
        ManifestError::SnapshotIo { path, .. } => at("build_snapshot_io", &path),
        ManifestError::SnapshotEntryUnsupported(path) => {
            at("build_snapshot_entry_unsupported", &path)
        }
        ManifestError::SnapshotProject(_) => Failure::new("build_snapshot_project_invalid"),
        ManifestError::VersionPolicy(error) => {
            let diagnostics = match &error {
                crate::project_core::native_version::VersionError::SchemeMismatch {
                    expected,
                    actual,
                } => {
                    json!({"expected_scheme":expected.as_str(),"actual_scheme":actual.as_str()})
                }
                _ => {
                    json!({"code":error.code(),"range":error.range().map(|range| json!({"byte_start":range.start,"byte_end":range.end})),"remediation":error.remediation()})
                }
            };
            Failure {
                diagnostics,
                ..Failure::new("project_version_policy_invalid").at(error.path().map_or_else(
                    || "version.scheme".to_owned(),
                    |path| path.to_string_lossy().into_owned(),
                ))
            }
        }
        ManifestError::ArtifactRead { path, .. } => at("build_artifact_read_failed", &path),
        ManifestError::Write { path, .. } => at("build_manifest_write_failed", &path),
        ManifestError::Serialize(_) => Failure::new("build_manifest_invalid"),
    }
}

/// Paths arise from a checked UTF-8 scope; lossy text is reserved for unsupported error coordinates.
fn at(code: &'static str, path: &Path) -> Failure {
    Failure::new(code).at(path.to_string_lossy())
}

/// Stage identifiers never depend on a locale or the platform's output language.
pub(super) const fn stage_name(stage: BuildStage) -> &'static str {
    match stage {
        BuildStage::CreateInfobase => "create_infobase",
        BuildStage::ImportSources => "import_sources",
    }
}

/// Native unpacking errors do not imply a project replacement or permission change.
pub(super) fn unpack(error: crate::build_core::artifact::UnpackError) -> Failure {
    use crate::build_core::artifact::UnpackError;
    match error {
        UnpackError::Io { path, .. } => at("unpack_io", &path),
        UnpackError::UnsupportedFile(path) => at("unpack_file_unsupported", &path),
        UnpackError::InvalidDescriptor(path) => at("unpack_descriptor_invalid", &path),
        UnpackError::MissingDescriptor(path) => at("unpack_descriptor_missing", &path),
        UnpackError::AmbiguousDescriptor(path) => at("unpack_descriptors_ambiguous", &path),
        UnpackError::UnsafePath(path) => at("unpack_path_unsafe", &path),
        UnpackError::DestinationExists(path) => at("unpack_destination_exists", &path),
        UnpackError::SourceChanged(path) => at("unpack_source_changed", &path),
        UnpackError::Run(error) => run(&error),
        UnpackError::Platform { stage, .. } => Failure {
            diagnostics: json!({"stage":stage}),
            ..Failure::new("platform_command_failed")
        },
    }
}
