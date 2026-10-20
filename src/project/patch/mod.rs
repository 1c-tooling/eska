//! Temporary composition adapter; VCS owns snapshots and build owns native execution.

pub use eska_vcs::project::patch::{PatchChange, PatchError, PatchModule, PatchPlan, plan};

/// Preserve the existing composite call while both implementations live with their tool owners.
///
/// # Errors
/// Returns planning, materialization, native validation or exclusive publication failures.
pub fn execute(
    plan: &PatchPlan,
    tool: &crate::project::build::Ibcmd,
    output: &std::path::Path,
) -> Result<(), PatchError> {
    eska_build_runtime::patch::execute(&plan.description, tool, output, |workspace| {
        eska_vcs::project::patch::materialize(plan, workspace)
    })
}
