//! The parent publishes only its own onboarding fields and never removes the child-owned root.

use super::{
    super::{NewArgs, artifact::Failure},
    authority::Verified,
    failure,
    policy::Plan,
    protocol::{self, Created},
};
use crate::{
    cli::localization::Localizer,
    project::{Project, create},
    project_core::publish,
    vcs::git,
};
use std::{fs, path::Path};

/// Candidate integrity and exact manifest/policy bytes precede each parent-owned publication.
pub(super) fn finish(
    plan: &Plan,
    args: &NewArgs,
    created: &mut Created,
    verified: &Verified,
    localizer: &Localizer,
) -> Result<Project, Failure> {
    recheck(plan, created, verified, localizer)?;
    let (original, config) =
        protocol::manifest(created).map_err(|_| failure("new-manifest-changed", localizer))?;
    if let Some(member) = &plan.member {
        return create::enroll_created_member(member, &original).map_err(|error| {
            Failure::new(
                "new-enrollment-failed",
                super::super::present(&error, localizer),
            )
        });
    }
    let workflow = plan
        .workflow
        .ok_or_else(|| failure("new-child-response-invalid", localizer))?;
    let config = config.with_workflow(workflow);
    let updated = config
        .to_toml()
        .map_err(|_| failure("new-finalization-failed", localizer))?;
    publish::publish_snapshot(
        &created.root,
        Path::new(crate::config::FILE_NAME),
        &original,
        &updated,
    )
    .map_err(|_| failure("new-manifest-changed", localizer))?;
    // This digest represents only our successful exact-byte workflow update, never a fresh read.
    created.manifest_sha256 = protocol::digest_bytes(updated.as_bytes());
    if !args.no_vcs {
        recheck(plan, created, verified, localizer)?;
        let git_dir = created.root.join(".git");
        fs::create_dir(&git_dir).map_err(|_| failure("new-git-reservation-failed", localizer))?;
        // The reserved directory is exclusive. On failure retain partial metadata for recovery;
        // recursively deleting a published child root or a raced foreign `.git` is never allowed.
        git::initialize_reserved(&git_dir)
            .map_err(|_| Failure::new("new-git-failed", localizer.text("new-git-error")))?;
    }
    recheck(plan, created, verified, localizer)?;
    config
        .into_project(created.root.clone())
        .map_err(|_| failure("new-finalization-failed", localizer))
}

/// Root aliases, source/control edits and changed root policy are not accepted as new baselines.
fn recheck(
    plan: &Plan,
    created: &Created,
    verified: &Verified,
    localizer: &Localizer,
) -> Result<(), Failure> {
    plan.verify_workspace(localizer)?;
    protocol::manifest(created).map_err(|_| failure("new-manifest-changed", localizer))?;
    verified
        .recheck(created)
        .map_err(|_| failure("new-source-changed", localizer))
}
