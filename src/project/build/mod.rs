//! Build settings, artifact planning and execution for Designer XML projects.

mod execute;
mod manifest;

pub use crate::build_core::{
    ArtifactType, BuildPlan, BuildSettings, BuildSettingsError, Ibcmd, InfobaseCleanOutcome,
    InstalledPlatform, InvalidArtifactsDirectoryReason, ManagedInfobaseError, PlanError,
    PlatformVersion, ProcessStream, RunError, RunnerPreference, ToolError, ToolOptions, ToolSource,
    clean_infobase, managed_infobase_root, validate_unique_outputs,
};

pub use execute::{
    BuildError, BuildResult, BuildStage, execute, execute_streaming,
    execute_streaming_with_manifest, preflight, preflight_manifest,
};
pub use manifest::{ManifestError, path_for_artifact as manifest_path};
