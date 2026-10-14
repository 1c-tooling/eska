//! Existing-directory enrollment never removes the initialized member or rewrites its source controls.

use std::fs;

use super::{
    DiscoveryContext, InitError, Project, ProjectName, WorkspaceMemberEnrollment, discovery,
};

/// Publish exact root membership after verifying the companion's original member manifest bytes.
///
/// # Errors
/// Returns concurrent member/root edits or validation failures. Restore applies only to unchanged
/// root-manifest bytes; the existing member and its newly initialized evidence always remain intact.
pub fn enroll_initialized_member(
    enrollment: &WorkspaceMemberEnrollment,
    name: &ProjectName,
    expected_manifest: &str,
) -> Result<Project, InitError> {
    let path = enrollment.member_root().join(crate::config::FILE_NAME);
    if fs::read(&path).map_err(|source| InitError::Io {
        path: path.clone(),
        source,
    })? != expected_manifest.as_bytes()
    {
        return Err(InitError::ChangedSource { path });
    }
    enrollment.publish().map_err(InitError::Workspace)?;
    let result = validate(enrollment, name);
    match result {
        Ok(project) => Ok(project),
        Err(original) if enrollment.restore().is_ok() => Err(original),
        Err(original) => Err(InitError::Rollback {
            paths: vec![enrollment.workspace_root().join(crate::config::FILE_NAME)],
            original: Box::new(original),
        }),
    }
}

/// Ordinary discovery validates inherited policy and the exact enrolled member coordinate.
fn validate(
    enrollment: &WorkspaceMemberEnrollment,
    name: &ProjectName,
) -> Result<Project, InitError> {
    let context = discovery::discover_context(enrollment.workspace_root())
        .map_err(|error| InitError::WorkspaceValidation(Box::new(error)))?;
    let DiscoveryContext::Workspace { workspace, .. } = context else {
        return Err(InitError::WorkspaceMemberMissing { name: name.clone() });
    };
    workspace
        .member(name)
        .filter(|member| member.root() == enrollment.member_root())
        .map(|member| member.project().clone())
        .ok_or_else(|| InitError::WorkspaceMemberMissing { name: name.clone() })
}
