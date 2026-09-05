use std::{fs, path::Path, process::Command};

use crate::support::TestDir;

/// Run eska with an explicit locale in an isolated project fixture.
fn eska(current_dir: &Path, locale: &str, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_eska"))
        .current_dir(current_dir)
        .env_remove("ESKA_LANG")
        .args(["--lang", locale])
        .args(args)
        .output()
        .expect("run eska")
}

/// Run Git without inheriting user configuration.
fn git(root: &Path, args: &[&str]) -> std::process::Output {
    Command::new("git")
        .current_dir(root)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", root.join("absent-global-config"))
        .env("GIT_AUTHOR_NAME", "Eska Test")
        .env("GIT_AUTHOR_EMAIL", "eska@example.invalid")
        .env("GIT_COMMITTER_NAME", "Eska Test")
        .env("GIT_COMMITTER_EMAIL", "eska@example.invalid")
        .args(["-c", "core.hooksPath=", "-c", "commit.gpgsign=false"])
        .args(args)
        .output()
        .expect("run Git fixture command")
}

/// Run one successful Git fixture command.
fn git_ok(root: &Path, args: &[&str]) {
    let output = git(root, args);
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// Create a local trunk project with one committed task change.
fn task_project(task: &str) -> (TestDir, std::path::PathBuf) {
    let fixture = TestDir::new();
    let output = eska(
        &fixture.0,
        "en",
        &[
            "new",
            "Billing",
            "--type",
            "configuration",
            "--workflow",
            "trunk",
        ],
    );
    assert!(output.status.success(), "{output:?}");
    let root = fixture.0.join("Billing");
    git_ok(&root, &["add", "."]);
    git_ok(&root, &["commit", "-m", "base"]);
    let start = eska(&root, "en", &["start", task]);
    assert!(start.status.success(), "{}", text(&start.stderr));
    fs::write(root.join("task.txt"), "task\n").expect("write task change");
    git_ok(&root, &["add", "task.txt"]);
    git_ok(&root, &["commit", "-m", "task"]);
    (fixture, root)
}

#[test]
/// Verify successful RU and EN user-facing completion end to end.
fn finishes_an_integrated_local_task_in_both_locales() {
    for (locale, expected) in [
        ("ru", "Задача FI-40 завершена"),
        ("en", "Task FI-40 finished"),
    ] {
        let (_fixture, root) = task_project("FI-40");
        git_ok(&root, &["switch", "main"]);
        git_ok(&root, &["merge", "--ff-only", "task/FI-40"]);
        git_ok(&root, &["switch", "task/FI-40"]);

        let output = eska(&root, locale, &["finish"]);

        assert!(output.status.success(), "{}", text(&output.stderr));
        assert!(output.stderr.is_empty());
        assert!(text(&output.stdout).contains(expected));
        assert_eq!(current_branch(&root), b"main");
        assert!(
            !git(&root, &["show-ref", "--verify", "refs/heads/task/FI-40"])
                .status
                .success()
        );
    }
}

#[test]
/// Verify localized preflight failures leave the active task unchanged.
fn unintegrated_and_dirty_errors_are_localized_and_preserve_the_task() {
    for (locale, unintegrated, dirty) in [
        (
            "ru",
            "ещё не интегрирован",
            "Рабочее дерево содержит несохранённые изменения",
        ),
        (
            "en",
            "has not been integrated",
            "The worktree contains unsaved changes",
        ),
    ] {
        let (_fixture, root) = task_project("FI-41");
        let output = eska(&root, locale, &["finish"]);
        assert_eq!(output.status.code(), Some(1));
        assert!(text(&output.stderr).contains(unintegrated));
        assert_eq!(current_branch(&root), b"task/FI-41");

        fs::write(root.join("dirty.txt"), "dirty\n").expect("write dirty change");
        let output = eska(&root, locale, &["finish"]);
        assert_eq!(output.status.code(), Some(1));
        assert!(text(&output.stderr).contains(dirty));
        assert_eq!(current_branch(&root), b"task/FI-41");
    }
}

#[test]
/// Verify localized help and the command's argument contract.
fn help_is_localized_and_extra_arguments_are_rejected() {
    for (locale, expected) in [
        ("ru", "Завершить активную задачу"),
        ("en", "Finish the active task"),
    ] {
        let help = eska(Path::new("."), locale, &["finish", "--help"]);
        assert!(help.status.success(), "{help:?}");
        assert!(text(&help.stdout).contains(expected));

        let invalid = eska(Path::new("."), locale, &["finish", "unexpected"]);
        assert_eq!(invalid.status.code(), Some(2));
    }
}

/// Return the active branch name as raw bytes.
fn current_branch(root: &Path) -> Vec<u8> {
    git(root, &["rev-parse", "--abbrev-ref", "HEAD"])
        .stdout
        .trim_ascii()
        .to_vec()
}

/// Render process output while removing Fluent isolation marks.
fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).replace(['\u{2068}', '\u{2069}'], "")
}
