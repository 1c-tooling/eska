use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};

use serde_json::{Value, json};

use crate::support::TestDir;

/// Run eska with an explicit locale in an isolated project fixture.
fn eska(current_dir: &Path, locale: &str, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_eska"))
        .current_dir(current_dir)
        .env_remove("ESKA_LANG")
        .args(["--lang", locale])
        .args(args)
        .output()
        .expect("run eska")
}

/// Run Git with deterministic identity and without user configuration.
fn git(root: &Path, args: &[&str]) {
    let output = Command::new("git")
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
        .expect("run Git fixture command");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// Create a committed nested project and return its owning fixture and root.
fn project() -> (TestDir, PathBuf) {
    let fixture = TestDir::new();
    let root = fixture.0.join("workspace").join("Billing");
    fs::create_dir_all(root.join("src")).expect("create source");
    fs::write(
        root.join("eska.toml"),
        "[project]\ntype = 'configuration'\n",
    )
    .expect("write project config");
    fs::write(root.join("src/module.bsl"), "Исходный\n").expect("write source");
    fs::write(fixture.0.join("outside.txt"), "Исходный\n").expect("write outer file");
    git(
        &fixture.0,
        &["init", "--initial-branch=main", "--template="],
    );
    git(&fixture.0, &["add", "."]);
    git(&fixture.0, &["commit", "-m", "base"]);
    (fixture, root)
}

/// JSON is stable across locales and excludes changes outside the project root.
#[test]
fn json_is_locale_independent_and_project_scoped() {
    let (_fixture, root) = project();
    fs::write(root.join("src/module.bsl"), "Подготовлено\n").expect("modify source");
    git(&root, &["add", "src/module.bsl"]);
    fs::write(root.join("src/module.bsl"), "Рабочая копия\n").expect("modify again");
    fs::write(root.join("src/new.bsl"), "Новый\n").expect("write untracked");
    fs::write(
        root.parent().unwrap().parent().unwrap().join("outside.txt"),
        "Вне проекта\n",
    )
    .expect("modify outer file");

    let expected = json!({
        "schema_version": 1,
        "files": [
            {"path": "src/module.bsl", "path_encoding": "utf-8", "index": "modified", "worktree": "modified"},
            {"path": "src/new.bsl", "path_encoding": "utf-8", "index": null, "worktree": "untracked"}
        ]
    });
    for locale in ["ru", "en"] {
        let output = eska(&root, locale, &["diff", "--format", "json"]);
        assert!(output.status.success(), "{output:?}");
        assert!(output.stderr.is_empty());
        let actual: Value = serde_json::from_slice(&output.stdout).expect("valid JSON diff");
        assert_eq!(actual, expected);
    }
}

/// Human output is localized, while raw output stays compact and stable.
#[test]
fn human_and_raw_modes_report_file_states() {
    let (_fixture, root) = project();
    fs::write(root.join("src/module.bsl"), "Изменено\n").expect("modify source");
    fs::write(root.join("src/new.bsl"), "Новый\n").expect("write untracked");

    for (locale, heading, modified, worktree, untracked) in [
        (
            "ru",
            "Изменения файлов",
            "изменён",
            "рабочая копия",
            "не отслеживается",
        ),
        (
            "en",
            "File changes",
            "modified",
            "working tree",
            "untracked",
        ),
    ] {
        let output = eska(&root, locale, &["diff"]);
        assert!(output.status.success(), "{output:?}");
        let text = String::from_utf8(output.stdout).expect("UTF-8 human diff");
        for expected in [
            heading,
            modified,
            worktree,
            untracked,
            "src/module.bsl",
            "src/new.bsl",
        ] {
            assert!(text.contains(expected), "missing `{expected}` in:\n{text}");
        }
    }

    let raw = eska(&root, "ru", &["diff", "--raw"]);
    assert!(raw.status.success(), "{raw:?}");
    assert_eq!(
        String::from_utf8(raw.stdout).unwrap(),
        ".M\tsrc/module.bsl\n.?\tsrc/new.bsl\n"
    );
    assert!(raw.stderr.is_empty());
}

/// A clean project has explicit human text and an empty raw stream.
#[test]
fn clean_output_and_help_are_localized() {
    let (_fixture, root) = project();
    for (locale, clean, about) in [
        (
            "ru",
            "Изменений файлов нет",
            "Показать изменения файлов проекта",
        ),
        ("en", "No file changes", "Show project file changes"),
    ] {
        let output = eska(&root, locale, &["diff"]);
        assert!(output.status.success(), "{output:?}");
        assert!(String::from_utf8_lossy(&output.stdout).contains(clean));

        let help = eska(&root, locale, &["diff", "--help"]);
        assert!(help.status.success(), "{help:?}");
        assert!(String::from_utf8_lossy(&help.stdout).contains(about));
    }
    let raw = eska(&root, "en", &["diff", "--raw"]);
    assert!(raw.status.success(), "{raw:?}");
    assert!(raw.stdout.is_empty());
}

/// Repository failures use exit code 1 and localized diagnostics.
#[test]
fn missing_repository_error_is_localized() {
    let fixture = TestDir::new();
    fs::create_dir(fixture.0.join("src")).expect("create source");
    fs::write(
        fixture.0.join("eska.toml"),
        "[project]\ntype = 'configuration'\n",
    )
    .expect("write project config");
    fs::write(fixture.0.join(".git"), "gitdir: absent\n")
        .expect("write invalid repository boundary");

    for (locale, expected) in [
        ("ru", "Не удалось прочитать изменения Git-репозитория"),
        ("en", "Could not read Git repository changes"),
    ] {
        let output = eska(&fixture.0, locale, &["diff"]);
        assert_eq!(output.status.code(), Some(1), "{output:?}");
        assert!(output.stdout.is_empty());
        assert!(String::from_utf8_lossy(&output.stderr).contains(expected));
    }
}

/// Raw and formatted output are mutually exclusive command contracts.
#[test]
fn raw_and_format_cannot_be_combined() {
    let fixture = TestDir::new();
    let output = eska(&fixture.0, "en", &["diff", "--raw", "--format", "json"]);
    assert_eq!(output.status.code(), Some(2), "{output:?}");
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("cannot be used with"));
}
