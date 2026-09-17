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
fn uses_branch_names_overridden_on_the_trunk_preset() {
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
    let task_placeholder = ["{", "task", "}"].concat();
    fs::write(
        root.join("eska.toml"),
        format!(
            "[project]\ntype = \"configuration\"\n\n[vcs.workflow]\npreset = \"trunk\"\n\n[vcs.workflow.policy]\nbase_branch = \"master\"\ntask_branch_template = \"feature/{task_placeholder}\"\nintegration_target = \"master\"\n"
        ),
    )
    .expect("configure branch names");
    git_ok(&root, &["branch", "-m", "master"]);
    git_ok(&root, &["add", "."]);
    git_ok(&root, &["commit", "-m", "base"]);
    git_ok(&root, &["branch", "feature/FI-34"]);

    let task = eska(&root, "ru", &["switch", "FI-34"]);
    assert!(task.status.success(), "{}", text(&task.stderr));
    assert_eq!(current_branch(&root), b"feature/FI-34");

    let base = eska(&root, "ru", &["switch", "--base"]);
    assert!(base.status.success(), "{}", text(&base.stderr));
    assert_eq!(current_branch(&root), b"master");
}

#[test]
fn missing_task_errors_are_localized_and_preserve_dirty_state() {
    for (locale, missing_message) in [
        ("ru", "Создайте её через eska start"),
        ("en", "Create it with eska start"),
    ] {
        let (_fixture, root) = project();
        fs::write(root.join("dirty.txt"), "dirty\n").expect("write dirty file");
        let missing = eska(&root, locale, &["switch", "FI-404"]);
        assert_eq!(missing.status.code(), Some(1));
        assert!(text(&missing.stderr).contains(missing_message));
        assert_eq!(fs::read(root.join("dirty.txt")).unwrap(), b"dirty\n");
        assert!(!root.join(".git/eska-shelves").exists());
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

/// JSON previews are identical across locales and do not create storage or change the index.
#[test]
fn dirty_switch_preview_and_same_branch_are_read_only() {
    let (_fixture, root) = project();
    fs::write(root.join("pending"), b"source").unwrap();
    let original = fs::read(root.join(".git/index")).unwrap();
    let ru = eska(
        &root,
        "ru",
        &["switch", "FI-34", "--dry-run", "--format", "json"],
    );
    let en = eska(
        &root,
        "en",
        &["switch", "FI-34", "--dry-run", "--format", "json"],
    );
    assert!(ru.status.success(), "{ru:?}");
    assert_eq!(ru.stdout, en.stdout);
    let document: serde_json::Value = serde_json::from_slice(&ru.stdout).unwrap();
    assert_eq!(document["dry_run"], true);
    assert_eq!(document["saved"]["files"][0]["path"], "pending");
    assert!(document["saved"]["id"].is_null());
    assert_eq!(fs::read(root.join(".git/index")).unwrap(), original);
    assert!(!root.join(".git/eska-shelves").exists());
    assert!(!root.join(".git/eska-shelves.lock").exists());
    assert_eq!(current_branch(&root), b"main");
    let same = eska(&root, "en", &["switch", "--base", "--format", "json"]);
    assert!(same.status.success(), "{same:?}");
    assert_eq!(fs::read(root.join("pending")).unwrap(), b"source");
    let task = eska(&root, "en", &["switch", "FI-34", "--format", "json"]);
    assert!(task.status.success(), "{task:?}");
    let document: serde_json::Value = serde_json::from_slice(&task.stdout).unwrap();
    assert!(document["saved"]["id"].is_string());
    let preview = eska(
        &root,
        "ru",
        &["switch", "--base", "--dry-run", "--format", "json"],
    );
    assert!(preview.status.success(), "{preview:?}");
    let document: serde_json::Value = serde_json::from_slice(&preview.stdout).unwrap();
    assert_eq!(document["restored"]["files"][0]["path"], "pending");
    assert_eq!(current_branch(&root), b"task/FI-34");
    let base = eska(&root, "ru", &["switch", "--base", "--format", "json"]);
    assert!(base.status.success(), "{base:?}");
    assert_eq!(fs::read(root.join("pending")).unwrap(), b"source");
    assert_eq!(fs::read(root.join(".git/index")).unwrap(), original);
}

/// Switching from a workspace member captures sibling members and files outside the workspace.
#[test]
fn workspace_switch_restores_all_repository_paths() {
    let fixture = TestDir::new();
    let root = fixture.0.join("tools");
    let report = root.join("src/report");
    let processing = root.join("src/processing");
    fs::create_dir_all(&report).unwrap();
    fs::create_dir_all(&processing).unwrap();
    fs::write(root.join("eska.toml"), "[workspace]\nmembers = ['src/report', 'src/processing']\n[vcs.workflow]\npreset = 'trunk'\n").unwrap();
    for (path, kind) in [(&report, "report"), (&processing, "processing")] {
        fs::write(
            path.join("eska.toml"),
            format!("[project]\nname = '{kind}'\ntype = '{kind}'\nsource = '.'\n"),
        )
        .unwrap();
        fs::write(path.join("source.xml"), b"base").unwrap();
    }
    git_ok(
        &fixture.0,
        &["init", "--initial-branch=main", "--template="],
    );
    git_ok(&fixture.0, &["add", "."]);
    git_ok(&fixture.0, &["commit", "-m", "base"]);
    git_ok(&fixture.0, &["branch", "task/A"]);
    fs::write(report.join("source.xml"), b"report staged").unwrap();
    git_ok(&fixture.0, &["add", "."]);
    fs::write(processing.join("source.xml"), b"processing unstaged").unwrap();
    fs::write(fixture.0.join("outside"), b"outside workspace").unwrap();
    let index = fs::read(fixture.0.join(".git/index")).unwrap();
    let result = eska(&report, "en", &["switch", "A"]);
    assert!(result.status.success(), "{result:?}");
    assert_eq!(fs::read(processing.join("source.xml")).unwrap(), b"base");
    assert!(!fixture.0.join("outside").exists());
    let result = eska(&root, "ru", &["switch", "--base"]);
    assert!(result.status.success(), "{result:?}");
    assert_eq!(fs::read(fixture.0.join(".git/index")).unwrap(), index);
    assert_eq!(
        fs::read(report.join("source.xml")).unwrap(),
        b"report staged"
    );
    assert_eq!(
        fs::read(processing.join("source.xml")).unwrap(),
        b"processing unstaged"
    );
    assert_eq!(
        fs::read(fixture.0.join("outside")).unwrap(),
        b"outside workspace"
    );
}
