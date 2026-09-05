use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

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
        .args(["-c", "commit.gpgsign=false"])
        .args(args)
        .output()
        .expect("run Git fixture command")
}

fn git_ok(root: &Path, args: &[&str]) -> Vec<u8> {
    let output = git(root, args);
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    output.stdout
}

fn project() -> (TestDir, PathBuf) {
    let fixture = TestDir::new();
    let root = fixture.0.join("workspace/Billing");
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(
        root.join("eska.toml"),
        "[project]\ntype = 'configuration'\n",
    )
    .unwrap();
    fs::write(root.join("src/module.bsl"), "base\n").unwrap();
    fs::write(fixture.0.join("outside.txt"), "base\n").unwrap();
    git_ok(
        &fixture.0,
        &["init", "--initial-branch=main", "--template="],
    );
    git_ok(&fixture.0, &["config", "user.name", "Eska Test"]);
    git_ok(
        &fixture.0,
        &["config", "user.email", "eska@example.invalid"],
    );
    git_ok(&fixture.0, &["add", "."]);
    git_ok(&fixture.0, &["commit", "-m", "base"]);
    (fixture, root)
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).replace(['\u{2068}', '\u{2069}'], "")
}

/// `-m` saves all project changes while keeping sibling changes outside the commit.
#[test]
fn saves_project_changes_with_an_explicit_message_in_both_locales() {
    for (locale, expected) in [("ru", "Сохранено файлов"), ("en", "Saved files")] {
        let (fixture, root) = project();
        fs::write(root.join("src/module.bsl"), "changed\n").unwrap();
        fs::write(root.join("src/new.bsl"), "new\n").unwrap();
        fs::create_dir_all(root.join("src/Catalogs")).unwrap();
        fs::write(root.join("src/Catalogs/Broken.xml"), "not xml\n").unwrap();
        fs::write(fixture.0.join("outside.txt"), "outside\n").unwrap();

        let output = eska(&root, locale, &["save", "-m", "project changes"]);

        assert!(output.status.success(), "{}", text(&output.stderr));
        assert!(output.stderr.is_empty());
        assert!(text(&output.stdout).contains(expected));
        assert_eq!(
            git_ok(&fixture.0, &["log", "-1", "--format=%s"]),
            b"project changes\n"
        );
        assert_eq!(
            git_ok(&fixture.0, &["show", "--format=", "--name-only", "HEAD"]),
            concat!(
                "workspace/Billing/src/Catalogs/Broken.xml\n",
                "workspace/Billing/src/module.bsl\n",
                "workspace/Billing/src/new.bsl\n"
            )
            .as_bytes()
        );
        assert_eq!(
            git_ok(&fixture.0, &["status", "--short"]),
            b" M outside.txt\n"
        );
    }
}

/// Empty and detached states fail with localized diagnostics and no new commit.
#[test]
fn preflight_errors_are_localized() {
    for (locale, no_changes, detached) in [
        (
            "ru",
            "В проекте нет изменений",
            "Нельзя сохранить изменения при detached HEAD",
        ),
        (
            "en",
            "The project has no changes",
            "Changes cannot be saved with a detached HEAD",
        ),
    ] {
        let (fixture, root) = project();
        let initial_head = git_ok(&fixture.0, &["rev-parse", "HEAD"]);
        let clean = eska(&root, locale, &["save", "-m", "unused"]);
        assert_eq!(clean.status.code(), Some(1));
        assert!(text(&clean.stderr).contains(no_changes));

        git_ok(&fixture.0, &["checkout", "--detach"]);
        fs::write(root.join("src/module.bsl"), "changed\n").unwrap();
        let output = eska(&root, locale, &["save", "-m", "detached"]);
        assert_eq!(output.status.code(), Some(1));
        assert!(text(&output.stderr).contains(detached));
        assert_eq!(git_ok(&fixture.0, &["rev-parse", "HEAD"]), initial_head);
    }
}

/// Unmerged project files are rejected without resolving or staging the conflict.
#[test]
fn conflicts_are_rejected_in_both_locales() {
    for (locale, expected) in [
        ("ru", "конфликтов в файлах проекта"),
        ("en", "project files have conflicts"),
    ] {
        let (fixture, root) = project();
        git_ok(&fixture.0, &["checkout", "-b", "other"]);
        fs::write(root.join("src/module.bsl"), "other\n").unwrap();
        git_ok(&fixture.0, &["add", "."]);
        git_ok(&fixture.0, &["commit", "-m", "other"]);
        git_ok(&fixture.0, &["checkout", "main"]);
        fs::write(root.join("src/module.bsl"), "main\n").unwrap();
        git_ok(&fixture.0, &["add", "."]);
        git_ok(&fixture.0, &["commit", "-m", "main"]);
        let head = git_ok(&fixture.0, &["rev-parse", "HEAD"]);
        let merge = git(&fixture.0, &["merge", "other"]);
        assert!(!merge.status.success());
        let before = git_ok(&fixture.0, &["status", "--short"]);

        let output = eska(&root, locale, &["save", "-m", "conflict"]);

        assert_eq!(output.status.code(), Some(1));
        assert!(text(&output.stderr).contains(expected));
        assert_eq!(git_ok(&fixture.0, &["rev-parse", "HEAD"]), head);
        assert_eq!(git_ok(&fixture.0, &["status", "--short"]), before);
    }
}

