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
fn preserves_changes_outside_a_nested_project() {
    let root = repository();
    commit(&root.0, "initial.txt");
    git(&root.0, &["branch", "task/FI-34"]);
    let project = project(&root.0.join("nested"));
    fs::write(root.0.join("outside.txt"), "dirty\n").expect("write outside change");
    let result =
        switch::execute(&project, switch::SwitchTarget::Task("FI-34")).expect("save and switch");
    assert!(result.saved.is_some());
    assert!(!root.0.join("outside.txt").exists());
    let result = switch::execute(&project, switch::SwitchTarget::Base).expect("switch and restore");
    assert!(result.restored.is_some());
    assert_eq!(fs::read(root.0.join("outside.txt")).unwrap(), b"dirty\n");
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

/// Two dirty tasks keep independent index and worktree contents across repeated switches.
#[test]
fn round_trip_between_two_dirty_tasks_restores_exact_indexes() {
    let root = repository();
    commit(&root.0, "file");
    git(&root.0, &["branch", "task/A"]);
    git(&root.0, &["branch", "task/B"]);
    let project = project(&root.0);
    switch::execute(&project, switch::SwitchTarget::Task("A")).unwrap();
    fs::write(root.0.join("file"), b"A staged\r\n").unwrap();
    git(&root.0, &["add", "file"]);
    fs::write(root.0.join("file"), b"A unstaged\r\n").unwrap();
    fs::write(root.0.join("A new"), b"A").unwrap();
    let a_index = fs::read(root.0.join(".git/index")).unwrap();
    let transition = switch::execute(&project, switch::SwitchTarget::Task("B")).unwrap();
    assert!(transition.saved.is_some());
    assert!(transition.restored.is_none());
    assert_eq!(fs::read(root.0.join("file")).unwrap(), b"file\n");
    assert!(!root.0.join("A new").exists());
    fs::write(root.0.join("file"), b"B staged").unwrap();
    git(&root.0, &["add", "file"]);
    fs::remove_file(root.0.join("file")).unwrap();
    let b_index = fs::read(root.0.join(".git/index")).unwrap();
    let transition = switch::execute(&project, switch::SwitchTarget::Task("A")).unwrap();
    assert!(transition.saved.is_some());
    assert!(transition.restored.is_some());
    assert_eq!(fs::read(root.0.join(".git/index")).unwrap(), a_index);
    assert_eq!(fs::read(root.0.join("file")).unwrap(), b"A unstaged\r\n");
    assert_eq!(fs::read(root.0.join("A new")).unwrap(), b"A");
    switch::execute(&project, switch::SwitchTarget::Task("B")).unwrap();
    assert_eq!(fs::read(root.0.join(".git/index")).unwrap(), b_index);
    assert!(!root.0.join("file").exists());
}

/// A moved or corrupt target shelf fails before capturing the source branch.
#[test]
fn invalid_target_shelf_preserves_the_dirty_source() {
    use eska::vcs::{repository::Repository, shelves};
    for corrupt in [false, true] {
        let root = repository();
        commit(&root.0, "initial");
        git(&root.0, &["branch", "task/A"]);
        let project = project(&root.0);
        switch::execute(&project, switch::SwitchTarget::Task("A")).unwrap();
        fs::write(root.0.join("A"), b"target").unwrap();
        switch::execute(&project, switch::SwitchTarget::Base).unwrap();
        let repo = Repository::discover(&root.0).unwrap();
        let saved = shelves::list(&repo).unwrap().remove(0);
        if corrupt {
            fs::write(
                root.0.join(".git/eska-shelves").join(&saved.id).join("0"),
                b"broken",
            )
            .unwrap();
        } else {
            let next = commit(&root.0, "new");
            git(
                &root.0,
                &["update-ref", "refs/heads/task/A", &next.to_string()],
            );
        }
        fs::write(root.0.join("source"), b"keep").unwrap();
        let index = fs::read(repo.index_path()).unwrap();
        assert!(matches!(
            switch::execute(&project, switch::SwitchTarget::Task("A")),
            Err(switch::SwitchError::Shelf(_))
        ));
        assert_eq!(current_branch(&root.0), b"main");
        assert_eq!(fs::read(root.0.join("source")).unwrap(), b"keep");
        assert_eq!(fs::read(repo.index_path()).unwrap(), index);
        assert_eq!(shelves::list(&repo).unwrap(), vec![saved]);
    }
}

/// Checkout refuses ignored collisions and restores the captured source state.
#[test]
fn ignored_checkout_collision_keeps_both_file_contents() {
    let root = repository();
    commit(&root.0, "initial");
    git(&root.0, &["checkout", "-b", "task/A"]);
    commit(&root.0, "collision");
    git(&root.0, &["checkout", "main"]);
    fs::create_dir_all(root.0.join(".git/info")).unwrap();
    fs::write(root.0.join(".git/info/exclude"), b"collision\n").unwrap();
    fs::write(root.0.join("collision"), b"local ignored").unwrap();
    fs::write(root.0.join("pending"), b"source pending").unwrap();
    let project = project(&root.0);
    assert!(matches!(
        switch::execute(&project, switch::SwitchTarget::Task("A")),
        Err(switch::SwitchError::Command(_))
    ));
    assert_eq!(current_branch(&root.0), b"main");
    assert_eq!(
        fs::read(root.0.join("collision")).unwrap(),
        b"local ignored"
    );
    assert_eq!(fs::read(root.0.join("pending")).unwrap(), b"source pending");
    assert_eq!(git(&root.0, &["show", "task/A:collision"]), b"collision\n");
}

/// A linked worktree owns its checked-out branch even when the source is dirty.
#[test]
fn target_checked_out_elsewhere_is_rejected_before_capture() {
    let root = repository();
    commit(&root.0, "initial");
    git(&root.0, &["branch", "task/A"]);
    let linked = root.0.join("linked");
    git(
        &root.0,
        &["worktree", "add", linked.to_str().unwrap(), "task/A"],
    );
    let project = project(&root.0);
    fs::write(root.0.join("pending"), b"source pending").unwrap();
    assert!(matches!(
        switch::execute(&project, switch::SwitchTarget::Task("A")),
        Err(switch::SwitchError::TargetCheckedOut { .. })
    ));
    assert_eq!(current_branch(&root.0), b"main");
    assert_eq!(fs::read(root.0.join("pending")).unwrap(), b"source pending");
    assert!(!root.0.join(".git/eska-shelves").exists());
}
