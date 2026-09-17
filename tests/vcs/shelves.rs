//! Shelf integration tests against real repositories and native filesystem paths.

use super::support::{commit, git, repository};
use eska::vcs::{
    repository::Repository,
    shelves::{self, Session},
};
use std::fs;

/// Restore staged/unstaged bytes, deletions and untracked files without altering the index.
#[test]
fn round_trip_preserves_raw_index_and_worktree() {
    let root = repository();
    commit(&root.0, "file.bsl");
    commit(&root.0, "deleted.bsl");
    fs::write(root.0.join("file.bsl"), b"staged\r\n").unwrap();
    git(&root.0, &["add", "file.bsl"]);
    fs::write(root.0.join("file.bsl"), b"\xef\xbb\xbfworktree\r\n").unwrap();
    fs::remove_file(root.0.join("deleted.bsl")).unwrap();
    fs::write(root.0.join("новый файл.bsl"), b"untracked\r\n").unwrap();
    let repo = Repository::discover(&root.0).unwrap();
    let index = fs::read(repo.index_path()).unwrap();
    let preview = shelves::plan(&repo).unwrap();
    assert_eq!(preview.files.len(), 3);
    assert_eq!(fs::read(repo.index_path()).unwrap(), index);
    let session = Session::acquire(&repo).unwrap();
    let shelf = session.capture().unwrap();
    assert!(
        !Repository::discover(&root.0)
            .unwrap()
            .status()
            .unwrap()
            .is_dirty()
    );
    assert!(!root.0.join("новый файл.bsl").exists());
    session.restore(Some(&shelf.id)).unwrap();
    assert_eq!(fs::read(repo.index_path()).unwrap(), index);
    assert_eq!(
        fs::read(root.0.join("file.bsl")).unwrap(),
        b"\xef\xbb\xbfworktree\r\n"
    );
    assert!(!root.0.join("deleted.bsl").exists());
    assert_eq!(
        fs::read(root.0.join("новый файл.bsl")).unwrap(),
        b"untracked\r\n"
    );
    assert!(shelves::list(&repo).unwrap().is_empty());
}

/// Retain a shelf when its original branch commit no longer matches.
#[test]
fn moved_branch_retains_the_shelf() {
    let root = repository();
    commit(&root.0, "initial");
    fs::write(root.0.join("pending"), b"pending").unwrap();
    let repo = Repository::discover(&root.0).unwrap();
    let session = Session::acquire(&repo).unwrap();
    session.capture().unwrap();
    commit(&root.0, "new-commit");
    assert!(matches!(
        session.restore(None),
        Err(shelves::Error::MovedBranch)
    ));
    assert_eq!(shelves::list(&repo).unwrap().len(), 1);
}

/// A new ignored path must not be overwritten by an old untracked payload.
#[test]
fn ignored_collision_preserves_both_copies() {
    let root = repository();
    commit(&root.0, "initial");
    fs::write(root.0.join("pending"), b"original").unwrap();
    let repo = Repository::discover(&root.0).unwrap();
    let session = Session::acquire(&repo).unwrap();
    session.capture().unwrap();
    fs::create_dir_all(root.0.join(".git/info")).unwrap();
    fs::write(root.0.join(".git/info/exclude"), b"pending\n").unwrap();
    fs::write(root.0.join("pending"), b"ignored").unwrap();
    assert!(matches!(
        session.restore(None),
        Err(shelves::Error::Collision(_))
    ));
    assert_eq!(fs::read(root.0.join("pending")).unwrap(), b"ignored");
    assert_eq!(shelves::list(&repo).unwrap().len(), 1);
}

/// Staged additions and renames survive a complete capture/restore cycle.
#[test]
fn staged_additions_and_renames_survive() {
    let root = repository();
    commit(&root.0, "before");
    git(&root.0, &["mv", "before", "after"]);
    fs::write(root.0.join("added"), b"staged").unwrap();
    git(&root.0, &["add", "added"]);
    fs::write(root.0.join("added"), b"unstaged").unwrap();
    let repo = Repository::discover(&root.0).unwrap();
    let original = fs::read(repo.index_path()).unwrap();
    let session = Session::acquire(&repo).unwrap();
    session.capture().unwrap();
    assert!(root.0.join("before").exists());
    assert!(!root.0.join("after").exists());
    session.restore(None).unwrap();
    assert_eq!(fs::read(repo.index_path()).unwrap(), original);
    assert!(!root.0.join("before").exists());
    assert_eq!(fs::read(root.0.join("after")).unwrap(), b"before\n");
    assert_eq!(fs::read(root.0.join("added")).unwrap(), b"unstaged");
}

/// Locks and active Git operations reject capture before a snapshot is created.
#[test]
fn lock_and_operation_preflight_is_read_only() {
    let root = repository();
    let head = commit(&root.0, "initial");
    fs::write(root.0.join("pending"), b"saved").unwrap();
    let repo = Repository::discover(&root.0).unwrap();
    let _session = Session::acquire(&repo).unwrap();
    assert!(matches!(
        Session::acquire(&repo),
        Err(shelves::Error::Locked)
    ));
    fs::write(repo.git_dir().join("MERGE_HEAD"), head.to_string()).unwrap();
    assert!(matches!(
        shelves::plan(&repo),
        Err(shelves::Error::InProgress)
    ));
    assert!(shelves::list(&repo).unwrap().is_empty());
    assert_eq!(fs::read(root.0.join("pending")).unwrap(), b"saved");
}

/// Corrupt payloads must be detected before either index or worktree is overwritten.
#[test]
fn corrupt_payload_is_retained_and_never_applied() {
    let root = repository();
    commit(&root.0, "initial");
    fs::write(root.0.join("pending"), b"saved").unwrap();
    let repo = Repository::discover(&root.0).unwrap();
    let session = Session::acquire(&repo).unwrap();
    let saved = session.capture().unwrap();
    let index = fs::read(repo.index_path()).unwrap();
    fs::write(
        repo.git_dir().join("eska-shelves").join(saved.id).join("0"),
        b"broken",
    )
    .unwrap();
    assert!(matches!(
        session.restore(None),
        Err(shelves::Error::InvalidShelf)
    ));
    assert_eq!(fs::read(repo.index_path()).unwrap(), index);
    assert!(!root.0.join("pending").exists());
    assert_eq!(shelves::list(&repo).unwrap().len(), 1);
}

/// Native Git byte paths, symlink targets and executable permissions remain exact on Unix.
#[cfg(unix)]
#[test]
fn unix_bytes_modes_and_links_are_preserved() {
    use std::{
        ffi::OsStr,
        os::unix::{
            ffi::OsStrExt,
            fs::{PermissionsExt, symlink},
        },
    };
    let root = repository();
    commit(&root.0, "run");
    fs::set_permissions(root.0.join("run"), fs::Permissions::from_mode(0o755)).unwrap();
    let filename = root.0.join(OsStr::from_bytes(b"raw-\xff"));
    fs::write(&filename, b"raw content").unwrap();
    symlink(OsStr::from_bytes(b"target-\xff"), root.0.join("link")).unwrap();
    let repo = Repository::discover(&root.0).unwrap();
    let session = Session::acquire(&repo).unwrap();
    session.capture().unwrap();
    session.restore(None).unwrap();
    assert_eq!(
        fs::metadata(root.0.join("run"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o755
    );
    assert_eq!(fs::read(filename).unwrap(), b"raw content");
    assert_eq!(
        fs::read_link(root.0.join("link"))
            .unwrap()
            .as_os_str()
            .as_bytes(),
        b"target-\xff"
    );
}
