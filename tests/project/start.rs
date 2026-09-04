use std::{fs, path::Path};

use eska::{
    project::{Project, ProjectConfiguration, ProjectType, SourceFormat, start},
    vcs::workflow::WorkflowPreset,
};

use crate::{
    support::TestDir,
    vcs::support::{commit, git, repository},
};

fn project(root: &Path, workflow: WorkflowPreset) -> Project {
    fs::create_dir_all(root.join("src")).expect("create source directory");
    Project::new(
        root.to_owned(),
        root.join("src"),
        ProjectConfiguration::new(ProjectType::Configuration, SourceFormat::DesignerXml)
            .with_workflow(workflow),
    )
    .expect("valid project")
}

fn remote(root: &Path) -> TestDir {
    let remote = TestDir::new();
    git(
        &remote.0,
        &["init", "--bare", "--initial-branch=main", "--template="],
    );
    let path = remote.0.to_str().expect("UTF-8 fixture path");
    git(root, &["remote", "add", "origin", path]);
    remote
}

#[test]
fn fetches_fast_forwards_base_and_switches_to_policy_branch() {
    let root = repository();
    let initial = commit(&root.0, "initial.txt");
    let remote = remote(&root.0);
    git(&root.0, &["push", "origin", "main"]);
    let remote_tip = commit(&root.0, "remote.txt");
    git(&root.0, &["push", "origin", "main"]);
    git(&root.0, &["reset", "--hard", &initial.to_string()]);

    let result =
        start::execute(&project(&root.0, WorkflowPreset::Trunk), "FI-1234").expect("start task");

    assert_eq!(result.task, "FI-1234");
    assert_eq!(result.branch, "task/FI-1234");
    assert_eq!(result.base_branch, "main");
    assert!(result.base_updated);
    assert_eq!(
        git(&root.0, &["rev-parse", "--abbrev-ref", "HEAD"]).trim_ascii(),
        b"task/FI-1234"
    );
    assert_eq!(
        git(&root.0, &["rev-parse", "refs/heads/main"]).trim_ascii(),
        remote_tip.to_string().as_bytes()
    );
    drop(remote);
}

#[test]
fn starts_from_local_base_when_policy_remote_is_not_configured() {
    let root = repository();
    let local_tip = commit(&root.0, "initial.txt");

    let result = start::execute(&project(&root.0, WorkflowPreset::Trunk), "LOCAL-1")
        .expect("start without remote");

    assert_eq!(result.remote, None);
    assert!(!result.base_updated);
    assert_eq!(result.branch, "task/LOCAL-1");
    assert_eq!(
        git(&root.0, &["rev-parse", "HEAD"]).trim_ascii(),
        local_tip.to_string().as_bytes()
    );
}

#[test]
fn configured_inaccessible_remote_reports_name_url_and_git_reason() {
    let root = repository();
    commit(&root.0, "initial.txt");
    let missing = root.0.join("missing-remote.git");
    git(
        &root.0,
        &[
            "remote",
            "add",
            "origin",
            missing.to_str().expect("UTF-8 path"),
        ],
    );

    let error = start::execute(&project(&root.0, WorkflowPreset::Trunk), "FI-9")
        .expect_err("inaccessible remote must fail");

    assert!(matches!(
        error,
        start::StartError::Fetch { remote, url, reason }
            if remote == "origin"
                && url == missing.to_string_lossy()
                && reason.contains("does not appear to be a git repository")
    ));
    assert_eq!(
        git(&root.0, &["rev-parse", "--abbrev-ref", "HEAD"]).trim_ascii(),
        b"main"
    );
}

#[test]
fn keeps_a_clean_local_base_that_is_ahead_of_remote() {
    let root = repository();
    commit(&root.0, "initial.txt");
    let remote = remote(&root.0);
    git(&root.0, &["push", "origin", "main"]);
    let local_tip = commit(&root.0, "local.txt");

    let result = start::execute(&project(&root.0, WorkflowPreset::GithubFlow), "FI-7")
        .expect("start from local base");

    assert!(!result.base_updated);
    assert_eq!(
        git(&root.0, &["rev-parse", "HEAD"]).trim_ascii(),
        local_tip.to_string().as_bytes()
    );
    drop(remote);
}

#[test]
fn reports_an_unchanged_base_when_local_and_remote_are_equal() {
    let root = repository();
    commit(&root.0, "initial.txt");
    let remote = remote(&root.0);
    git(&root.0, &["push", "origin", "main"]);

    let result = start::execute(&project(&root.0, WorkflowPreset::Trunk), "FI-8")
        .expect("start from equal base");

    assert!(!result.base_updated);
    drop(remote);
}

#[test]
fn rejects_dirty_worktree_before_running_network_operations() {
    let root = repository();
    commit(&root.0, "initial.txt");
    fs::write(root.0.join("dirty.txt"), "dirty\n").expect("write dirty file");

    let error = start::execute(&project(&root.0, WorkflowPreset::Trunk), "FI-1")
        .expect_err("dirty worktree must fail");

    assert!(matches!(
        error,
        start::StartError::DirtyWorkspace { files: 1 }
    ));
    assert_eq!(
        git(&root.0, &["rev-parse", "--abbrev-ref", "HEAD"]).trim_ascii(),
        b"main"
    );
}

#[test]
fn rejects_existing_task_branch_before_fetch() {
    let root = repository();
    commit(&root.0, "initial.txt");
    git(&root.0, &["branch", "task/FI-1"]);

    let error = start::execute(&project(&root.0, WorkflowPreset::Trunk), "FI-1")
        .expect_err("existing branch must fail");

    assert!(matches!(
        error,
        start::StartError::TaskBranchExists { branch } if branch == "task/FI-1"
    ));
}

#[test]
fn rejects_diverged_base_without_moving_refs_or_head() {
    let root = repository();
    let initial = commit(&root.0, "initial.txt");
    let remote = remote(&root.0);
    git(&root.0, &["push", "origin", "main"]);
    commit(&root.0, "remote.txt");
    git(&root.0, &["push", "origin", "main"]);
    git(&root.0, &["reset", "--hard", &initial.to_string()]);
    let local_tip = commit(&root.0, "local.txt");

    let error = start::execute(&project(&root.0, WorkflowPreset::Trunk), "FI-1")
        .expect_err("diverged base must fail");

    assert!(matches!(error, start::StartError::BaseDiverged { .. }));
    assert_eq!(
        git(&root.0, &["rev-parse", "HEAD"]).trim_ascii(),
        local_tip.to_string().as_bytes()
    );
    let branch_check = crate::vcs::support::git_output(
        &root.0,
        &["show-ref", "--verify", "--quiet", "refs/heads/task/FI-1"],
    );
    assert_eq!(branch_check.status.code(), Some(1));
    drop(remote);
}
