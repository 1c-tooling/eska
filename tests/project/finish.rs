use std::{fs, path::Path};

use eska::{
    project::{Project, ProjectConfiguration, ProjectType, SourceFormat, finish},
    vcs::workflow::{
        FinishRequirement, PolicyOverrides, PublishBehavior, SyncStrategy, WorkflowPreset,
        WorkflowSettings, WorkingBranchPolicy,
    },
};

use crate::{
    support::TestDir,
    vcs::support::{commit, git, git_output, repository},
};

/// Construct a project with the supplied validated workflow settings.
fn project(root: &Path, workflow: WorkflowSettings) -> Project {
    fs::create_dir_all(root.join("src")).expect("create source directory");
    Project::new(
        root.to_owned(),
        root.join("src"),
        ProjectConfiguration::new(ProjectType::Configuration, SourceFormat::DesignerXml)
            .with_workflow_settings(workflow),
    )
    .expect("valid project")
}

/// Create and attach a bare origin fixture.
fn remote(root: &Path) -> TestDir {
    let remote = TestDir::new();
    git(
        &remote.0,
        &["init", "--bare", "--initial-branch=main", "--template="],
    );
    git(
        root,
        &[
            "remote",
            "add",
            "origin",
            remote.0.to_str().expect("UTF-8 fixture path"),
        ],
    );
    remote
}

/// Return whether a direct local branch exists.
fn branch_exists(root: &Path, branch: &str) -> bool {
    git_output(
        root,
        &[
            "show-ref",
            "--verify",
            "--quiet",
            &format!("refs/heads/{branch}"),
        ],
    )
    .status
    .success()
}

#[test]
/// Fetch remote integration, update base and remove the verified local task branch.
fn fetches_integrated_remote_base_then_switches_and_deletes_task_branch() {
    let root = repository();
    let initial = commit(&root.0, "initial.txt");
    let remote = remote(&root.0);
    git(&root.0, &["push", "origin", "main"]);
    git(&root.0, &["switch", "-c", "task/FI-40"]);
    let task_tip = commit(&root.0, "task.txt");
    git(&root.0, &["push", "origin", "task/FI-40"]);
    git(&root.0, &["switch", "main"]);
    git(&root.0, &["merge", "--ff-only", "task/FI-40"]);
    git(&root.0, &["push", "origin", "main"]);
    git(&root.0, &["switch", "task/FI-40"]);
    git(
        &root.0,
        &["branch", "--force", "main", &initial.to_string()],
    );

    let result = finish::execute(&project(
        &root.0,
        WorkflowSettings::selection(WorkflowPreset::Trunk),
    ))
    .expect("finish integrated task");

    assert_eq!(result.task, "FI-40");
    assert!(result.base_updated);
    assert!(result.branch_deleted);
    assert_eq!(result.remote.as_deref(), Some("origin"));
    assert_eq!(
        git(&root.0, &["rev-parse", "--abbrev-ref", "HEAD"]).trim_ascii(),
        b"main"
    );
    assert_eq!(
        git(&root.0, &["rev-parse", "refs/heads/main"]).trim_ascii(),
        task_tip.to_string().as_bytes()
    );
    assert!(!branch_exists(&root.0, "task/FI-40"));
    drop(remote);
}

#[test]
/// Use the local integration target when the policy remote is not configured.
fn finishes_against_local_integration_target_without_a_remote() {
    let root = repository();
    commit(&root.0, "initial.txt");
    git(&root.0, &["switch", "-c", "task/LOCAL-40"]);
    commit(&root.0, "task.txt");
    git(&root.0, &["switch", "main"]);
    git(&root.0, &["merge", "--ff-only", "task/LOCAL-40"]);
    git(&root.0, &["switch", "task/LOCAL-40"]);

    let result = finish::execute(&project(
        &root.0,
        WorkflowSettings::selection(WorkflowPreset::Trunk),
    ))
    .expect("finish local task");

    assert_eq!(result.remote, None);
    assert!(!result.base_updated);
    assert!(!branch_exists(&root.0, "task/LOCAL-40"));
}

#[test]
/// Preserve a published task branch when local deletion is disabled by policy.
fn published_requirement_preserves_branch_when_policy_disables_cleanup() {
    let root = repository();
    commit(&root.0, "initial.txt");
    let remote = remote(&root.0);
    git(&root.0, &["push", "origin", "main"]);
    git(&root.0, &["switch", "-c", "task/PUBLISHED-40"]);
    commit(&root.0, "task.txt");
    git(&root.0, &["push", "origin", "task/PUBLISHED-40"]);
    let settings = WorkflowSettings::new(
        WorkflowPreset::Custom,
        None,
        PolicyOverrides {
            base_branch: Some("main".into()),
            working_branch: Some(WorkingBranchPolicy::TaskBranch),
            task_branch_template: Some("task/{task}".into()),
            remote: Some("origin".into()),
            sync_strategy: Some(SyncStrategy::Rebase),
            integration_target: Some("main".into()),
            publish: Some(PublishBehavior::PushTaskBranch),
            finish: Some(FinishRequirement::Published),
            delete_local_branch: Some(false),
        },
    )
    .expect("valid published-only workflow");

    let result = finish::execute(&project(&root.0, settings)).expect("finish published task");

    assert!(!result.branch_deleted);
    assert!(branch_exists(&root.0, "task/PUBLISHED-40"));
    assert_eq!(
        git(&root.0, &["rev-parse", "--abbrev-ref", "HEAD"]).trim_ascii(),
        b"main"
    );
    drop(remote);
}

