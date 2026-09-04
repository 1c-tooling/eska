use std::{fs, path::PathBuf};

use eska::project::{discovery, save};

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

    let project = discovery::discover(&fixture.0).unwrap();
    let result = save::execute(&project, Some("initial")).unwrap();

    assert_eq!(result.files, 2);
    assert_eq!(git(&fixture.0, &["log", "-1", "--format=%s"]), b"initial\n");
}
