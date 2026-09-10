//! Build settings, artifact planning and execution for Designer XML projects.

mod execute;
mod infobase;
mod manifest;
mod plan;
mod settings;
mod tool;

pub use execute::{
    BuildError, BuildResult, BuildStage, execute, execute_streaming,
    execute_streaming_with_manifest, preflight, preflight_manifest,
};
pub use infobase::{
    CleanOutcome as InfobaseCleanOutcome, ManagedInfobaseError, clean as clean_infobase,
};
pub use manifest::{ManifestError, path_for_artifact as manifest_path};
pub use plan::{
    ArtifactType, BuildPlan, PlanError, managed_infobase_root, validate_unique_outputs,
};
pub use settings::{
    BuildSettings, BuildSettingsError, InvalidArtifactsDirectoryReason, PlatformVersion,
};
pub use tool::{
    Ibcmd, InstalledPlatform, ProcessStream, RunError, RunnerPreference, ToolError, ToolOptions,
    ToolSource,
};
