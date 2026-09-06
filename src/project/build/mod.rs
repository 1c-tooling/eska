//! Build settings, artifact planning and execution for Designer XML projects.

mod execute;
mod plan;
mod settings;
mod tool;

pub use execute::{BuildError, BuildResult, BuildStage, execute, execute_streaming};
pub use plan::{ArtifactType, BuildPlan, PlanError};
pub use settings::{
    BuildSettings, BuildSettingsError, InvalidArtifactsDirectoryReason, PlatformVersion,
};
pub use tool::{
    Ibcmd, InstalledPlatform, ProcessStream, RunError, RunnerPreference, ToolError, ToolOptions,
    ToolSource,
};
