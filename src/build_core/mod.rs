//! Native artifact planning and platform infrastructure without Git, CLI or IDE state.
//!
//! The `build` feature enables this module. Source conversion and export permissions
//! belong to [`crate::project_core`]; resolving a build plan does not authorize export.

mod execute;
mod infobase;
mod manifest;
mod plan;
mod provenance;
mod tool;

pub use crate::project_core::platform_settings::{
    BuildSettings, BuildSettingsError, InvalidArtifactsDirectoryReason, PlatformVersion,
};
pub use execute::{
    BuildError, BuildResult, BuildStage, execute, execute_streaming,
    execute_streaming_with_manifest, preflight, preflight_manifest,
};
pub use infobase::{
    CleanOutcome as InfobaseCleanOutcome, ManagedInfobase, ManagedInfobaseError,
    clean as clean_infobase,
};
pub use manifest::{ManifestError, path_for_artifact as manifest_path};
pub use plan::{
    ArtifactType, BuildPlan, PlanError, managed_infobase_root, validate_unique_outputs,
};
pub use provenance::{BuildProvenance, GitProvenance};
pub use tool::{
    Ibcmd, InstalledPlatform, ProcessStream, RunError, RunnerPreference, ToolError, ToolOptions,
    ToolSource,
};
