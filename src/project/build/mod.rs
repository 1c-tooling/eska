//! Build settings, artifact planning and execution for Designer XML projects.

mod execute;
mod manifest;

pub use crate::build_core::{
    ArtifactType, BuildError, BuildPlan, BuildResult, BuildSettings, BuildSettingsError,
    BuildStage, Ibcmd, InfobaseCleanOutcome, InstalledPlatform, InvalidArtifactsDirectoryReason,
    ManagedInfobaseError, ManifestError, PlanError, PlatformVersion, ProcessStream, RunError,
    RunnerPreference, ToolError, ToolOptions, ToolSource, clean_infobase, execute,
    execute_streaming, managed_infobase_root, manifest_path, preflight, preflight_manifest,
    validate_unique_outputs,
};

pub use execute::execute_streaming_with_manifest;
