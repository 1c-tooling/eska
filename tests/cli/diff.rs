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

    for (locale, heading, modified, untracked) in [
        (
            "ru",
            "Изменения файлов",
            "Изменены — рабочая копия (1):\n    ✎ src/module.bsl",
            "Не отслеживаются — рабочая копия (1):\n    ? src/new.bsl",
        ),
        (
            "en",
            "File changes",
            "Modified — working tree (1):\n    ✎ src/module.bsl",
            "Untracked — working tree (1):\n    ? src/new.bsl",
        ),
    ] {
        let output = eska(&root, locale, &["diff"]);
        assert!(output.status.success(), "{output:?}");
        let text = String::from_utf8(output.stdout).expect("UTF-8 human diff");
        for expected in [heading, modified, untracked] {
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

/// One revision compares committed trees with HEAD while preserving logical metadata output.
#[test]
fn revision_diff_supports_human_raw_and_versioned_json() {
    let (_fixture, root) = project();
    fs::create_dir_all(root.join("src/Catalogs")).expect("create catalogs");
    fs::write(
        root.join("src/Catalogs/Контрагенты.xml"),
        catalog_descriptor("Исходный"),
    )
    .expect("write catalog descriptor");
    git(&root, &["add", "."]);
    git(&root, &["commit", "-m", "metadata base"]);
    git(&root, &["tag", "-a", "baseline", "-m", "baseline"]);

    fs::write(
        root.join("src/Catalogs/Контрагенты.xml"),
        catalog_descriptor("Изменённый"),
    )
    .expect("modify catalog descriptor");
    let form_module = root.join("src/Documents/Приход/Forms/ФормаДокумента/Ext/Form/Module.bsl");
    fs::create_dir_all(form_module.parent().unwrap()).expect("create form directory");
    fs::write(&form_module, "Процедура ПриОткрытии()\nКонецПроцедуры\n")
        .expect("write form module");
    fs::write(root.join("notes.txt"), "committed\n").expect("write ordinary file");
    git(&root, &["add", "."]);
    git(&root, &["commit", "-m", "metadata changes"]);
    fs::write(root.join("src/uncommitted.bsl"), "local\n").expect("write local-only file");

    for (locale, heading, modified, added, catalog, form) in [
        (
            "ru",
            "Изменения",
            "Изменены (1):",
            "Добавлены (1):",
            "Справочник.Контрагенты.Реквизит.Реквизит1",
            "Документ.Приход.Форма.ФормаДокумента",
        ),
        (
            "en",
            "Changes",
            "Modified (1):",
            "Added (1):",
            "Catalog.Контрагенты.Attribute.Реквизит1",
            "Document.Приход.Form.ФормаДокумента",
        ),
    ] {
        let output = eska(&root, locale, &["diff", "baseline"]);
        assert!(output.status.success(), "{output:?}");
        let text = String::from_utf8(output.stdout).expect("UTF-8 revision diff");
        for expected in [
            heading,
            "baseline",
            "HEAD",
            modified,
            added,
            catalog,
            form,
            "notes.txt",
        ] {
            assert!(text.contains(expected), "missing `{expected}` in:\n{text}");
        }
        assert!(text.contains(&format!("✎ {catalog}")), "{text}");
        assert!(text.contains(&format!("+ {form}")), "{text}");
        assert!(!text.contains("uncommitted.bsl"), "{text}");
    }

    let raw = eska(&root, "ru", &["diff", "baseline", "HEAD", "--raw"]);
    assert!(raw.status.success(), "{raw:?}");
    assert_eq!(
        String::from_utf8(raw.stdout).unwrap(),
        concat!(
            "A\tnotes.txt\n",
            "M\tsrc/Catalogs/Контрагенты.xml\n",
            "A\tsrc/Documents/Приход/Forms/ФормаДокумента/Ext/Form/Module.bsl\n"
        )
    );

    let json = eska(&root, "en", &["diff", "baseline", "--format", "json"]);
    assert!(json.status.success(), "{json:?}");
    let document: Value = serde_json::from_slice(&json.stdout).expect("valid revision JSON");
    assert_eq!(document["schema_version"], 2);
    assert_eq!(document["comparison"]["kind"], "revisions");
    assert_eq!(document["comparison"]["strategy"], "direct");
    assert_eq!(document["comparison"]["from"]["revision"], "baseline");
    assert_eq!(document["comparison"]["to"]["revision"], "HEAD");
    assert_eq!(document["comparison"]["merge_base_commit"], Value::Null);
    assert_eq!(document["files"].as_array().unwrap().len(), 3);
    assert_eq!(document["files"][1]["change"], "modified");
    assert!(document["files"][1].get("index").is_none());
    assert!(document["files"][1].get("worktree").is_none());
}

/// Branch-point mode excludes changes made only on the comparison branch.
#[test]
fn revision_diff_can_start_at_the_branch_point() {
    let (_fixture, root) = project();
    git(&root, &["checkout", "-b", "feature"]);
    fs::write(root.join("src/feature.bsl"), "feature\n").unwrap();
    git(&root, &["add", "."]);
    git(&root, &["commit", "-m", "feature"]);
    git(&root, &["checkout", "main"]);
    fs::write(root.join("src/main.bsl"), "main\n").unwrap();
    git(&root, &["add", "."]);
    git(&root, &["commit", "-m", "main"]);
    git(&root, &["checkout", "feature"]);

    let direct = eska(&root, "en", &["diff", "main", "--raw"]);
    assert_eq!(
        String::from_utf8(direct.stdout).unwrap(),
        "A\tsrc/feature.bsl\nD\tsrc/main.bsl\n"
    );
    let branch = eska(
        &root,
        "en",
        &["diff", "main", "--since-branch-point", "--raw"],
    );
    assert_eq!(
        String::from_utf8(branch.stdout).unwrap(),
        "A\tsrc/feature.bsl\n"
    );

    let json = eska(
        &root,
        "ru",
        &["diff", "main", "--since-branch-point", "--format", "json"],
    );
    let document: Value = serde_json::from_slice(&json.stdout).expect("valid branch JSON");
    assert_eq!(document["comparison"]["strategy"], "merge-base");
    assert!(document["comparison"]["merge_base_commit"].is_string());
}

/// Invalid revisions and incomplete branch-point requests fail without Git mutation.
#[test]
fn revision_diff_errors_are_localized_and_usage_is_bounded() {
    let (_fixture, root) = project();
    for (locale, expected) in [
        ("ru", "Не удалось разрешить Git-ревизию"),
        ("en", "Could not resolve Git revision"),
    ] {
        let output = eska(&root, locale, &["diff", "missing"]);
        assert_eq!(output.status.code(), Some(1), "{output:?}");
        assert!(output.stdout.is_empty());
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains(expected), "{stderr}");
        assert!(stderr.contains("missing"), "{stderr}");
    }
    let missing_base = eska(&root, "en", &["diff", "--since-branch-point"]);
    assert_eq!(missing_base.status.code(), Some(2), "{missing_base:?}");
    let too_many = eska(&root, "en", &["diff", "HEAD", "HEAD", "HEAD"]);
    assert_eq!(too_many.status.code(), Some(2), "{too_many:?}");
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