/// A failed commit restores the exact index that existed before project staging.
#[cfg(unix)]
#[test]
fn failed_commit_restores_existing_staging() {
    let (fixture, root) = project();
    fs::write(fixture.0.join("outside.txt"), "staged outside\n").unwrap();
    git_ok(&fixture.0, &["add", "outside.txt"]);
    fs::write(root.join("src/module.bsl"), "changed\n").unwrap();
    let index = fixture.0.join(".git/index");
    let original_index = fs::read(&index).unwrap();
    let hook = fixture.0.join(".git/hooks/pre-commit");
    fs::create_dir_all(hook.parent().unwrap()).unwrap();
    fs::write(&hook, "#!/bin/sh\nexit 1\n").unwrap();
    fs::set_permissions(&hook, fs::Permissions::from_mode(0o755)).unwrap();
    let original_head = git_ok(&fixture.0, &["rev-parse", "HEAD"]);

    let output = eska(&root, "en", &["save", "-m", "blocked"]);

    assert_eq!(output.status.code(), Some(1));
    assert!(text(&output.stderr).contains("Could not create a commit"));
    assert_eq!(fs::read(index).unwrap(), original_index);
    assert_eq!(git_ok(&fixture.0, &["rev-parse", "HEAD"]), original_head);
    assert_eq!(
        git_ok(&fixture.0, &["status", "--short"]),
        b"M  outside.txt\n M workspace/Billing/src/module.bsl\n"
    );
}

/// Without `-m`, Git's configured editor supplies the commit message.
#[cfg(unix)]
#[test]
fn uses_the_configured_editor_when_message_is_omitted() {
    let (fixture, root) = project();
    fs::write(root.join("src/module.bsl"), "changed\n").unwrap();
    let editor = fixture.0.join("editor.sh");
    let captured = fixture.0.join("generated-message.txt");
    fs::write(
        &editor,
        format!(
            "#!/bin/sh\ncp \"$1\" '{}'\nprintf 'editor message\\n' > \"$1\"\n",
            captured.display()
        ),
    )
    .unwrap();
    fs::set_permissions(&editor, fs::Permissions::from_mode(0o755)).unwrap();
    let editor_command = format!("'{}'", editor.display());
    git_ok(&fixture.0, &["config", "core.editor", &editor_command]);

    let output = eska(&root, "en", &["save"]);

    assert!(output.status.success(), "{}", text(&output.stderr));
    assert_eq!(
        git_ok(&fixture.0, &["log", "-1", "--format=%s"]),
        b"editor message\n"
    );
    let draft = fs::read_to_string(captured).expect("captured generated draft");
    assert!(
        draft.starts_with("chore: Changes to project files\n\n"),
        "{draft}"
    );
    assert!(draft.contains("- File changed: src/module.bsl."), "{draft}");
}

/// The editor inherits the caller locale so it can render a UTF-8 generated draft.
#[cfg(unix)]
#[test]
fn generated_draft_editor_inherits_the_caller_locale() {
    let (fixture, root) = project();
    fs::write(root.join("src/module.bsl"), "changed\n").unwrap();
    let editor = fixture.0.join("editor.sh");
    fs::write(
        &editor,
        "#!/bin/sh\n[ \"$LC_ALL\" = \"C.UTF-8\" ] || exit 77\nprintf 'message\n' > \"$1\"\n",
    )
    .unwrap();
    fs::set_permissions(&editor, fs::Permissions::from_mode(0o755)).unwrap();
    let editor_command = format!("'{}'", editor.display());
    git_ok(&fixture.0, &["config", "core.editor", &editor_command]);

    let output = Command::new(env!("CARGO_BIN_EXE_eska"))
        .current_dir(&root)
        .env_remove("ESKA_LANG")
        .env("LC_ALL", "C.UTF-8")
        .args(["--lang", "ru", "save"])
        .output()
        .expect("run eska");

    assert!(output.status.success(), "{}", text(&output.stderr));
}

/// Leaving the generated template unchanged cancels the commit and restores prior staging.
#[cfg(unix)]
#[test]
fn unchanged_generated_draft_cancels_the_commit() {
    let (fixture, root) = project();
    fs::write(fixture.0.join("outside.txt"), "staged outside\n").unwrap();
    git_ok(&fixture.0, &["add", "outside.txt"]);
    fs::write(root.join("src/module.bsl"), "changed\n").unwrap();
    let original_head = git_ok(&fixture.0, &["rev-parse", "HEAD"]);
    let original_index = fs::read(fixture.0.join(".git/index")).unwrap();
    let editor = fixture.0.join("editor.sh");
    fs::write(&editor, "#!/bin/sh\nexit 0\n").unwrap();
    fs::set_permissions(&editor, fs::Permissions::from_mode(0o755)).unwrap();
    let editor_command = format!("'{}'", editor.display());
    git_ok(&fixture.0, &["config", "core.editor", &editor_command]);

    let output = eska(&root, "ru", &["save"]);

    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert_eq!(git_ok(&fixture.0, &["rev-parse", "HEAD"]), original_head);
    assert_eq!(
        fs::read(fixture.0.join(".git/index")).unwrap(),
        original_index
    );
}

