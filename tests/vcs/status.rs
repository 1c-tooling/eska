use std::{fs, path::Path, process::Command};

use eska::vcs::{
    repository::{Error, Operation, Repository},
    status::{Change, Status},
};

use super::support::{commit, git, git_output, repository};

fn changes(status: &Status) -> Vec<(String, Option<Change>, Option<Change>)> {
    status
        .entries
        .iter()
        .map(|entry| (entry.path.to_string(), entry.index, entry.worktree))
        .collect()
}

#[test]
fn missing_index_stays_missing_and_stat_only_changes_stay_clean() {
    let dir = repository();
    let repo = Repository::discover(&dir.0).unwrap();
    assert!(!repo.status().unwrap().is_dirty());
    fs::write(dir.0.join("file"), "file\n").unwrap();
    assert_eq!(
        changes(&repo.status().unwrap()),
        [("file".into(), None, Some(Change::Untracked))]
    );
    assert!(!dir.0.join(".git/index").exists());
    git(&dir.0, &["add", "file"]);
    git(&dir.0, &["commit", "-m", "file"]);
    let index = fs::read(dir.0.join(".git/index")).unwrap();
    fs::remove_file(dir.0.join("file")).unwrap();
    fs::write(dir.0.join("file"), "file\n").unwrap();
    assert!(!repo.status().unwrap().is_dirty());
    assert_eq!(fs::read(dir.0.join(".git/index")).unwrap(), index);
}

#[test]
fn staged_unstaged_untracked_and_ignored_paths_are_distinct_and_read_only() {
    let dir = repository();
    commit(&dir.0, "modified");
    commit(&dir.0, "deleted");
    fs::write(dir.0.join(".gitignore"), "ignored/\n*.tmp\n").unwrap();
    git(&dir.0, &["add", ".gitignore"]);
    git(&dir.0, &["commit", "-m", "ignore"]);
    let repo = Repository::discover(&dir.0).unwrap();
    assert!(!repo.status().unwrap().is_dirty());
    fs::write(dir.0.join("modified"), "staged\n").unwrap();
    git(&dir.0, &["add", "modified"]);
    fs::write(dir.0.join("modified"), "unstaged\n").unwrap();
    fs::remove_file(dir.0.join("deleted")).unwrap();
    fs::create_dir_all(dir.0.join("untracked/subdir")).unwrap();
    fs::write(dir.0.join("untracked/subdir/модуль.bsl"), "new").unwrap();
    fs::create_dir(dir.0.join("ignored")).unwrap();
    fs::write(dir.0.join("ignored/file"), "ignored").unwrap();
    fs::write(dir.0.join("ignored.tmp"), "ignored").unwrap();
    let index_before = fs::read(dir.0.join(".git/index")).unwrap();
    let status = repo.status().unwrap();
    assert!(status.is_dirty());
    assert_eq!(
        changes(&status),
        [
            ("deleted".into(), None, Some(Change::Deleted)),
            (
                "modified".into(),
                Some(Change::Modified),
                Some(Change::Modified)
            ),
            (
                "untracked/subdir/модуль.bsl".into(),
                None,
                Some(Change::Untracked)
            ),
        ]
    );
    assert_eq!(
        status
            .changed_paths()
            .map(ToString::to_string)
            .collect::<Vec<_>>(),
        ["deleted", "modified", "untracked/subdir/модуль.bsl"]
    );
    assert_eq!(repo.status().unwrap(), status);
    assert_eq!(fs::read(dir.0.join(".git/index")).unwrap(), index_before);
    assert!(!dir.0.join(".git/index.lock").exists());
}

#[test]
fn unborn_index_and_intent_to_add_are_reported() {
    let dir = repository();
    fs::write(dir.0.join("added"), "staged").unwrap();
    fs::write(dir.0.join("intent"), "intent").unwrap();
    git(&dir.0, &["add", "added"]);
    git(&dir.0, &["add", "--intent-to-add", "intent"]);
    let repo = Repository::discover(&dir.0).unwrap();
    assert_eq!(
        changes(&repo.status().unwrap()),
        [
            ("added".into(), Some(Change::Added), None),
            ("intent".into(), None, Some(Change::IntentToAdd)),
        ]
    );
}

