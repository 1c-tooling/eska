use std::fs;

use eska::vcs::repository::{Error, Head, OpenError, ReferenceTarget, Repository};

use super::support::{commit, git, repository};
use crate::support::TestDir;

#[test]
fn discovery_and_unborn_head() {
    let dir = repository();
    fs::create_dir_all(dir.0.join("src/nested")).unwrap();
    let repo = Repository::discover(&dir.0.join("src/nested")).unwrap();
    assert_eq!(repo.work_dir(), dir.0);
    assert_eq!(repo.git_dir(), dir.0.join(".git"));
    assert_eq!(
        repo.head().unwrap(),
        Head::Unborn {
            reference: "refs/heads/main".into()
        }
    );
    assert!(repo.references().unwrap().is_empty());
    assert!(repo.history(10).unwrap().is_empty());
}

#[test]
fn attached_detached_and_packed_references() {
    let dir = repository();
    let id = commit(&dir.0, "первый файл");
    git(&dir.0, &["branch", "feature/task"]);
    git(&dir.0, &["tag", "light"]);
    git(&dir.0, &["tag", "-a", "annotated", "-m", "tag"]);
    git(
        &dir.0,
        &["update-ref", "refs/remotes/origin/main", &id.to_string()],
    );
    git(
        &dir.0,
        &[
            "symbolic-ref",
            "refs/remotes/origin/HEAD",
            "refs/remotes/origin/main",
        ],
    );
    git(&dir.0, &["pack-refs", "--all"]);
    let repo = Repository::discover(&dir.0).unwrap();
    assert_eq!(
        repo.head().unwrap(),
        Head::Attached {
            reference: "refs/heads/main".into(),
            id
        }
    );
    let refs = repo.references().unwrap();
    assert_eq!(
        refs.iter()
            .map(|reference| reference.name.to_string())
            .collect::<Vec<_>>(),
        [
            "refs/heads/feature/task",
            "refs/heads/main",
            "refs/remotes/origin/HEAD",
            "refs/remotes/origin/main",
            "refs/tags/annotated",
            "refs/tags/light",
        ]
    );
    assert_eq!(
        refs[2].target,
        ReferenceTarget::Symbolic("refs/remotes/origin/main".into())
    );
    assert_ne!(refs[4].target, ReferenceTarget::Object(id));
    assert_eq!(refs[5].target, ReferenceTarget::Object(id));
    git(&dir.0, &["checkout", "--detach", &id.to_string()]);
    assert_eq!(repo.head().unwrap(), Head::Detached { id });
}

#[test]
fn bounded_history_traverses_both_merge_parents_once() {
    let dir = repository();
    let base = commit(&dir.0, "base");
    git(&dir.0, &["checkout", "-b", "feature"]);
    let feature = commit(&dir.0, "feature");
    git(&dir.0, &["checkout", "main"]);
    let main = commit(&dir.0, "main");
    git(&dir.0, &["merge", "--no-ff", "feature", "-m", "merge"]);
    let repo = Repository::discover(&dir.0).unwrap();
    let history = repo.history(20).unwrap();
    assert_eq!(history.len(), 4);
    assert_eq!(history[0].parents, [main, feature]);
    assert_eq!(history[0].message, "merge\n");
    assert_eq!(history[1].id, main);
    assert_eq!(history[2].id, feature);
    assert_eq!(history[3].id, base);
    assert!(history[3].parents.is_empty());
    assert_eq!(repo.history(2).unwrap(), history[..2]);
    assert!(repo.history(0).unwrap().is_empty());
}

#[test]
fn linked_worktree_uses_its_own_head_and_root() {
    let dir = repository();
    let id = commit(&dir.0, "base");
    let linked = dir.0.join("linked");
    git(&dir.0, &["worktree", "add", "-b", "task", "linked"]);
    let repo = Repository::discover(&linked).unwrap();
    assert_eq!(repo.work_dir(), linked);
    assert_ne!(repo.git_dir(), dir.0.join(".git"));
    assert_eq!(
        repo.head().unwrap(),
        Head::Attached {
            reference: "refs/heads/task".into(),
            id
        }
    );
}

#[test]
fn discovery_rejects_missing_bare_and_broken_nearest_repositories() {
    let missing = TestDir::new();
    assert!(matches!(
        Repository::discover(&missing.0),
        Err(Error::NotFound { .. })
    ));
    assert!(matches!(
        Repository::discover(&missing.0.join("absent")),
        Err(Error::Io { .. })
    ));
    fs::write(missing.0.join("file"), "file").unwrap();
    assert!(matches!(
        Repository::discover(&missing.0.join("file")),
        Err(Error::NotDirectory { .. })
    ));
    git(&missing.0, &["init", "--bare", "--template="]);
    assert!(matches!(
        Repository::discover(&missing.0),
        Err(Error::Open(OpenError::Bare))
    ));

    let dir = repository();
    let nested = dir.0.join("nested");
    fs::create_dir(&nested).unwrap();
    fs::write(nested.join(".git"), "gitdir: missing").unwrap();
    assert!(matches!(
        Repository::discover(&nested),
        Err(Error::Open(OpenError::Open(_)))
    ));
}

#[test]
fn broken_head_target_is_an_error() {
    let dir = repository();
    fs::write(
        dir.0.join(".git/refs/heads/main"),
        "1111111111111111111111111111111111111111\n",
    )
    .unwrap();
    let repo = Repository::discover(&dir.0).unwrap();
    assert!(matches!(repo.head(), Err(Error::Operation { .. })));
    assert!(repo.history(1).is_err());
}
