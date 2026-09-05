use std::{fs, path::Path};

use eska::{
    project::{Project, ProjectConfiguration, ProjectType, SourceFormat, switch},
    vcs::workflow::WorkflowPreset,
};

use crate::vcs::support::{commit, git, git_output, repository};

fn project(root: &Path) -> Project {
    fs::create_dir_all(root.join("src")).expect("create source directory");
    Project::new(
        root.to_owned(),
        root.join("src"),
        ProjectConfiguration::new(ProjectType::Configuration, SourceFormat::DesignerXml)
            .with_workflow(WorkflowPreset::Trunk),
    )
    .expect("valid project")
}

#[test]
fn switches_between_existing_task_and_base_without_fetching() {
    let root = repository();
    commit(&root.0, "initial.txt");
    git(&root.0, &["branch", "task/FI-34"]);
    git(&root.0, &["remote", "add", "origin", "missing.git"]);
    let project = project(&root.0);

    let task =
        switch::execute(&project, switch::SwitchTarget::Task("FI-34")).expect("switch to task");
    assert_eq!(task.task.as_deref(), Some("FI-34"));
    assert_eq!(task.branch, "task/FI-34");
    assert_eq!(current_branch(&root.0), b"task/FI-34");

    let base = switch::execute(&project, switch::SwitchTarget::Base).expect("switch to base");
    assert_eq!(base.task, None);
    assert_eq!(base.branch, "main");
    assert_eq!(current_branch(&root.0), b"main");
}

#[test]
fn rejects_changes_outside_a_nested_project_before_switching() {
    let root = repository();
    commit(&root.0, "initial.txt");
    git(&root.0, &["branch", "task/FI-34"]);
    let nested = root.0.join("nested");
    let project = project(&nested);
    fs::write(root.0.join("outside.txt"), "dirty\n").expect("write outside change");

    let error = switch::execute(&project, switch::SwitchTarget::Task("FI-34"))
        .expect_err("dirty repository must fail");

    assert!(matches!(
        error,
        switch::SwitchError::DirtyWorkspace { files: 1 }
    ));
    assert_eq!(current_branch(&root.0), b"main");
}

#[test]
fn reports_a_missing_task_branch_without_creating_it() {
    let root = repository();
    commit(&root.0, "initial.txt");
    let project = project(&root.0);

    let error = switch::execute(&project, switch::SwitchTarget::Task("FI-404"))
        .expect_err("missing task must fail");

    assert!(matches!(
        error,
        switch::SwitchError::TaskBranchMissing { branch } if branch == "task/FI-404"
    ));
    assert_eq!(
        git_output(
            &root.0,
            &["show-ref", "--verify", "--quiet", "refs/heads/task/FI-404"]
        )
        .status
        .code(),
        Some(1)
    );
}

fn current_branch(root: &Path) -> Vec<u8> {
    git(root, &["rev-parse", "--abbrev-ref", "HEAD"])
        .trim_ascii()
        .to_vec()
}
