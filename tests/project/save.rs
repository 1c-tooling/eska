use std::{fs, path::PathBuf};

use eska::{
    project::{discovery, save},
    vcs::status::Change,
};
use gix::bstr::ByteSlice;

use crate::{
    support::TestDir,
    vcs::support::{git, repository},
};

fn nested_project() -> (TestDir, PathBuf) {
    let fixture = repository();
    let root = fixture.0.join("workspace/Billing");
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(
        root.join("eska.toml"),
        "[project]\ntype = 'configuration'\n",
    )
    .unwrap();
    fs::write(root.join("src/module.bsl"), "base\n").unwrap();
    fs::write(root.join("src/deleted.bsl"), "base\n").unwrap();
    fs::write(root.join(".gitignore"), "src/ignored.bsl\n").unwrap();
    fs::write(fixture.0.join("outside.txt"), "base\n").unwrap();
    git(&fixture.0, &["config", "user.name", "Eska Test"]);
    git(
        &fixture.0,
        &["config", "user.email", "eska@example.invalid"],
    );
    git(&fixture.0, &["add", "."]);
    git(&fixture.0, &["commit", "-m", "base"]);
    (fixture, root)
}

/// The read-only plan exposes the exact subsequent commit scope and preserves repository bytes.
#[test]
fn plan_matches_save_scope_without_mutating_repository() {
    let (fixture, root) = nested_project();
    fs::write(fixture.0.join("outside.txt"), "staged outside\n").unwrap();
    git(&fixture.0, &["add", "outside.txt"]);
    fs::write(root.join("src/module.bsl"), "staged project\n").unwrap();
    git(&fixture.0, &["add", "workspace/Billing/src/module.bsl"]);
    fs::write(root.join("src/module.bsl"), "worktree project\n").unwrap();
    fs::remove_file(root.join("src/deleted.bsl")).unwrap();
    fs::write(root.join("src/new.bsl"), "new\n").unwrap();
    fs::write(root.join("src/ignored.bsl"), "ignored\n").unwrap();

    let index_path = fixture.0.join(".git/index");
    let head_path = fixture.0.join(".git/HEAD");
    let branch_path = fixture.0.join(".git/refs/heads/main");
    let index_before = fs::read(&index_path).unwrap();
    let head_before = fs::read(&head_path).unwrap();
    let branch_before = fs::read(&branch_path).unwrap();
    let status_before = git(&fixture.0, &["status", "--short"]);
    let worktree_before = fs::read(root.join("src/module.bsl")).unwrap();

    let project = discovery::discover(&root).unwrap();
    let plan = save::plan(&project).unwrap();

    let files = plan
        .files()
        .iter()
        .map(|file| (file.path().to_owned(), file.index(), file.worktree()))
        .collect::<Vec<_>>();
    assert_eq!(
        files,
        vec![
            (
                b"src/deleted.bsl".as_bstr().to_owned(),
                None,
                Some(Change::Deleted)
            ),
            (
                b"src/module.bsl".as_bstr().to_owned(),
                Some(Change::Modified),
                Some(Change::Modified)
            ),
            (
                b"src/new.bsl".as_bstr().to_owned(),
                None,
                Some(Change::Untracked)
            ),
        ]
    );
    assert_eq!(fs::read(&index_path).unwrap(), index_before);
    assert_eq!(fs::read(&head_path).unwrap(), head_before);
    assert_eq!(fs::read(&branch_path).unwrap(), branch_before);
    assert_eq!(git(&fixture.0, &["status", "--short"]), status_before);
    assert_eq!(
        fs::read(root.join("src/module.bsl")).unwrap(),
        worktree_before
    );

    save::execute(&project, Some("save planned scope")).unwrap();
    assert_eq!(
        git(&fixture.0, &["show", "--format=", "--name-only", "HEAD"]),
        concat!(
            "workspace/Billing/src/deleted.bsl\n",
            "workspace/Billing/src/module.bsl\n",
            "workspace/Billing/src/new.bsl\n"
        )
        .as_bytes()
    );
    assert_eq!(git(&fixture.0, &["status", "--short"]), b"M  outside.txt\n");
}

/// Saving a nested project includes its worktree state and preserves sibling staging.
#[test]
fn saves_only_project_changes_and_preserves_sibling_staging() {
    let (fixture, root) = nested_project();
    fs::write(fixture.0.join("outside.txt"), "staged outside\n").unwrap();
    git(&fixture.0, &["add", "outside.txt"]);
    fs::write(root.join("src/module.bsl"), "staged project\n").unwrap();
    git(&fixture.0, &["add", "workspace/Billing/src/module.bsl"]);
    fs::write(root.join("src/module.bsl"), "saved worktree\n").unwrap();
    fs::write(root.join("src/new.bsl"), "new\n").unwrap();

    let project = discovery::discover(&root).unwrap();
    let result = save::execute(&project, Some("save project")).unwrap();

    assert_eq!(result.files, 2);
    assert_eq!(
        git(&fixture.0, &["show", "--format=", "--name-only", "HEAD"]),
        b"workspace/Billing/src/module.bsl\nworkspace/Billing/src/new.bsl\n"
    );
    assert_eq!(
        git(
            &fixture.0,
            &["show", "HEAD:workspace/Billing/src/module.bsl"]
        ),
        b"saved worktree\n"
    );
    assert_eq!(git(&fixture.0, &["status", "--short"]), b"M  outside.txt\n");
}

/// An unborn repository can save its first project `ChangeSet`.
#[test]
fn creates_the_first_commit() {
    let fixture = repository();
    fs::create_dir_all(fixture.0.join("src")).unwrap();
    fs::write(
        fixture.0.join("eska.toml"),
        "[project]\ntype = 'configuration'\n",
    )
    .unwrap();
    fs::write(fixture.0.join("src/module.bsl"), "initial\n").unwrap();
    git(&fixture.0, &["config", "user.name", "Eska Test"]);
    git(
        &fixture.0,
        &["config", "user.email", "eska@example.invalid"],
    );

    let head_path = fixture.0.join(".git/HEAD");
    let head_before = fs::read(&head_path).unwrap();
    let project = discovery::discover(&fixture.0).unwrap();
    let plan = save::plan(&project).unwrap();
    assert_eq!(plan.files().len(), 2);
    assert_eq!(fs::read(&head_path).unwrap(), head_before);
    assert!(!fixture.0.join(".git/index").exists());

    let result = save::execute(&project, Some("initial")).unwrap();

    assert_eq!(result.files, 2);
    assert_eq!(git(&fixture.0, &["log", "-1", "--format=%s"]), b"initial\n");
}