#[test]
/// Leave an unintegrated task active and intact.
fn rejects_unintegrated_task_without_switching_or_deleting_it() {
    let root = repository();
    commit(&root.0, "initial.txt");
    git(&root.0, &["switch", "-c", "task/FI-UNMERGED"]);
    commit(&root.0, "task.txt");

    let error = finish::execute(&project(
        &root.0,
        WorkflowSettings::selection(WorkflowPreset::Trunk),
    ))
    .expect_err("unintegrated task must fail");

    assert!(matches!(error, finish::FinishError::NotIntegrated { .. }));
    assert_eq!(
        git(&root.0, &["rev-parse", "--abbrev-ref", "HEAD"]).trim_ascii(),
        b"task/FI-UNMERGED"
    );
    assert!(branch_exists(&root.0, "task/FI-UNMERGED"));
}

#[test]
/// Use the fetched remote target rather than a stale local integration assumption.
fn rejects_task_not_integrated_into_remote_target() {
    let root = repository();
    commit(&root.0, "initial.txt");
    let remote = remote(&root.0);
    git(&root.0, &["push", "origin", "main"]);
    git(&root.0, &["switch", "-c", "task/FI-REMOTE"]);
    commit(&root.0, "task.txt");

    let error = finish::execute(&project(
        &root.0,
        WorkflowSettings::selection(WorkflowPreset::Trunk),
    ))
    .expect_err("remote integration target does not contain task");

    assert!(matches!(
        error,
        finish::FinishError::NotIntegrated { reference }
            if reference == "refs/remotes/origin/main"
    ),);
    assert!(branch_exists(&root.0, "task/FI-REMOTE"));
    drop(remote);
}

#[test]
/// Refuse a diverged base before checking completion or switching branches.
fn rejects_a_base_diverged_from_remote() {
    let root = repository();
    let initial = commit(&root.0, "initial.txt");
    let remote = remote(&root.0);
    git(&root.0, &["push", "origin", "main"]);
    commit(&root.0, "remote.txt");
    git(&root.0, &["push", "origin", "main"]);
    git(&root.0, &["reset", "--hard", &initial.to_string()]);
    commit(&root.0, "local.txt");
    git(&root.0, &["switch", "-c", "task/FI-DIVERGED"]);

    let error = finish::execute(&project(
        &root.0,
        WorkflowSettings::selection(WorkflowPreset::Trunk),
    ))
    .expect_err("diverged base must fail");

    assert!(matches!(error, finish::FinishError::BaseDiverged { .. }));
    assert_eq!(
        git(&root.0, &["rev-parse", "--abbrev-ref", "HEAD"]).trim_ascii(),
        b"task/FI-DIVERGED"
    );
    drop(remote);
}

#[test]
/// Preserve a task when the configured remote cannot be fetched.
fn reports_an_inaccessible_remote_without_switching() {
    let root = repository();
    commit(&root.0, "initial.txt");
    git(&root.0, &["switch", "-c", "task/FI-OFFLINE"]);
    let missing = root.0.join("missing-remote.git");
    git(
        &root.0,
        &[
            "remote",
            "add",
            "origin",
            missing.to_str().expect("UTF-8 fixture path"),
        ],
    );

    let error = finish::execute(&project(
        &root.0,
        WorkflowSettings::selection(WorkflowPreset::Trunk),
    ))
    .expect_err("inaccessible remote must fail");

    assert!(
        matches!(
            &error,
            finish::FinishError::Fetch { remote, url, reason }
                if remote == "origin"
                    && url == &missing.to_string_lossy()
                    && reason.contains("valid git directory")
        ),
        "{error:?}"
    );
    assert_eq!(
        git(&root.0, &["rev-parse", "--abbrev-ref", "HEAD"]).trim_ascii(),
        b"task/FI-OFFLINE"
    );
}

#[test]
/// Inspect the complete repository worktree before network or cleanup actions.
fn rejects_dirty_files_outside_a_nested_project_before_fetch() {
    let root = repository();
    commit(&root.0, "initial.txt");
    git(&root.0, &["switch", "-c", "task/FI-DIRTY"]);
    let nested = root.0.join("nested");
    let project = project(&nested, WorkflowSettings::selection(WorkflowPreset::Trunk));
    fs::write(root.0.join("outside.txt"), "dirty\n").expect("write outside change");

    let error = finish::execute(&project).expect_err("dirty repository must fail");

    assert!(matches!(
        error,
        finish::FinishError::DirtyWorkspace { files: 1 }
    ));
    assert_eq!(
        git(&root.0, &["rev-parse", "--abbrev-ref", "HEAD"]).trim_ascii(),
        b"task/FI-DIRTY"
    );
}

#[test]
/// Detect sequenced Git state even when no file-level changes are present.
fn rejects_an_in_progress_git_operation_even_when_the_index_is_clean() {
    let root = repository();
    let tip = commit(&root.0, "initial.txt");
    git(&root.0, &["switch", "-c", "task/FI-MERGE"]);
    fs::write(root.0.join(".git/MERGE_HEAD"), format!("{tip}\n")).expect("mark merge in progress");

    let error = finish::execute(&project(
        &root.0,
        WorkflowSettings::selection(WorkflowPreset::Trunk),
    ))
    .expect_err("operation in progress must fail");

    assert!(matches!(error, finish::FinishError::OperationInProgress));
}

#[test]
/// Require an active branch that exactly matches the policy task template.
fn rejects_a_non_task_active_branch() {
    let root = repository();
    commit(&root.0, "initial.txt");

    let error = finish::execute(&project(
        &root.0,
        WorkflowSettings::selection(WorkflowPreset::Trunk),
    ))
    .expect_err("base branch must not finish");

    assert!(matches!(error, finish::FinishError::NotTaskBranch));
}
