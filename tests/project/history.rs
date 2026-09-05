use std::{fs, path::Path};

use eska::{
    project::{Project, ProjectConfiguration, ProjectType, SourceFormat, history},
    vcs::workflow::WorkflowPreset,
};

use crate::vcs::support::{commit, git, repository};

fn project(root: &Path, workflow: Option<WorkflowPreset>) -> Project {
    fs::create_dir_all(root.join("src")).expect("create source directory");
    let configuration =
        ProjectConfiguration::new(ProjectType::Configuration, SourceFormat::DesignerXml);
    let configuration = match workflow {
        Some(workflow) => configuration.with_workflow(workflow),
        None => configuration,
    };
    Project::new(root.to_owned(), root.join("src"), configuration).expect("valid project")
}

#[test]
fn attributes_only_commits_unique_to_one_matching_task_branch() {
    let root = repository();
    let base = commit(&root.0, "base.txt");
    git(&root.0, &["checkout", "-b", "task/FI-1"]);
    let task = commit(&root.0, "task.txt");

    let history =
        history::inspect(&project(&root.0, Some(WorkflowPreset::Trunk)), 10).expect("read history");

    assert_eq!(history.len(), 2);
    assert_eq!(history[0].commit.id, task);
    assert_eq!(history[0].task.as_deref(), Some("FI-1"));
    assert_eq!(history[1].commit.id, base);
    assert_eq!(history[1].task, None);
}

#[test]
fn omits_ambiguous_task_and_task_without_a_configured_workflow() {
    let root = repository();
    commit(&root.0, "base.txt");
    git(&root.0, &["checkout", "-b", "task/FI-1"]);
    let task = commit(&root.0, "task.txt");
    git(&root.0, &["branch", "task/FI-2", &task.to_string()]);

    let ambiguous = history::inspect(&project(&root.0, Some(WorkflowPreset::Trunk)), 1)
        .expect("read ambiguous history");
    assert_eq!(ambiguous[0].task, None);

    let unconfigured =
        history::inspect(&project(&root.0, None), 1).expect("read unconfigured history");
    assert_eq!(unconfigured[0].task, None);
}

#[test]
fn merged_commit_is_base_history_instead_of_task_history() {
    let root = repository();
    commit(&root.0, "base.txt");
    git(&root.0, &["checkout", "-b", "task/FI-1"]);
    let task = commit(&root.0, "task.txt");
    git(&root.0, &["checkout", "main"]);
    git(&root.0, &["merge", "--ff-only", "task/FI-1"]);

    let history = history::inspect(&project(&root.0, Some(WorkflowPreset::Trunk)), 1)
        .expect("read merged history");

    assert_eq!(history[0].commit.id, task);
    assert_eq!(history[0].task, None);
}
