use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};

use serde_json::{Value, json};

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

fn git(root: &Path, args: &[&str]) {
    let output = Command::new("git")
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
        .expect("run Git fixture command");
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn project() -> (TestDir, PathBuf) {
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
            "git-flow",
        ],
    );
    assert!(output.status.success(), "{output:?}");
    let root = fixture.0.join("Billing");
    git(&root, &["add", "."]);
    git(&root, &["commit", "-m", "base"]);
    git(
        &root,
        &["update-ref", "refs/remotes/origin/develop", "HEAD"],
    );
    git(&root, &["checkout", "-b", "feature/FI-1234"]);
    fs::write(
        root.join("src/feature.bsl"),
        "Процедура Новая()\nКонецПроцедуры\n",
    )
    .expect("write task change");
    git(&root, &["add", "."]);
    git(&root, &["commit", "-m", "feature"]);
    (fixture, root)
}

#[test]
fn json_output_has_a_stable_locale_independent_schema() {
    let (_fixture, root) = project();
    for locale in ["ru", "en"] {
        let output = eska(&root, locale, &["status", "--format", "json"]);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(output.stderr.is_empty());
        let actual: Value = serde_json::from_slice(&output.stdout).expect("valid JSON status");
        assert_eq!(
            actual,
            json!({
                "schema_version": 1,
                "project": {
                    "name": "Billing",
                    "root": root,
                    "type": "configuration"
                },
                "workflow": {
                    "preset": "git-flow",
                    "task": "FI-1234",
                    "branch": "feature/FI-1234",
                    "base": "develop",
                    "head": "attached"
                },
                "changes": {
                    "files": 0,
                    "added": 0,
                    "modified": 0,
                    "deleted": 0,
                    "type_changed": 0,
                    "untracked": 0,
                    "intent_to_add": 0,
                    "conflicts": 0
                },
                "synchronization": { "ahead": 1, "behind": 0 },
                "locks": { "available": false, "count": null },
                "readiness": { "save": false, "publish": true }
            })
        );
    }
}

#[test]
fn human_output_localizes_changes_and_conservative_readiness() {
    let (_fixture, root) = project();
    fs::write(root.join("src/feature.bsl"), "Изменено\n").expect("modify source");
    fs::write(root.join("src/new.bsl"), "Новый\n").expect("untracked source");

    for (locale, expected) in [
        (
            "ru",
            [
                "Проект:   Billing",
                "Тип:      Конфигурация",
                "Workflow: Git Flow",
                "Задача:   FI-1234",
                "Ветка:    feature/FI-1234",
                "База:     develop",
                "Файлов:",
                "Изменено:",
                "Не отслеживается:",
                "Захваты\n  Доступность: недоступно",
                "✓ можно сохранять",
                "✗ можно публиковать",
            ],
        ),
        (
            "en",
            [
                "Project:  Billing",
                "Type:     Configuration",
                "Workflow: Git Flow",
                "Task:     FI-1234",
                "Branch:   feature/FI-1234",
                "Base:     develop",
                "Files:",
                "Modified:",
                "Untracked:",
                "Locks\n  Availability: unavailable",
                "✓ can save",
                "✗ can publish",
            ],
        ),
    ] {
        let output = eska(&root, locale, &["status"]);
        assert!(output.status.success(), "{output:?}");
        assert!(output.stderr.is_empty());
        let text = String::from_utf8(output.stdout)
            .expect("UTF-8 human status")
            .replace(['\u{2068}', '\u{2069}'], "");
        for fragment in expected {
            assert!(text.contains(fragment), "missing `{fragment}` in:\n{text}");
        }
    }
}

#[test]
fn help_and_missing_workflow_errors_are_localized() {
    let fixture = TestDir::new();
    for (locale, help, error) in [
        (
            "ru",
            "Показать состояние проекта и workflow",
            "Для проекта не настроен workflow",
        ),
        (
            "en",
            "Show project and workflow state",
            "Workflow is not configured",
        ),
    ] {
        let help_output = eska(&fixture.0, locale, &["status", "--help"]);
        assert!(help_output.status.success(), "{help_output:?}");
        assert!(String::from_utf8_lossy(&help_output.stdout).contains(help));

        let root = fixture.0.join(format!("project-{locale}"));
        fs::create_dir_all(root.join("src")).expect("create source");
        fs::write(
            root.join("eska.toml"),
            "[project]\ntype = 'configuration'\n",
        )
        .expect("write project config");
        git(&root, &["init", "--initial-branch=main", "--template="]);
        let output = eska(&root, locale, &["status"]);
        assert_eq!(output.status.code(), Some(1), "{output:?}");
        assert!(output.stdout.is_empty());
        assert!(String::from_utf8_lossy(&output.stderr).contains(error));
    }
}

#[test]
fn changes_are_scoped_to_the_project_inside_an_ancestor_repository() {
    let fixture = TestDir::new();
    let root = fixture.0.join("Billing");
    fs::create_dir_all(root.join("src")).expect("create nested project");
    fs::write(
        root.join("eska.toml"),
        "[project]\ntype = 'configuration'\n[vcs.workflow]\npreset = 'trunk'\n",
    )
    .expect("write project config");
    fs::write(root.join("src/module.bsl"), "Исходный\n").expect("write project source");
    fs::write(fixture.0.join("outside.txt"), "Исходный\n").expect("write outer file");
    git(
        &fixture.0,
        &["init", "--initial-branch=main", "--template="],
    );
    git(&fixture.0, &["add", "."]);
    git(&fixture.0, &["commit", "-m", "base"]);
    git(
        &fixture.0,
        &["update-ref", "refs/remotes/origin/main", "HEAD"],
    );
    git(&fixture.0, &["checkout", "-b", "task/FI-42"]);
    fs::write(root.join("src/module.bsl"), "Изменён\n").expect("modify project source");
    fs::write(fixture.0.join("outside.txt"), "Изменён\n").expect("modify outer file");

    let output = eska(&root, "en", &["status", "--format", "json"]);
    assert!(output.status.success(), "{output:?}");
    let actual: Value = serde_json::from_slice(&output.stdout).expect("valid JSON status");
    assert_eq!(actual["changes"]["files"], 1);
    assert_eq!(actual["changes"]["modified"], 1);
    assert_eq!(actual["workflow"]["task"], "FI-42");
}