/// Semantic changes produce a localized Conventional Commit draft before the editor opens.
#[cfg(unix)]
#[test]
fn generated_draft_summarizes_semantic_changes_in_both_locales() {
    for (locale, subject, module, method) in [
        (
            "ru",
            "feat(common-module): Изменения ОбщийМодуль.Обмен",
            "- Изменён модуль: ОбщийМодуль.Обмен.",
            "- Изменена процедура: ОбщийМодуль.Обмен — Выполнить.",
        ),
        (
            "en",
            "feat(common-module): Changes to CommonModule.Обмен",
            "- Module changed: CommonModule.Обмен.",
            "- Procedure changed: CommonModule.Обмен — Выполнить.",
        ),
    ] {
        let (fixture, root) = project();
        let module_path = root.join("src/CommonModules/Обмен/Ext/Module.bsl");
        fs::create_dir_all(module_path.parent().unwrap()).unwrap();
        fs::write(
            root.join("src/CommonModules/Обмен.xml"),
            r#"<MetaDataObject xmlns="http://v8.1c.ru/8.3/MDClasses"><CommonModule uuid="11111111-1111-1111-1111-111111111111"><Properties><Name>Обмен</Name></Properties></CommonModule></MetaDataObject>"#,
        )
        .unwrap();
        fs::write(
            &module_path,
            "Процедура Выполнить()\n    Возврат;\nКонецПроцедуры\n",
        )
        .unwrap();
        git_ok(&fixture.0, &["add", "."]);
        git_ok(&fixture.0, &["commit", "-m", "semantic base"]);
        fs::write(
            &module_path,
            "Процедура Выполнить()\n    Сообщить(\"Готово\");\nКонецПроцедуры\n",
        )
        .unwrap();
        let editor = fixture.0.join("editor.sh");
        fs::write(&editor, "#!/bin/sh\nprintf '\naccepted\n' >> \"$1\"\n").unwrap();
        fs::set_permissions(&editor, fs::Permissions::from_mode(0o755)).unwrap();
        let editor_command = format!("'{}'", editor.display());
        git_ok(&fixture.0, &["config", "core.editor", &editor_command]);

        let output = eska(&root, locale, &["save"]);

        assert!(output.status.success(), "{}", text(&output.stderr));
        let message = String::from_utf8(git_ok(&fixture.0, &["log", "-1", "--format=%B"])).unwrap();
        for expected in [subject, module, method] {
            assert!(
                message.contains(expected),
                "missing `{expected}` in:\n{message}"
            );
        }
    }
}

/// A failed generated-draft editor restores staging exactly like an explicit-message failure.
#[cfg(unix)]
#[test]
fn generated_editor_failure_restores_existing_staging() {
    let (fixture, root) = project();
    fs::write(fixture.0.join("outside.txt"), "staged outside\n").unwrap();
    git_ok(&fixture.0, &["add", "outside.txt"]);
    fs::write(root.join("src/module.bsl"), "changed\n").unwrap();
    let index = fixture.0.join(".git/index");
    let original_index = fs::read(&index).unwrap();
    let original_head = git_ok(&fixture.0, &["rev-parse", "HEAD"]);
    let editor = fixture.0.join("editor.sh");
    fs::write(&editor, "#!/bin/sh\nexit 1\n").unwrap();
    fs::set_permissions(&editor, fs::Permissions::from_mode(0o755)).unwrap();
    let editor_command = format!("'{}'", editor.display());
    git_ok(&fixture.0, &["config", "core.editor", &editor_command]);

    let output = eska(&root, "en", &["save"]);

    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert!(text(&output.stderr).contains("Could not create a commit"));
    assert_eq!(fs::read(index).unwrap(), original_index);
    assert_eq!(git_ok(&fixture.0, &["rev-parse", "HEAD"]), original_head);
}

/// Help and empty-message validation are localized CLI behavior.
#[test]
fn help_and_empty_message_are_localized() {
    for (locale, help, empty) in [
        (
            "ru",
            "Сохранить текущие изменения проекта",
            "Сообщение commit не может быть пустым",
        ),
        (
            "en",
            "Save current project changes",
            "The commit message cannot be empty",
        ),
    ] {
        let help_output = eska(Path::new("."), locale, &["save", "--help"]);
        assert!(help_output.status.success());
        assert!(text(&help_output.stdout).contains(help));

        let (_fixture, root) = project();
        fs::write(root.join("src/module.bsl"), "changed\n").unwrap();
        let output = eska(&root, locale, &["save", "-m", "   "]);
        assert_eq!(output.status.code(), Some(1));
        assert!(text(&output.stderr).contains(empty));
    }
}
