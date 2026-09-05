//! Build settings, artifact planning and execution for Designer XML projects.

mod plan;
mod settings;

pub use plan::{ArtifactType, BuildPlan, PlanError};
pub use settings::{
    BuildSettings, BuildSettingsError, InvalidArtifactsDirectoryReason, PlatformVersion,
};
