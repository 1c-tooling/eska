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

/// Human output groups Configurator identities and falls back only for ordinary files.
#[test]
fn human_output_groups_metadata_and_detects_changed_attributes() {
    let (_fixture, root) = project();
    fs::create_dir_all(root.join("src/Catalogs")).expect("create catalogs");
    fs::create_dir_all(root.join("src/CommonModules/ОбщийМодуль1/Ext"))
        .expect("create common module");
    fs::create_dir_all(root.join("src/Documents/Приход/Forms/ФормаДокумента/Ext/Form"))
        .expect("create document form");
    fs::write(
        root.join("src/Catalogs/Контрагенты.xml"),
        catalog_descriptor("Исходный"),
    )
    .expect("write catalog descriptor");
    fs::write(
        root.join("src/CommonModules/ОбщийМодуль1.xml"),
        common_module_descriptor(),
    )
    .expect("write common module descriptor");
    fs::write(
        root.join("src/CommonModules/ОбщийМодуль1/Ext/Module.bsl"),
        "Процедура Тест()\nКонецПроцедуры\n",
    )
    .expect("write common module");
    fs::write(root.join("notes.txt"), "Исходный\n").expect("write ordinary file");
    fs::write(
        root.join("src/Documents/Приход/Forms/ФормаДокумента/Ext/Form/Module.bsl"),
        "Процедура ПриОткрытии()\nКонецПроцедуры\n",
    )
    .expect("write form module");
    git(&root, &["add", "."]);
    git(&root, &["commit", "-m", "metadata"]);

    fs::write(
        root.join("src/Catalogs/Контрагенты.xml"),
        catalog_descriptor("Изменённый"),
    )
    .expect("change attribute");
    fs::write(
        root.join("src/CommonModules/ОбщийМодуль1/Ext/Module.bsl"),
        "Процедура Тест()\n    Возврат;\nКонецПроцедуры\n",
    )
    .expect("change module");
    fs::write(root.join("notes.txt"), "Изменённый\n").expect("change ordinary file");
    fs::write(
        root.join("src/Documents/Приход/Forms/ФормаДокумента/Ext/Form/Module.bsl"),
        "Процедура ПриОткрытии()\n    Возврат;\nКонецПроцедуры\n",
    )
    .expect("change form module");

    for (locale, catalog, module, form, other) in [
        (
            "ru",
            "Справочник.Контрагенты.Реквизит.Реквизит1",
            "ОбщийМодуль.ОбщийМодуль1",
            "Документ.Приход.Форма.ФормаДокумента",
            "Прочие файлы",
        ),
        (
            "en",
            "Catalog.Контрагенты.Attribute.Реквизит1",
            "CommonModule.ОбщийМодуль1",
            "Document.Приход.Form.ФормаДокумента",
            "Other files",
        ),
    ] {
        let output = eska(&root, locale, &["diff"]);
        assert!(output.status.success(), "{output:?}");
        let text = String::from_utf8(output.stdout).expect("UTF-8 human diff");
        for expected in [catalog, module, form, other, "notes.txt"] {
            assert!(text.contains(expected), "missing `{expected}` in:\n{text}");
        }
        let other_position = text.find(other).unwrap();
        assert!(text.find(catalog).unwrap() < other_position, "{text}");
        assert!(text.find(module).unwrap() < other_position, "{text}");
        assert!(text.find(form).unwrap() < other_position, "{text}");
        assert!(!text.contains("src/Catalogs/Контрагенты.xml"), "{text}");
        assert!(!text.contains("src/CommonModules"), "{text}");
        assert!(!text.contains("src/Documents"), "{text}");
    }

    let json = eska(&root, "ru", &["diff", "--format", "json"]);
    let document: Value = serde_json::from_slice(&json.stdout).expect("valid JSON diff");
    assert_eq!(document["files"].as_array().unwrap().len(), 4);
    assert_eq!(
        document["files"][0]["path"], "notes.txt",
        "JSON remains file-based and byte-order sorted"
    );
}

/// Human output assigns Designer payload files to their nearest Configurator owner.
#[test]
fn human_output_collapses_designer_payloads_without_changing_json_paths() {
    let (_fixture, root) = project();
    let files = [
        "src/CommonCommands/Обновить/Ext/CommandModule.bsl",
        "src/DocumentNumerators/Счета.xml",
        "src/Ext/Splash/Picture.png",
        "src/Reports/Продажи/Templates/ОсновнаяСхема/Ext/Template/Items/Логотип/Picture.png",
        "src/Subsystems/Учет/Subsystems/Продажи/Ext/CommandInterface.xml",
        "src/WSReferences/Статистика/Ext/1.xsd",
    ];
    for path in files {
        let path = root.join(path);
        fs::create_dir_all(path.parent().unwrap()).expect("create payload parent");
        fs::write(path, "payload").expect("write Designer payload");
    }

    for (locale, expected) in [
        (
            "ru",
            [
                "ОбщаяКоманда.Обновить",
                "Нумератор.Счета",
                "Конфигурация",
                "Отчет.Продажи.Макет.ОсновнаяСхема",
                "Подсистема.Учет.Подсистема.Продажи",
                "WSСсылка.Статистика",
            ],
        ),
        (
            "en",
            [
                "CommonCommand.Обновить",
                "DocumentNumerator.Счета",
                "Configuration",
                "Report.Продажи.Template.ОсновнаяСхема",
                "Subsystem.Учет.Subsystem.Продажи",
                "WSReference.Статистика",
            ],
        ),
    ] {
        let output = eska(&root, locale, &["diff"]);
        assert!(output.status.success(), "{output:?}");
        let text = String::from_utf8(output.stdout).expect("UTF-8 human diff");
        for logical_path in expected {
            assert!(
                text.contains(logical_path),
                "missing `{logical_path}` in:\n{text}"
            );
        }
        assert!(!text.contains("Прочие файлы"), "{text}");
        assert!(!text.contains("Other files"), "{text}");
        assert!(!text.contains("src/"), "{text}");
    }

    let output = eska(&root, "ru", &["diff", "--format", "json"]);
    let document: Value = serde_json::from_slice(&output.stdout).expect("valid JSON diff");
    let actual_paths: Vec<_> = document["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|file| file["path"].as_str().unwrap())
        .collect();
    assert_eq!(actual_paths, files);
}

/// Build a minimal catalog descriptor whose attribute property can change independently.
fn catalog_descriptor(comment: &str) -> String {
    format!(
        r#"<MetaDataObject xmlns="http://v8.1c.ru/8.3/MDClasses"><Catalog><Properties><Name>Контрагенты</Name></Properties><ChildObjects><Attribute><Properties><Name>Реквизит1</Name><Comment>{comment}</Comment></Properties></Attribute></ChildObjects></Catalog></MetaDataObject>"#
    )
}

/// Build a minimal common module descriptor accepted by the metadata projection.
const fn common_module_descriptor() -> &'static str {
    r#"<MetaDataObject xmlns="http://v8.1c.ru/8.3/MDClasses"><CommonModule><Properties><Name>ОбщийМодуль1</Name></Properties></CommonModule></MetaDataObject>"#
}
