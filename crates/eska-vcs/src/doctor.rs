//! Read-only Git and workflow prerequisites owned by the VCS component.

use crate::vcs::{command::Executor, repository::Repository, workflow::WorkflowPolicy};
use eska_project_core::{
    doctor::{Check, CheckStatus},
    project::Project,
};
use std::{
    collections::BTreeSet,
    fs, io,
    path::{Path, PathBuf},
};

/// Inspect repository, workflow and system Git capabilities without network access.
#[must_use]
pub fn inspect_vcs(
    scope_root: &Path,
    project: &Project,
    attribute_roots: &[PathBuf],
) -> Vec<Check> {
    let mut checks = Vec::new();
    let repository = Repository::discover(scope_root);
    checks.push(match &repository {
        Ok(repository) if project.root().starts_with(repository.work_dir()) => Check::new(
            "vcs.repository",
            CheckStatus::Pass,
            "available",
            &["status", "start", "save", "switch", "finish"],
        ),
        Ok(_) => Check::new(
            "vcs.repository",
            CheckStatus::Fail,
            "project-outside-repository",
            &["status", "start", "save", "switch", "finish"],
        )
        .with_remediation("move-project-into-repository"),
        Err(_) => Check::new(
            "vcs.repository",
            CheckStatus::Fail,
            "not-found",
            &["status", "start", "save", "switch", "finish"],
        )
        .with_remediation("initialize-git-repository"),
    });

    let policy = project
        .configuration()
        .workflow_settings()
        .map(|settings| settings.resolve(None));
    checks.push(match &policy {
        Some(Ok(_)) => Check::new(
            "vcs.workflow",
            CheckStatus::Pass,
            "configured",
            &["start", "switch", "finish", "patch"],
        ),
        Some(Err(_)) => Check::new(
            "vcs.workflow",
            CheckStatus::Fail,
            "invalid",
            &["start", "switch", "finish", "patch"],
        )
        .with_remediation("correct-workflow-policy"),
        None => Check::new(
            "vcs.workflow",
            CheckStatus::Warning,
            "not-configured",
            &["start", "switch", "finish", "patch"],
        )
        .with_remediation("configure-workflow"),
    });
    checks.extend(inspect_repository_state(
        repository.as_ref().ok(),
        policy.as_ref().and_then(|policy| policy.as_ref().ok()),
    ));
    checks.extend(inspect_system_git(scope_root, attribute_roots));
    checks
}

/// Inspect repository state and the configured remote without contacting it.
fn inspect_repository_state(
    repository: Option<&Repository>,
    policy: Option<&WorkflowPolicy>,
) -> Vec<Check> {
    let Some(repository) = repository else {
        return vec![
            Check::new(
                "vcs.operation",
                CheckStatus::Skipped,
                "repository-required",
                &["start", "switch", "finish", "patch"],
            ),
            Check::new(
                "vcs.remote",
                CheckStatus::Skipped,
                "repository-required",
                &["start", "finish"],
            ),
        ];
    };
    let operation = if repository.has_in_progress_operation() {
        Check::new(
            "vcs.operation",
            CheckStatus::Fail,
            "in-progress",
            &["start", "switch", "finish", "patch"],
        )
        .with_remediation("finish-git-operation")
    } else {
        Check::new(
            "vcs.operation",
            CheckStatus::Pass,
            "clean",
            &["start", "switch", "finish", "patch"],
        )
    };
    let remote = policy.map_or_else(
        || {
            Check::new(
                "vcs.remote",
                CheckStatus::Skipped,
                "workflow-required",
                &["start", "finish"],
            )
        },
        |policy| match repository.remote(policy.remote()) {
            Ok(Some(_)) => Check::new(
                "vcs.remote",
                CheckStatus::Pass,
                "configured",
                &["start", "finish"],
            ),
            Ok(None) => Check::new(
                "vcs.remote",
                CheckStatus::Pass,
                "not-configured",
                &["start", "finish"],
            ),
            Err(_) => Check::new(
                "vcs.remote",
                CheckStatus::Fail,
                "invalid",
                &["start", "finish"],
            )
            .with_remediation("correct-remote"),
        },
    );
    vec![operation, remote]
}

/// Inspect system Git, author identity and conditional Git LFS availability.
#[must_use]
pub fn inspect_system_git(scope_root: &Path, attribute_roots: &[PathBuf]) -> Vec<Check> {
    let mut checks = Vec::new();
    let mut attribute_roots = attribute_roots.to_vec();
    if let Ok(repository) = Repository::discover(scope_root) {
        attribute_roots.push(repository.work_dir().to_path_buf());
    }

    let git = Executor::new(scope_root);
    let git_available = git.git_available();
    checks.push(if git_available {
        Check::new(
            "vcs.git",
            CheckStatus::Pass,
            "available",
            &["save", "switch", "finish"],
        )
    } else {
        Check::new(
            "vcs.git",
            CheckStatus::Fail,
            "not-found",
            &["save", "switch", "finish"],
        )
        .with_remediation("install-git")
    });
    checks.push(if !git_available {
        Check::new(
            "vcs.author",
            CheckStatus::Skipped,
            "git-required",
            &["save"],
        )
    } else if git.author_configured() {
        Check::new("vcs.author", CheckStatus::Pass, "configured", &["save"])
    } else {
        Check::new("vcs.author", CheckStatus::Fail, "not-configured", &["save"])
            .with_remediation("configure-git-author")
    });

    match lfs_required(&attribute_roots) {
        Ok(false) => checks.push(Check::new(
            "vcs.git-lfs",
            CheckStatus::Skipped,
            "not-required",
            &["save"],
        )),
        Ok(true) if !git_available => checks.push(Check::new(
            "vcs.git-lfs",
            CheckStatus::Skipped,
            "git-required",
            &["save"],
        )),
        Ok(true) if git.git_lfs_available() => checks.push(Check::new(
            "vcs.git-lfs",
            CheckStatus::Pass,
            "available",
            &["save"],
        )),
        Ok(true) => checks.push(
            Check::new("vcs.git-lfs", CheckStatus::Fail, "not-found", &["save"])
                .with_remediation("install-git-lfs"),
        ),
        Err(_) => checks.push(
            Check::new(
                "vcs.git-lfs",
                CheckStatus::Fail,
                "attributes-unreadable",
                &["save"],
            )
            .with_remediation("check-git-attributes"),
        ),
    }
    checks
}

/// Check whether any applicable attributes file selects the Git LFS filter.
fn lfs_required(roots: &[PathBuf]) -> Result<bool, io::Error> {
    let mut visited = BTreeSet::new();
    for root in roots {
        let path = root.join(".gitattributes");
        if !visited.insert(path.clone()) {
            continue;
        }
        match fs::read(&path) {
            Ok(contents)
                if contents
                    .windows(b"filter=lfs".len())
                    .any(|part| part == b"filter=lfs") =>
            {
                return Ok(true);
            }
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
    }
    Ok(false)
}
