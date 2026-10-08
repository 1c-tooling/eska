//! Application adapter that supplies Git facts to the independent native pipeline.

use super::{BuildError, BuildPlan, BuildResult, BuildStage, Ibcmd, ProcessStream, manifest};
use crate::project::Project;

/// Build an immutable source snapshot and record application-level repository facts.
///
/// # Errors
/// Returns the shared native pipeline's structured source, process or publication failures.
pub fn execute_streaming_with_manifest<F>(
    plan: &BuildPlan,
    project: &Project,
    project_name: Option<&str>,
    ibcmd: &Ibcmd,
    on_output: F,
) -> Result<BuildResult, BuildError>
where
    F: FnMut(BuildStage, ProcessStream, &[u8]),
{
    crate::build_core::preflight_manifest(plan)?;
    let provenance = manifest::capture(project, project_name);
    crate::build_core::execute_streaming_with_manifest(plan, project, provenance, ibcmd, on_output)
}
