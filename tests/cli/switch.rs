use std::{fs, path::Path, process::Command};

use crate::support::TestDir;

fn eska(current_dir: &Path, locale: &str, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_eska"))
        .current_dir(current_dir)
        .env_remove("ESKA_LANG")
        .args(["--lang", locale])
        .args(args)
        .output()
        .expect("run eska")
}

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

fn git_ok(root: &Path, args: &[&str]) {
    let output = git(root, args);
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn project() -> (TestDir, std::path::PathBuf) {
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
    git_ok(&root, &["branch", "task/FI-34"]);
    (fixture, root)
}

#[test]
fn switches_to_a_task_and_back_to_base_in_both_locales() {
    for (locale, task_message, base_message) in [
        (
            "ru",
            "Активирована задача FI-34",
            "Активирована базовая ветка main",
        ),
        ("en", "Activated task FI-34", "Activated base branch main"),
    ] {
        let (_fixture, root) = project();
        let task = eska(&root, locale, &["switch", "FI-34"]);
        assert!(task.status.success(), "{}", text(&task.stderr));
        assert!(text(&task.stdout).contains(task_message));
        assert_eq!(current_branch(&root), b"task/FI-34");

        let base = eska(&root, locale, &["switch", "--base"]);
        assert!(base.status.success(), "{}", text(&base.stderr));
        assert!(text(&base.stdout).contains(base_message));
        assert_eq!(current_branch(&root), b"main");
    }
}

#[test]
fn dirty_and_missing_task_errors_are_localized_and_preserve_head() {
    for (locale, dirty_message, missing_message) in [
        ("ru", "Выполните eska save", "Создайте её через eska start"),
        ("en", "Run eska save", "Create it with eska start"),
    ] {
        let (_fixture, root) = project();
        fs::write(root.join("dirty.txt"), "dirty\n").expect("write dirty file");
        let dirty = eska(&root, locale, &["switch", "FI-34"]);
        assert_eq!(dirty.status.code(), Some(1));
        assert!(text(&dirty.stderr).contains(dirty_message));
        assert_eq!(current_branch(&root), b"main");

        fs::remove_file(root.join("dirty.txt")).expect("remove dirty fixture");
        let missing = eska(&root, locale, &["switch", "FI-404"]);
        assert_eq!(missing.status.code(), Some(1));
        assert!(text(&missing.stderr).contains(missing_message));
        assert_eq!(current_branch(&root), b"main");
    }
}

#[test]
fn help_and_target_exclusivity_are_localized() {
    for (locale, expected) in [
        ("ru", "Переключиться на существующую задачу"),
        ("en", "Switch to an existing task"),
    ] {
        let help = eska(Path::new("."), locale, &["switch", "--help"]);
        assert!(help.status.success(), "{help:?}");
        assert!(text(&help.stdout).contains(expected));

        let missing = eska(Path::new("."), locale, &["switch"]);
        assert_eq!(missing.status.code(), Some(2));
        let conflict = eska(Path::new("."), locale, &["switch", "FI-34", "--base"]);
        assert_eq!(conflict.status.code(), Some(2));
    }
}

fn current_branch(root: &Path) -> Vec<u8> {
    git(root, &["rev-parse", "--abbrev-ref", "HEAD"])
        .stdout
        .trim_ascii()
        .to_vec()
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).replace(['\u{2068}', '\u{2069}'], "")
}
