//! A companion-created member retains its own directory while existing enrollment owns root settings.

use std::fs;

use super::{CreationError, DiscoveryContext, Project, WorkspaceCreationPlan, discovery};
use crate::project::onboarding::WorkspaceEnrollmentError;

/// Prepare only the conventional parent; the child tool must exclusively claim the member itself.
///
/// # Errors
/// Refuses a raced/unsafe parent without deleting any existing directory.
pub fn prepare_member_parent(plan: &WorkspaceCreationPlan) -> Result<bool, CreationError> {
    if plan.create_parent {
        fs::create_dir(&plan.parent).map_err(|source| CreationError::Io {
            path: plan.parent.clone(),
            source,
        })?;
        Ok(true)
    } else if fs::canonicalize(&plan.parent).map_err(|source| CreationError::Io {
        path: plan.parent.clone(),
        source,
    })? == plan.parent
    {
        Ok(false)
    } else {
        Err(CreationError::InvalidDestination {
            path: plan.parent.clone(),
        })
    }
}

/// Remove an owned parent only when empty; retain a successfully or partially created child project.
pub fn cleanup_member_parent(plan: &WorkspaceCreationPlan, owned: bool) {
    if owned {
        let _ = fs::remove_dir(&plan.parent);
    }
}

/// Enroll a verified child result using exact root-manifest publication and validation/restore.
///
/// # Errors
/// Returns concurrent settings/member changes or invalid inherited policy. No path deletes the
/// member directory, including when root-manifest restoration is refused after an external edit.
pub fn enroll_created_member(
    plan: &WorkspaceCreationPlan,
    expected_manifest: &str,
) -> Result<Project, CreationError> {
    let path = plan.destination.join(crate::config::FILE_NAME);
    let bytes = fs::read(&path).map_err(|source| CreationError::Io {
        path: path.clone(),
        source,
    })?;
    if bytes != expected_manifest.as_bytes() {
        return Err(CreationError::Workspace(
            WorkspaceEnrollmentError::ManifestChanged { path },
        ));
    }
    plan.enrollment
        .publish()
        .map_err(CreationError::Workspace)?;
    let result = validate(plan);
    match result {
        Ok(project) => Ok(project),
        Err(original) => {
            if plan.enrollment.restore().is_ok() {
                Err(original)
            } else {
                Err(CreationError::WorkspaceRollback {
                    paths: vec![
                        plan.enrollment
                            .workspace_root()
                            .join(crate::config::FILE_NAME),
                    ],
                    original: Box::new(original),
                })
            }
        }
    }
}

/// Full ordinary discovery applies member naming, inherited settings and native-version policy.
fn validate(plan: &WorkspaceCreationPlan) -> Result<Project, CreationError> {
    let context = discovery::discover_context(plan.enrollment.workspace_root())
        .map_err(|error| CreationError::WorkspaceValidation(Box::new(error)))?;
    let DiscoveryContext::Workspace { workspace, .. } = context else {
        return Err(CreationError::WorkspaceMemberMissing {
            name: plan.name.clone(),
        });
    };
    workspace
        .member(&plan.name)
        .filter(|member| member.root() == plan.destination)
        .map(|member| member.project().clone())
        .ok_or_else(|| CreationError::WorkspaceMemberMissing {
            name: plan.name.clone(),
        })
}
