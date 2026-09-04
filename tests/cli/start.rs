use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};

use crate::support::TestDir;

fn eska(current_dir: &Path, locale: &str, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_eska"))
        .current_dir(current_dir)
        .env_remove("ESKA_LANG")
        .args(["--lang", locale])
        .args(args)
        .output()
        .expect("run eska")
}

fn git(root: &Path, args: &[&str]) -> Output {
    Command::new("git")
        .current_dir(root)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", root.join("absent-global-config"))
        .env("GIT_AUTHOR_NAME", "Eska Test")
        .env("GIT_AUTHOR_EMAIL", "eska@example.invalid")
        .env("GIT_COMMITTER_NAME", "Eska Test")
        .env("GIT_COMMITTER_EMAIL", "eska@example.invalid")
        .env("GIT_AUTHOR_DATE", "2026-01-01T00:00:00+00:00")
        .env("GIT_COMMITTER_DATE", "2026-01-01T00:00:00+00:00")
        .args(["-c", "core.hooksPath=", "-c", "commit.gpgsign=false"])
        .args(args)
        .output()
        .expect("run Git fixture command")
}

fn git_ok(root: &Path, args: &[&str]) {
    let output = git(root, args);
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn local_project(workflow: &str) -> (TestDir, PathBuf) {
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
            workflow,
        ],
    );
    assert!(output.status.success(), "{output:?}");
    let root = fixture.0.join("Billing");
    git_ok(&root, &["add", "."]);
    git_ok(&root, &["commit", "-m", "base"]);
    if workflow == "git-flow" {
        git_ok(&root, &["branch", "develop"]);
    }
    (fixture, root)
}

fn project(workflow: &str) -> (TestDir, PathBuf, TestDir) {
    let (fixture, root) = local_project(workflow);
    let remote = TestDir::new();
    git_ok(
        &remote.0,
        &["init", "--bare", "--initial-branch=main", "--template="],
    );
    git_ok(
        &root,
        &[
            "remote",
            "add",
            "origin",
            remote.0.to_str().expect("UTF-8 path"),
        ],
    );
    let base = if workflow == "git-flow" {
        "develop"
    } else {
        "main"
    };
    git_ok(&root, &["push", "origin", base]);
    (fixture, root, remote)
}

#[test]
fn starts_locally_without_a_configured_remote_in_both_locales() {
    for (locale, expected) in [
        ("ru", "Удалённый репозиторий не настроен"),
        ("en", "No remote repository is configured"),
    ] {
        let (_fixture, root) = local_project("trunk");
        let output = eska(&root, locale, &["start", "LOCAL-1"]);

        assert!(output.status.success(), "{}", text(&output.stderr));
        assert!(output.stderr.is_empty());
        assert!(text(&output.stdout).contains(expected));
        let head = git(&root, &["rev-parse", "--abbrev-ref", "HEAD"]);
        assert_eq!(head.stdout.trim_ascii(), b"task/LOCAL-1");
    }
}

#[test]
fn inaccessible_remote_error_includes_remote_url_and_git_reason() {
    for (locale, expected) in [
        ("ru", "Не удалось получить изменения из репозитория origin"),
        ("en", "Could not fetch changes from repository origin"),
    ] {
        let (_fixture, root) = local_project("trunk");
        let missing = root.join("missing-remote.git");
        let url = missing.to_str().expect("UTF-8 path");
        git_ok(&root, &["remote", "add", "origin", url]);

        let output = eska(&root, locale, &["start", "FI-9"]);

        assert_eq!(output.status.code(), Some(1));
        assert!(output.stdout.is_empty());
        let error = text(&output.stderr);
        assert!(error.contains(expected), "{error}");
        assert!(error.contains(url), "{error}");
        assert!(
            error.contains("does not appear to be a git repository"),
            "{error}"
        );
        let head = git(&root, &["rev-parse", "--abbrev-ref", "HEAD"]);
        assert_eq!(head.stdout.trim_ascii(), b"main");
    }
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).replace(['\u{2068}', '\u{2069}'], "")
}

#[test]
fn starts_policy_branch_and_localizes_success() {
    for (locale, workflow, expected_branch, expected_message) in [
        ("ru", "trunk", "task/FI-1234", "Задача FI-1234 начата"),
        ("en", "git-flow", "feature/FI-1234", "Task FI-1234 started"),
    ] {
        let (_fixture, root, _remote) = project(workflow);
        let output = eska(&root, locale, &["start", "FI-1234"]);
        assert!(output.status.success(), "{}", text(&output.stderr));
        assert!(output.stderr.is_empty());
        assert!(text(&output.stdout).contains(expected_message));
        let head = git(&root, &["rev-parse", "--abbrev-ref", "HEAD"]);
        assert_eq!(head.stdout.trim_ascii(), expected_branch.as_bytes());

        let status = eska(&root, locale, &["status", "--format", "json"]);
        let json: serde_json::Value = serde_json::from_slice(&status.stdout).expect("status JSON");
        assert_eq!(json["workflow"]["task"], "FI-1234");
        assert_eq!(json["workflow"]["branch"], expected_branch);
    }
}

#[test]
fn dirty_preflight_is_localized_and_preserves_head() {
    for (locale, expected) in [
        ("ru", "Рабочее дерево содержит изменения"),
        ("en", "The worktree contains changes"),
    ] {
        let (_fixture, root, _remote) = project("trunk");
        fs::write(root.join("dirty.txt"), "dirty\n").expect("write dirty file");
        let output = eska(&root, locale, &["start", "FI-1"]);
        assert_eq!(output.status.code(), Some(1));
        assert!(output.stdout.is_empty());
        assert!(text(&output.stderr).contains(expected));
        let head = git(&root, &["rev-parse", "--abbrev-ref", "HEAD"]);
        assert_eq!(head.stdout.trim_ascii(), b"main");
    }
}

#[test]
fn help_and_invalid_task_are_localized() {
    for (locale, help, invalid) in [
        (
            "ru",
            "Начать работу над задачей",
            "Идентификатор задачи нельзя использовать",
        ),
        (
            "en",
            "Start work on a task",
            "The task identifier cannot be used",
        ),
    ] {
        let help_output = eska(Path::new("."), locale, &["start", "--help"]);
        assert!(help_output.status.success(), "{help_output:?}");
        assert!(text(&help_output.stdout).contains(help));

        let (_fixture, root, _remote) = project("trunk");
        let output = eska(&root, locale, &["start", "nested/FI-1"]);
        assert_eq!(output.status.code(), Some(1));
        assert!(text(&output.stderr).contains(invalid));
    }
}
