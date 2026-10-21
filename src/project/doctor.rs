//! Compatibility reexports of the single diagnostic implementations in their actual owners.

use super::{Project, build::ToolOptions};
pub use eska_project_core::doctor::{Check, CheckStatus, Report, Summary};
pub use eska_vcs::doctor::{inspect_system_git, inspect_vcs};

/// Preserve the transitional Rust entry while platform checks live outside the pure core.
#[must_use]
pub fn inspect_project(
    project: &Project,
    name: Option<&str>,
    tools: Option<&ToolOptions>,
) -> Vec<Check> {
    let mut checks = eska_project_core::doctor::inspect_project(project, name);
    checks.extend(eska_build_runtime::doctor::inspect(project, name, tools));
    checks
}
