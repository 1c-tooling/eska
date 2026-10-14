//! The parent finalizes policy without owning cleanup of the existing root or child-published evidence.

use std::{fs, path::Path};

use super::{super::super::artifact::Failure, failure, policy::Plan};
use crate::{
    cli::{
        commands::{
            init::InitArgs,
            new::eska::protocol::{self, Created},
        },
        localization::Localizer,
    },
    project::{Project, init},
    project_core::publish,
    vcs::git,
};

/// Exact child controls and preserved preflight bytes precede parent workflow, Git or membership writes.
pub(super) fn finish(
    plan: &Plan,
    args: &InitArgs,
    created: &mut Created,
    localizer: &Localizer,
) -> Result<Project, Failure> {
    plan.verify_workspace(localizer)?;
    plan.core
        .verify_published(
            &created.accepted,
            &created.ancestor,
            &created.manifest_sha256,
        )
        .map_err(|error| super::source_failure(&error, localizer))?;
    let (original, config) = protocol::manifest(created)
        .map_err(|_| failure("init-eska-manifest-changed", localizer))?;
    if let Some(enrollment) = &plan.enrollment {
        return init::enroll_initialized_member(enrollment, &plan.name, &original).map_err(
            |error| {
                Failure::new(
                    "init-eska-enrollment-failed",
                    super::super::present(&error, localizer),
                )
            },
        );
    }
    let workflow = plan
        .workflow
        .ok_or_else(|| failure("init-eska-child-response-invalid", localizer))?;
    let config = config.with_workflow(workflow);
    let updated = config
        .to_toml()
        .map_err(|_| failure("init-eska-finalization-failed", localizer))?;
    publish::publish_snapshot(
        &created.root,
        Path::new(crate::config::FILE_NAME),
        &original,
        &updated,
    )
    .map_err(|_| failure("init-eska-manifest-changed", localizer))?;
    created.manifest_sha256 = protocol::digest_bytes(updated.as_bytes());
    if !args.no_vcs {
        protocol::manifest(created)
            .map_err(|_| failure("init-eska-manifest-changed", localizer))?;
        if plan.git_present {
            if !git::exists(&created.root)
                .map_err(|_| failure("init-eska-git-invalid", localizer))?
            {
                return Err(failure("init-eska-git-invalid", localizer));
            }
        } else {
            let path = created.root.join(".git");
            fs::create_dir(&path).map_err(|_| failure("init-eska-git-failed", localizer))?;
            // The exclusive new .git is retained on failure; neither user inputs nor published controls
            // are recursively removed after a successful child acknowledgment.
            git::initialize_reserved(&path)
                .map_err(|_| failure("init-eska-git-failed", localizer))?;
        }
    }
    protocol::manifest(created).map_err(|_| failure("init-eska-manifest-changed", localizer))?;
    config
        .into_project(created.root.clone())
        .map_err(|_| failure("init-eska-finalization-failed", localizer))
}
