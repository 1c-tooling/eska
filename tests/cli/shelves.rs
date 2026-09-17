//! Public shelf CLI contracts, localization and read-only previews.

use crate::{support::TestDir, vcs::support::git};
use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};

/// Run the real CLI with deterministic locale and isolated global configuration.
fn eska(root: &Path, locale: &str, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_eska"))
        .current_dir(root)
        .env("ESKA_GLOBAL_CONFIG", root.join("absent-global.toml"))
        .env("NO_COLOR", "1")
        .args(["--lang", locale])
        .args(args)
        .output()
        .unwrap()
}

/// Create a committed configuration fixture through the public onboarding command.
fn project() -> (TestDir, PathBuf) {
    let fixture = TestDir::new();
    let output = eska(
        &fixture.0,
        "en",
        &[
            "new",
            "demo",
            "--type",
            "configuration",
            "--workflow",
            "trunk",
        ],
    );
    assert!(output.status.success(), "{output:?}");
    let root = fixture.0.join("demo");
    git(&root, &["add", "."]);
    git(&root, &["commit", "-m", "initial"]);
    (fixture, root)
}

/// Extract machine output only after checking exit status and terminal escape absence.
fn document(output: &Output) -> Value {
    assert!(output.status.success(), "{output:?}");
    assert!(output.stderr.is_empty(), "{output:?}");
    assert!(!output.stdout.contains(&0x1b));
    serde_json::from_slice(&output.stdout).unwrap()
}

/// Capture/list/restore keep stable IDs and restore the raw index across both UI languages.
#[test]
fn json_round_trip_and_previews_are_locale_independent() {
    let (_fixture, root) = project();
    fs::write(root.join("pending.bsl"), b"staged\r\n").unwrap();
    git(&root, &["add", "pending.bsl"]);
    fs::write(root.join("pending.bsl"), b"unstaged\r\n").unwrap();
    let index = fs::read(root.join(".git/index")).unwrap();
    let before = document(&eska(
        &root,
        "ru",
        &["shelve", "--dry-run", "--format", "json"],
    ));
    assert_eq!(
        before,
        document(&eska(
            &root,
            "en",
            &["shelve", "--dry-run", "--format", "json"]
        ))
    );
    assert_eq!(before["shelf"]["files"][0]["index"], "added");
    assert_eq!(before["shelf"]["files"][0]["worktree"], "modified");
    assert_eq!(before["shelf"]["id"], Value::Null);
    assert!(!root.join(".git/eska-shelves").exists());
    assert!(!root.join(".git/eska-shelves.lock").exists());
    assert_eq!(fs::read(root.join(".git/index")).unwrap(), index);
    let saved = document(&eska(&root, "ru", &["shelve", "--format", "json"]));
    let id = saved["shelf"]["id"].as_str().unwrap();
    assert!(!root.join("pending.bsl").exists());
    let listed = document(&eska(&root, "ru", &["shelves", "--format", "json"]));
    assert_eq!(
        listed,
        document(&eska(&root, "en", &["shelves", "--format", "json"]))
    );
    assert_eq!(listed["shelves"][0], saved["shelf"]);
    let preview = document(&eska(
        &root,
        "ru",
        &["unshelve", id, "--dry-run", "--format", "json"],
    ));
    assert_eq!(
        preview,
        document(&eska(
            &root,
            "en",
            &["unshelve", id, "--dry-run", "--format", "json"]
        ))
    );
    assert!(
        !root
            .join(".git/eska-shelves")
            .join(id)
            .join("restore.json")
            .exists()
    );
    document(&eska(&root, "en", &["unshelve", id, "--format", "json"]));
    assert_eq!(fs::read(root.join(".git/index")).unwrap(), index);
    assert_eq!(fs::read(root.join("pending.bsl")).unwrap(), b"unstaged\r\n");
    assert_eq!(
        document(&eska(&root, "en", &["shelves", "--format", "json"]))["shelves"],
        serde_json::json!([])
    );
}

/// Human results and help are localized without exposing backend terminology.
#[test]
fn human_results_and_help_are_localized() {
    for (locale, saved, restored, empty) in [
        ("ru", "сохранена", "восстановлена", "Сохранённых полок нет"),
        ("en", "saved", "restored", "No saved shelves"),
    ] {
        let (_fixture, root) = project();
        fs::write(root.join("новый файл"), b"pending").unwrap();
        for command in ["shelve", "unshelve", "shelves"] {
            let output = eska(&root, locale, &[command, "--help"]);
            assert!(output.status.success(), "{output:?}");
            assert!(!String::from_utf8_lossy(&output.stdout).contains("stash"));
        }
        let captured = eska(&root, locale, &["shelve"]);
        assert!(captured.status.success(), "{captured:?}");
        assert!(String::from_utf8_lossy(&captured.stdout).contains(saved));
        let recovered = eska(&root, locale, &["unshelve"]);
        assert!(recovered.status.success(), "{recovered:?}");
        assert!(String::from_utf8_lossy(&recovered.stdout).contains(restored));
        let listed = eska(&root, locale, &["shelves"]);
        assert!(String::from_utf8_lossy(&listed.stdout).contains(empty));
        assert!(!captured.stdout.contains(&0x1b));
    }
}

/// Runtime errors remain machine-readable and identical across locales.
#[test]
fn json_errors_are_stable_and_keep_localized_stderr() {
    let (_fixture, root) = project();
    for (command, code) in [("shelve", "empty"), ("unshelve", "shelf-missing")] {
        let ru = eska(&root, "ru", &[command, "--format", "json"]);
        let en = eska(&root, "en", &[command, "--format", "json"]);
        assert_eq!(ru.status.code(), Some(1));
        assert_eq!(en.status.code(), Some(1));
        assert_eq!(ru.stdout, en.stdout);
        assert_ne!(ru.stderr, en.stderr);
        let error: Value = serde_json::from_slice(&ru.stdout).unwrap();
        assert_eq!(error["error"]["code"], code);
        assert_eq!(error["schema_version"], 1);
    }
}
