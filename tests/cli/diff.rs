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

/// Run redirected semantic output with explicit color suppression.
fn eska_no_color(current_dir: &Path, locale: &str, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_eska"))
        .current_dir(current_dir)
        .env_remove("ESKA_LANG")
        .env("NO_COLOR", "1")
        .args(["--lang", locale])
        .args(args)
        .output()
        .expect("run eska without color")
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

fn workspace() -> (TestDir, PathBuf, PathBuf) {
    let fixture = TestDir::new();
    let root = fixture.0.join("tools");
    let report = root.join("src/sales-report");
    let processing = root.join("src/import-orders");
    fs::create_dir_all(&report).expect("report member");
    fs::create_dir_all(&processing).expect("processing member");
    fs::write(
        root.join("eska.toml"),
        "[workspace]\nmembers = ['src/sales-report', 'src/import-orders']\n",
    )
    .expect("workspace config");
    fs::write(
        report.join("eska.toml"),
        "[project]\nname = 'sales-report'\ntype = 'report'\nsource = '.'\n",
    )
    .expect("report config");
    fs::write(
        report.join("SalesReport.xml"),
        r#"<MetaDataObject xmlns="http://v8.1c.ru/8.3/MDClasses"><ExternalReport uuid="00000000-0000-0000-0000-000000000001"><Properties><Name>SalesReport</Name></Properties></ExternalReport></MetaDataObject>"#,
    )
    .expect("report source");
    fs::write(
        processing.join("eska.toml"),
        "[project]\nname = 'import-orders'\ntype = 'processing'\nsource = '.'\n",
    )
    .expect("processing config");
    // This unchanged member must never reach object-model parsing in semantic workspace mode.
    fs::write(processing.join("ImportOrders.xml"), "<broken>").expect("processing source");
    fs::write(root.join("README.md"), "base\n").expect("workspace file");
    fs::write(fixture.0.join("outside.txt"), "base\n").expect("repository sibling");
    git(
        &fixture.0,
        &["init", "--initial-branch=main", "--template="],
    );
    git(&fixture.0, &["add", "."]);
    git(&fixture.0, &["commit", "-m", "base"]);
    (fixture, root, report)
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

#[test]
fn workspace_diff_groups_members_and_workspace_files_in_every_output_mode() {
    let (fixture, root, report) = workspace();
    fs::write(report.join("module.bsl"), "new\n").expect("report change");
    fs::write(root.join("src/import-orders/new.bsl"), "new\n").expect("processing change");
    fs::write(root.join("README.md"), "changed\n").expect("workspace change");
    fs::write(fixture.0.join("outside.txt"), "changed\n").expect("outside change");

    for locale in ["ru", "en"] {
        let output = eska(&root, locale, &["diff", "--format", "json"]);
        assert!(output.status.success(), "{output:?}");
        assert!(output.stderr.is_empty(), "{output:?}");
        let document: Value = serde_json::from_slice(&output.stdout).expect("workspace JSON");
        assert_eq!(document["schema_version"], 1);
        assert_eq!(document["projects"][0]["name"], "sales-report");
        assert_eq!(document["projects"][0]["files"][0]["path"], "module.bsl");
        assert_eq!(document["projects"][1]["name"], "import-orders");
        assert_eq!(document["workspace_files"][0]["path"], "README.md");
        assert!(!String::from_utf8_lossy(&output.stdout).contains("outside.txt"));
    }

    let raw = eska(&root, "en", &["diff", "--raw"]);
    assert!(raw.status.success(), "{raw:?}");
    let raw = String::from_utf8(raw.stdout).expect("raw output");
    assert!(raw.contains("sales-report\t.?\tmodule.bsl\n"), "{raw}");
    assert!(raw.contains("import-orders\t.?\tnew.bsl\n"), "{raw}");
    assert!(raw.contains("-\t.M\tREADME.md\n"), "{raw}");

    let human = eska(&root, "ru", &["diff"]);
    let human = String::from_utf8(human.stdout)
        .expect("human output")
        .replace(['\u{2068}', '\u{2069}'], "");
    assert!(human.contains("Проект «sales-report»"), "{human}");
    assert!(human.contains("Файлы workspace"), "{human}");
}

#[test]
fn workspace_diff_preserves_single_json_and_supports_revisions_and_semantics() {
    let (_fixture, root, report) = workspace();
    fs::write(report.join("module.bsl"), "new\n").expect("report change");
    fs::write(root.join("README.md"), "changed\n").expect("workspace change");

    let single = eska(&report, "en", &["diff", "--format", "json"]);
    assert!(single.status.success(), "{single:?}");
    let document: Value = serde_json::from_slice(&single.stdout).expect("single JSON");
    assert!(document.get("projects").is_none());
    assert_eq!(document["files"][0]["path"], "module.bsl");

    let semantic = eska(&root, "en", &["diff", "--semantic", "--format", "json"]);
    assert!(semantic.status.success(), "{semantic:?}");
    let document: Value = serde_json::from_slice(&semantic.stdout).expect("semantic JSON");
    assert_eq!(document["schema_version"], 4);
    assert_eq!(document["kind"], "semantic_workspace");
    assert_eq!(document["projects"][0]["name"], "sales-report");
    assert_eq!(document["projects"][0]["analysis"]["complete"], true);
    assert_eq!(document["projects"][0]["analysis"]["fallbacks"], json!([]));
    assert_eq!(document["projects"][1]["analysis"]["complete"], true);
    assert_eq!(document["projects"][1]["analysis"]["fallbacks"], json!([]));
    assert!(semantic.stderr.is_empty(), "{semantic:?}");
    assert_eq!(document["workspace_files"]["kind"], "workspace");

    git(&root, &["add", "."]);
    git(&root, &["commit", "-m", "workspace changes"]);
    let revisions = eska(&root, "en", &["diff", "HEAD^", "HEAD", "--format", "json"]);
    assert!(revisions.status.success(), "{revisions:?}");
    let document: Value = serde_json::from_slice(&revisions.stdout).expect("revision JSON");
    assert_eq!(document["schema_version"], 2);
    assert_eq!(document["projects"][0]["name"], "sales-report");
    assert_eq!(document["workspace_files"][0]["path"], "README.md");
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

        let semantic = eska(&root, locale, &["diff", "--semantic"]);
        assert!(semantic.status.success(), "{semantic:?}");
        let expected = if locale == "ru" {
            "Семантических изменений нет"
        } else {
            "No semantic changes"
        };
        assert!(String::from_utf8_lossy(&semantic.stdout).contains(expected));
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

/// Semantic mode exposes stable JSON events while human labels remain localized.
#[test]
fn semantic_workspace_diff_reports_objects_modules_routines_forms_and_attributes() {
    let (_fixture, root) = semantic_project();
    apply_semantic_changes(&root);

    let mut documents = Vec::new();
    for locale in ["ru", "en"] {
        let output = eska(&root, locale, &["diff", "--semantic", "--format", "json"]);
        assert!(output.status.success(), "{output:?}");
        assert!(output.stderr.is_empty(), "{output:?}");
        documents.push(serde_json::from_slice::<Value>(&output.stdout).expect("semantic JSON"));
    }
    assert_eq!(documents[0], documents[1]);
    let document = &documents[0];
    assert_eq!(document["schema_version"], 4);
    assert_eq!(document["kind"], "semantic");
    assert_eq!(document["comparison"]["kind"], "workspace");
    assert_eq!(document["analysis"]["complete"], true);
    assert_eq!(document["analysis"]["fallbacks"], json!([]));
    let events = document["events"].as_array().expect("events");
    let kinds: Vec<_> = events
        .iter()
        .map(|event| event["kind"].as_str().expect("event kind"))
        .collect();
    for kind in [
        "object_added",
        "object_changed",
        "module_changed",
        "method_changed",
        "function_changed",
        "form_changed",
        "metadata_attribute_changed",
    ] {
        assert!(kinds.contains(&kind), "missing {kind}: {document}");
    }
    let method = events
        .iter()
        .find(|event| event["kind"] == "method_changed")
        .expect("method event");
    assert_eq!(method["member"], "Выполнить");
    assert_eq!(method["object"]["id"], "common-module:ОбщийМодуль1");
    assert_eq!(method["stage"], "worktree");
    assert!(
        method.get("line").is_none(),
        "JSON schema must stay unchanged"
    );
    assert!(
        method.get("column").is_none(),
        "JSON schema must stay unchanged"
    );

    for (locale, header, group, method_group, method) in [
        (
            "ru",
            "Семантические изменения",
            "ОбщийМодуль:",
            "Изменён метод — в рабочей копии (2):",
            "    ✎ ОбщийМодуль.ОбщийМодуль1 — Процедура.Выполнить (1, 1)",
        ),
        (
            "en",
            "Semantic changes",
            "CommonModule:",
            "Method changed — in working tree (2):",
            "    ✎ CommonModule.ОбщийМодуль1 — Procedure.Выполнить (1, 1)",
        ),
    ] {
        let output = eska(&root, locale, &["diff", "--semantic"]);
        assert!(output.status.success(), "{output:?}");
        let text = String::from_utf8(output.stdout).expect("human semantic diff");
        for expected in [header, group, method_group, method] {
            assert!(text.contains(expected), "missing `{expected}` in:\n{text}");
        }
        assert!(!text.contains("\x1b["), "{text:?}");
        assert!(!text.contains("src/CommonModules"), "{text}");
    }

    let raw = eska(&root, "ru", &["diff", "--semantic", "--raw"]);
    assert!(raw.status.success(), "{raw:?}");
    let raw = String::from_utf8(raw.stdout).expect("raw semantic diff");
    assert!(
        raw.contains(
            "worktree\tmethod_changed\tcommon-module:ОбщийМодуль1\tВыполнить\tsrc/CommonModules/ОбщийМодуль1/Ext/Module.bsl"
        ),
        "{raw}"
    );

    let no_color = eska_no_color(&root, "ru", &["diff", "--semantic"]);
    assert!(no_color.status.success(), "{no_color:?}");
    assert!(!no_color.stdout.contains(&b'\x1b'), "{no_color:?}");
}

/// Added and removed methods use coordinates from their respective source snapshots.
#[test]
fn semantic_method_lifecycle_reports_declaration_kind_and_coordinates() {
    let (_fixture, root) = semantic_project();
    fs::write(
        root.join("src/CommonModules/ОбщийМодуль1/Ext/Module.bsl"),
        concat!(
            "Процедура Выполнить()\n    Сообщить(\"Исходный\");\nКонецПроцедуры\n",
            "  Процедура Добавить()\nКонецПроцедуры\n"
        ),
    )
    .expect("change routine lifecycle");

    for (locale, added, removed) in [
        (
            "ru",
            "Добавлен метод — в рабочей копии (1):\n    + ОбщийМодуль.ОбщийМодуль1 — Процедура.Добавить (4, 3)",
            "Удалён метод — в рабочей копии (1):\n    − ОбщийМодуль.ОбщийМодуль1 — Функция.ПолучитьЗначение (4, 1)",
        ),
        (
            "en",
            "Method added — in working tree (1):\n    + CommonModule.ОбщийМодуль1 — Procedure.Добавить (4, 3)",
            "Method removed — in working tree (1):\n    − CommonModule.ОбщийМодуль1 — Function.ПолучитьЗначение (4, 1)",
        ),
    ] {
        let output = eska(&root, locale, &["diff", "--semantic"]);
        assert!(output.status.success(), "{output:?}");
        let text = String::from_utf8(output.stdout).expect("semantic human output");
        assert!(text.contains(added), "missing `{added}` in:\n{text}");
        assert!(text.contains(removed), "missing `{removed}` in:\n{text}");
    }
}

/// Changed methods follow source coordinates instead of declaration names.
#[test]
fn semantic_methods_follow_source_order() {
    let (_fixture, root) = semantic_project_with_module(concat!(
        "Функция ЯПервая()\n    Возврат 1;\nКонецФункции\n",
        "Процедура АВторая()\n    Сообщить(\"Исходный\");\nКонецПроцедуры\n"
    ));
    let module = root.join("src/CommonModules/ОбщийМодуль1/Ext/Module.bsl");
    fs::write(
        module,
        concat!(
            "Функция ЯПервая()\n    Возврат 2;\nКонецФункции\n",
            "Процедура АВторая()\n    Сообщить(\"Изменённый\");\nКонецПроцедуры\n"
        ),
    )
    .expect("change methods in ordering fixture");

    for (locale, first, second) in [
        (
            "ru",
            "ОбщийМодуль.ОбщийМодуль1 — Функция.ЯПервая (1, 1)",
            "ОбщийМодуль.ОбщийМодуль1 — Процедура.АВторая (4, 1)",
        ),
        (
            "en",
            "CommonModule.ОбщийМодуль1 — Function.ЯПервая (1, 1)",
            "CommonModule.ОбщийМодуль1 — Procedure.АВторая (4, 1)",
        ),
    ] {
        let output = eska(&root, locale, &["diff", "--semantic"]);
        assert!(output.status.success(), "{output:?}");
        let text = String::from_utf8(output.stdout).expect("semantic human output");
        let first = text.find(first).expect("first method in output");
        let second = text.find(second).expect("second method in output");
        assert!(
            first < second,
            "methods do not follow source order:\n{text}"
        );
    }
}

/// Equal index/worktree events share one compact human subgroup without changing JSON events.
#[test]
fn semantic_human_combines_identical_index_and_worktree_events() {
    let (_fixture, root) = semantic_project();
    let module = root.join("src/CommonModules/ОбщийМодуль1/Ext/Module.bsl");
    fs::write(
        &module,
        "Процедура Выполнить()\n    Сообщить(\"Индекс\");\nКонецПроцедуры\nФункция ПолучитьЗначение()\n    Возврат 1;\nКонецФункции\n",
    )
    .expect("write index version");
    git(
        &root,
        &["add", "src/CommonModules/ОбщийМодуль1/Ext/Module.bsl"],
    );
    fs::write(
        module,
        "Процедура Выполнить()\n    Сообщить(\"Рабочая копия\");\nКонецПроцедуры\nФункция ПолучитьЗначение()\n    Возврат 1;\nКонецФункции\n",
    )
    .expect("write worktree version");

    let human = eska(&root, "ru", &["diff", "--semantic"]);
    assert!(human.status.success(), "{human:?}");
    let text = String::from_utf8(human.stdout).expect("semantic human output");
    assert!(
        text.contains("Изменён метод — в индексе и рабочей копии (1):"),
        "{text}"
    );

    let json = eska(&root, "en", &["diff", "--semantic", "--format", "json"]);
    let document: Value = serde_json::from_slice(&json.stdout).expect("semantic JSON");
    assert_eq!(
        document["events"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|event| event["kind"] == "method_changed")
            .count(),
        2
    );
}

/// Object lifecycle suppresses derived module/form events only for the exact same identity.
#[test]
fn semantic_added_form_suppresses_derived_events_for_that_form() {
    let (_fixture, root) = semantic_project();
    let form = root.join("src/Documents/Возврат/Forms/ФормаСписка");
    fs::create_dir_all(form.join("Ext/Form")).expect("create form source");
    fs::write(
        form.with_extension("xml"),
        semantic_object_descriptor(
            "Form",
            "ФормаСписка",
            "66666666-6666-6666-6666-666666666666",
        ),
    )
    .expect("write form descriptor");
    fs::write(form.join("Ext/Form.xml"), "<Form/>\n").expect("write managed form");
    fs::write(
        form.join("Ext/Form/Module.bsl"),
        "Процедура ПриОткрытии()\nКонецПроцедуры\n",
    )
    .expect("write form module");

    let output = eska(&root, "en", &["diff", "--semantic", "--format", "json"]);
    assert!(output.status.success(), "{output:?}");
    let document: Value = serde_json::from_slice(&output.stdout).expect("semantic JSON");
    let events = document["events"].as_array().expect("events");
    let form_events = events
        .iter()
        .filter(|event| event["object"]["id"] == "document:Возврат/form:ФормаСписка")
        .collect::<Vec<_>>();
    assert_eq!(form_events.len(), 1, "{document}");
    assert_eq!(form_events[0]["kind"], "object_added");
}

/// Semantic sections follow Configurator order rather than localized alphabetical order.
#[test]
fn semantic_groups_follow_configurator_order_in_both_locales() {
    let (_fixture, root) = semantic_project();
    apply_semantic_changes(&root);
    for (directory, file, tag, name, uuid) in [
        (
            "Constants",
            "Организация.xml",
            "Constant",
            "Организация",
            "77777777-7777-7777-7777-777777777777",
        ),
        (
            "Documents",
            "Заказ.xml",
            "Document",
            "Заказ",
            "88888888-8888-8888-8888-888888888888",
        ),
    ] {
        fs::create_dir_all(root.join("src").join(directory)).expect("create metadata directory");
        fs::write(
            root.join("src").join(directory).join(file),
            semantic_object_descriptor(tag, name, uuid),
        )
        .expect("write metadata descriptor");
    }

    for (locale, groups) in [
        (
            "ru",
            [
                "ОбщийМодуль:",
                "ОбщаяФорма:",
                "Константа:",
                "Справочник:",
                "Документ:",
            ],
        ),
        (
            "en",
            [
                "CommonModule:",
                "CommonForm:",
                "Constant:",
                "Catalog:",
                "Document:",
            ],
        ),
    ] {
        let output = eska(&root, locale, &["diff", "--semantic"]);
        let text = String::from_utf8(output.stdout).expect("semantic human output");
        let positions = groups.map(|group| text.find(group).unwrap_or_else(|| panic!("{text}")));
        assert!(positions.windows(2).all(|pair| pair[0] < pair[1]), "{text}");
    }
}

/// One malformed descriptor degrades only its exact path and preserves independent events.
#[test]
fn semantic_json_reports_descriptor_fallback_without_cancelling_other_objects() {
    let (_fixture, root) = semantic_project();
    fs::write(
        root.join("src/Catalogs/Контрагенты.xml"),
        "<MetaDataObject>",
    )
    .expect("break catalog descriptor");
    fs::write(
        root.join("src/CommonModules/ОбщийМодуль1/Ext/Module.bsl"),
        concat!(
            "Процедура Выполнить()\n    Сообщить(\"Изменённый\");\nКонецПроцедуры\n",
            "Функция ПолучитьЗначение()\n    Возврат 1;\nКонецФункции\n"
        ),
    )
    .expect("change independent module");

    let mut documents = Vec::new();
    for (locale, warning) in [
        ("ru", "дескриптор не удалось разобрать"),
        ("en", "the descriptor could not be parsed"),
    ] {
        let output = eska(&root, locale, &["diff", "--semantic", "--format", "json"]);
        assert!(output.status.success(), "{output:?}");
        let stderr = String::from_utf8_lossy(&output.stderr).replace(['\u{2068}', '\u{2069}'], "");
        assert!(stderr.contains(warning), "{stderr}");
        assert!(stderr.contains("src/Catalogs/Контрагенты.xml"), "{stderr}");
        documents.push(serde_json::from_slice::<Value>(&output.stdout).expect("semantic JSON"));
    }

    assert_eq!(documents[0], documents[1]);
    let document = &documents[0];
    assert_eq!(document["analysis"]["complete"], false);
    assert_eq!(
        document["analysis"]["fallbacks"],
        json!([{
            "reason": "descriptor-parse",
            "stage": "worktree",
            "path": "src/Catalogs/Контрагенты.xml",
            "path_encoding": "utf-8"
        }])
    );
    let events = document["events"].as_array().expect("events");
    assert!(events.iter().any(|event| {
        event["kind"] == "method_changed" && event["object"]["id"] == "common-module:ОбщийМодуль1"
    }));
    assert!(events.iter().any(|event| {
        event["kind"] == "object_changed" && event["object"]["id"] == "catalog:Контрагенты"
    }));
}

/// An incomplete BSL routine keeps the module event and exposes its exact fallback.
#[test]
fn semantic_json_reports_incomplete_bsl_at_module_level() {
    let (_fixture, root) = semantic_project();
    fs::write(
        root.join("src/CommonModules/ОбщийМодуль1/Ext/Module.bsl"),
        "Процедура Выполнить()\n",
    )
    .expect("write incomplete BSL");

    let output = eska(&root, "en", &["diff", "--semantic", "--format", "json"]);
    assert!(output.status.success(), "{output:?}");
    let document: Value = serde_json::from_slice(&output.stdout).expect("semantic JSON");
    assert_eq!(document["analysis"]["complete"], false);
    assert_eq!(
        document["analysis"]["fallbacks"][0]["reason"],
        "routine-parse"
    );
    assert_eq!(
        document["analysis"]["fallbacks"][0]["path"],
        "src/CommonModules/ОбщийМодуль1/Ext/Module.bsl"
    );
    assert!(
        document["events"]
            .as_array()
            .expect("events")
            .iter()
            .any(|event| event["kind"] == "module_changed")
    );
}

/// Descriptor deletion and move preserve inline and standalone object identities.
#[test]
fn semantic_diff_preserves_identities_across_descriptor_deletion_and_move() {
    let (_fixture, root) = semantic_project();
    fs::remove_file(root.join("src/Catalogs/Контрагенты.xml")).expect("delete catalog descriptor");
    fs::rename(
        root.join("src/CommonForms/Основная.xml"),
        root.join("src/CommonForms/Перемещённая.xml"),
    )
    .expect("move standalone form descriptor");

    let output = eska(&root, "en", &["diff", "--semantic", "--format", "json"]);
    assert!(output.status.success(), "{output:?}");
    assert!(output.stderr.is_empty(), "{output:?}");
    let document: Value = serde_json::from_slice(&output.stdout).expect("semantic JSON");
    assert_eq!(document["analysis"]["complete"], true);
    let events = document["events"].as_array().expect("events");
    for (kind, id) in [
        ("object_removed", "catalog:Контрагенты"),
        ("object_removed", "catalog:Контрагенты/attribute:Реквизит1"),
        ("object_removed", "common-form:Основная"),
        ("object_added", "common-form:Основная"),
    ] {
        assert!(
            events
                .iter()
                .any(|event| event["kind"] == kind && event["object"]["id"] == id),
            "missing {kind} for {id}: {document}"
        );
    }
}

/// Committed semantic comparison uses revision stages and explicit endpoints.
#[test]
fn semantic_revision_diff_has_a_separate_versioned_comparison() {
    let (_fixture, root) = semantic_project();
    apply_semantic_changes(&root);
    git(&root, &["add", "."]);
    git(&root, &["commit", "-m", "semantic changes"]);

    let output = eska(
        &root,
        "en",
        &["diff", "HEAD~1", "HEAD", "--semantic", "--format", "json"],
    );
    assert!(output.status.success(), "{output:?}");
    let document: Value = serde_json::from_slice(&output.stdout).expect("semantic revision JSON");
    assert_eq!(document["schema_version"], 4);
    assert_eq!(document["comparison"]["kind"], "revisions");
    assert_eq!(document["analysis"]["complete"], true);
    assert_eq!(document["comparison"]["strategy"], "direct");
    assert_eq!(document["comparison"]["from"]["revision"], "HEAD~1");
    assert_eq!(document["comparison"]["to"]["revision"], "HEAD");
    assert!(
        document["events"]
            .as_array()
            .expect("events")
            .iter()
            .all(|event| event["stage"] == "revision")
    );

    for (locale, expected) in [
        ("ru", "Изменён метод — между Git-ревизиями (2):"),
        ("en", "Method changed — between Git revisions (2):"),
    ] {
        let output = eska(&root, locale, &["diff", "HEAD~1", "HEAD", "--semantic"]);
        assert!(output.status.success(), "{output:?}");
        let text = String::from_utf8(output.stdout).expect("semantic human output");
        assert!(text.contains(expected), "missing `{expected}` in:\n{text}");
        assert!(!text.contains(" — revisions"), "{text}");
        assert!(!text.contains(" — ревизии"), "{text}");
    }
}

/// Create committed Designer sources accepted by the logical object model.
fn semantic_project() -> (TestDir, PathBuf) {
    semantic_project_with_module(concat!(
        "Процедура Выполнить()\n    Сообщить(\"Исходный\");\nКонецПроцедуры\n",
        "Функция ПолучитьЗначение()\n    Возврат 1;\nКонецФункции\n"
    ))
}

/// Create committed Designer sources with a caller-provided common module.
fn semantic_project_with_module(module_source: &str) -> (TestDir, PathBuf) {
    let (fixture, root) = project();
    for directory in [
        "src/Catalogs",
        "src/CommonModules/ОбщийМодуль1/Ext",
        "src/CommonForms/Основная/Ext",
    ] {
        fs::create_dir_all(root.join(directory)).expect("create semantic source directory");
    }
    fs::write(
        root.join("src/Catalogs/Контрагенты.xml"),
        semantic_catalog_descriptor("Исходный"),
    )
    .expect("write semantic catalog");
    fs::write(
        root.join("src/CommonModules/ОбщийМодуль1.xml"),
        semantic_common_module_descriptor(),
    )
    .expect("write semantic common module descriptor");
    fs::write(
        root.join("src/CommonModules/ОбщийМодуль1/Ext/Module.bsl"),
        module_source,
    )
    .expect("write semantic module");
    fs::write(
        root.join("src/CommonForms/Основная.xml"),
        semantic_common_form_descriptor(),
    )
    .expect("write semantic form descriptor");
    fs::write(
        root.join("src/CommonForms/Основная/Ext/Form.xml"),
        "<Form><Title>Исходная</Title></Form>\n",
    )
    .expect("write semantic form");
    git(&root, &["add", "."]);
    git(&root, &["commit", "-m", "semantic base"]);
    (fixture, root)
}

/// Apply representative uncommitted changes for every initial T21 event family.
fn apply_semantic_changes(root: &Path) {
    fs::write(
        root.join("src/Catalogs/Контрагенты.xml"),
        semantic_catalog_descriptor("Изменённый"),
    )
    .expect("change semantic catalog");
    fs::write(
        root.join("src/CommonModules/ОбщийМодуль1/Ext/Module.bsl"),
        concat!(
            "Процедура Выполнить()\n    Сообщить(\"Изменённый\");\nКонецПроцедуры\n",
            "Функция ПолучитьЗначение()\n    Возврат 2;\nКонецФункции\n"
        ),
    )
    .expect("change semantic module");
    fs::write(
        root.join("src/CommonForms/Основная/Ext/Form.xml"),
        "<Form><Title>Изменённая</Title></Form>\n",
    )
    .expect("change semantic form");
    fs::write(
        root.join("src/Catalogs/Новый.xml"),
        semantic_object_descriptor("Catalog", "Новый", "55555555-5555-5555-5555-555555555555"),
    )
    .expect("add semantic object");
}

/// Build a catalog with an independently addressable attribute.
fn semantic_catalog_descriptor(comment: &str) -> String {
    format!(
        r#"<MetaDataObject xmlns="http://v8.1c.ru/8.3/MDClasses"><Catalog uuid="11111111-1111-1111-1111-111111111111"><Properties><Name>Контрагенты</Name></Properties><ChildObjects><Attribute uuid="22222222-2222-2222-2222-222222222222"><Properties><Name>Реквизит1</Name><Comment>{comment}</Comment></Properties></Attribute></ChildObjects></Catalog></MetaDataObject>"#
    )
}

/// Build a valid common-module descriptor for semantic fixtures.
fn semantic_common_module_descriptor() -> String {
    semantic_object_descriptor(
        "CommonModule",
        "ОбщийМодуль1",
        "33333333-3333-3333-3333-333333333333",
    )
}

/// Build a valid common-form descriptor for semantic fixtures.
fn semantic_common_form_descriptor() -> String {
    semantic_object_descriptor(
        "CommonForm",
        "Основная",
        "44444444-4444-4444-4444-444444444444",
    )
}

/// Build one minimal valid Designer metadata object descriptor.
fn semantic_object_descriptor(kind: &str, name: &str, uuid: &str) -> String {
    format!(
        r#"<MetaDataObject xmlns="http://v8.1c.ru/8.3/MDClasses"><{kind} uuid="{uuid}"><Properties><Name>{name}</Name></Properties></{kind}></MetaDataObject>"#
    )
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