#[test]
fn moves_include_both_paths_and_staged_deletion_can_also_be_untracked() {
    let dir = repository();
    commit(&dir.0, "old");
    commit(&dir.0, "removed");
    git(&dir.0, &["mv", "old", "new"]);
    git(&dir.0, &["rm", "removed"]);
    fs::write(dir.0.join("removed"), "replacement").unwrap();
    git(&dir.0, &["config", "status.renames", "true"]);
    let repo = Repository::discover(&dir.0).unwrap();
    assert_eq!(
        changes(&repo.status().unwrap()),
        [
            ("new".into(), Some(Change::Added), None),
            ("old".into(), Some(Change::Deleted), None),
            (
                "removed".into(),
                Some(Change::Deleted),
                Some(Change::Untracked)
            ),
        ]
    );
}

#[test]
fn unmerged_index_is_a_conflict() {
    let dir = repository();
    commit(&dir.0, "conflict");
    git(&dir.0, &["checkout", "-b", "feature"]);
    fs::write(dir.0.join("conflict"), "feature\n").unwrap();
    git(&dir.0, &["commit", "-am", "feature"]);
    git(&dir.0, &["checkout", "main"]);
    fs::write(dir.0.join("conflict"), "main\n").unwrap();
    git(&dir.0, &["commit", "-am", "main"]);
    assert!(!git_output(&dir.0, &["merge", "feature"]).status.success());
    let repo = Repository::discover(&dir.0).unwrap();
    let index_before = fs::read(dir.0.join(".git/index")).unwrap();
    assert_eq!(
        changes(&repo.status().unwrap()),
        [("conflict".into(), Some(Change::Conflict), None)]
    );
    assert_eq!(fs::read(dir.0.join(".git/index")).unwrap(), index_before);
}

#[test]
fn corrupt_index_returns_an_error() {
    let dir = repository();
    commit(&dir.0, "base");
    fs::write(dir.0.join(".git/index"), "broken").unwrap();
    let repo = Repository::discover(&dir.0).unwrap();
    assert!(matches!(repo.status(), Err(Error::InvalidIndex { .. })));
    fs::write(dir.0.join(".git/index"), [0_u8; 64]).unwrap();
    assert!(matches!(
        repo.status(),
        Err(Error::Operation {
            operation: Operation::Status,
            ..
        })
    ));
}

#[test]
fn repository_reads_ignore_environment_redirects_and_do_not_require_git() {
    let dir = repository();
    commit(&dir.0, "base");
    fs::create_dir(dir.0.join("nested")).unwrap();
    fs::write(dir.0.join("outside"), "new").unwrap();
    let output = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "vcs::status::isolated_reader_child",
            "--nocapture",
        ])
        .current_dir(dir.0.join("nested"))
        .env("ESKA_REPOSITORY_READER_TEST", &dir.0)
        .env("PATH", "")
        .env("GIT_DIR", dir.0.join("absent"))
        .env("GIT_WORK_TREE", dir.0.join("absent"))
        .env("GIT_INDEX_FILE", dir.0.join("absent-index"))
        .env("GIT_CONFIG_COUNT", "1")
        .env("GIT_CONFIG_KEY_0", "status.showUntrackedFiles")
        .env("GIT_CONFIG_VALUE_0", "no")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn isolated_reader_child() {
    let Some(root) = std::env::var_os("ESKA_REPOSITORY_READER_TEST") else {
        return;
    };
    let repo = Repository::discover(Path::new(".")).unwrap();
    assert_eq!(repo.work_dir(), Path::new(&root));
    assert!(repo.head().unwrap().id().is_some());
    assert_eq!(repo.history(5).unwrap().len(), 1);
    assert_eq!(repo.references().unwrap().len(), 1);
    assert_eq!(
        changes(&repo.status().unwrap()),
        [("outside".into(), None, Some(Change::Untracked))]
    );
}

#[cfg(unix)]
#[test]
fn non_utf8_paths_and_symlink_type_changes_are_preserved() {
    use std::{
        ffi::OsString,
        os::unix::{ffi::OsStringExt, fs::symlink},
    };
    let dir = repository();
    commit(&dir.0, "type");
    fs::remove_file(dir.0.join("type")).unwrap();
    symlink("missing", dir.0.join("type")).unwrap();
    let raw_name = b"raw-\xff\nname";
    fs::write(dir.0.join(OsString::from_vec(raw_name.to_vec())), "new").unwrap();
    let repo = Repository::discover(&dir.0).unwrap();
    let status = repo.status().unwrap();
    assert_eq!(status.entries.len(), 2);
    assert_eq!(status.entries[0].path.as_slice(), raw_name);
    assert_eq!(status.entries[1].worktree, Some(Change::TypeChanged));
    git(&dir.0, &["add", "type"]);
    assert_eq!(
        repo.status().unwrap().entries[1].index,
        Some(Change::TypeChanged)
    );
}
