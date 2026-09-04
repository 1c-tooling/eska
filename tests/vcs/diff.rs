use std::fs;

use eska::vcs::{repository::Repository, status::Change};

use super::support::{commit, git, repository};

/// Revision resolution peels tags and tree diff retains paths, states and blob snapshots.
#[test]
fn revisions_compare_committed_trees_without_reading_the_worktree() {
    let dir = repository();
    fs::write(dir.0.join("tracked.txt"), "base\n").unwrap();
    git(&dir.0, &["add", "tracked.txt"]);
    git(&dir.0, &["commit", "-m", "base"]);
    git(&dir.0, &["tag", "-a", "baseline", "-m", "baseline"]);
    fs::write(dir.0.join("tracked.txt"), "committed\n").unwrap();
    fs::write(dir.0.join("added.txt"), "added\n").unwrap();
    git(&dir.0, &["add", "."]);
    git(&dir.0, &["commit", "-m", "changed"]);
    fs::write(dir.0.join("tracked.txt"), "worktree\n").unwrap();

    let repository = Repository::discover(&dir.0).unwrap();
    let baseline = repository.resolve_commit("baseline").unwrap();
    let head = repository.resolve_commit("HEAD").unwrap();
    let head_id = head.id.to_string();
    assert_eq!(repository.resolve_commit(&head_id).unwrap(), head);
    assert_eq!(repository.resolve_commit(&head_id[..8]).unwrap(), head);
    let changes = repository.diff_commits(baseline, head).unwrap();

    assert_eq!(
        changes
            .iter()
            .map(|entry| (entry.path.to_string(), entry.change))
            .collect::<Vec<_>>(),
        [
            ("added.txt".to_owned(), Change::Added),
            ("tracked.txt".to_owned(), Change::Modified),
        ]
    );
    let tracked = &changes[1];
    assert_eq!(repository.blob(tracked.before.unwrap()).unwrap(), b"base\n");
    assert_eq!(
        repository.blob(tracked.after.unwrap()).unwrap(),
        b"committed\n"
    );
}

/// Merge-base resolution selects the shared ancestor of diverged branches.
#[test]
fn merge_base_resolves_the_branch_point() {
    let dir = repository();
    let base = commit(&dir.0, "base.txt");
    git(&dir.0, &["checkout", "-b", "feature"]);
    commit(&dir.0, "feature.txt");
    git(&dir.0, &["checkout", "main"]);
    commit(&dir.0, "main.txt");

    let repository = Repository::discover(&dir.0).unwrap();
    let main = repository.resolve_commit("main").unwrap();
    let feature = repository.resolve_commit("feature").unwrap();
    assert_eq!(
        repository.merge_base_commit(main, feature).unwrap().id,
        base
    );
}
