use super::*;

/// A placement conflict remains a localized, read-only refusal in both locales.
#[test]
fn migration_refuses_to_move_conditional_variables_after_a_new_function() {
    let fixture = workspace("1.2.3", "2.3.4.5");
    let root = fixture.0.join("src/sales-report");
    let path = root.join("SalesReport/Ext/ObjectModule.bsl");
    let input =
        "\u{feff}#Если Сервер Тогда\r\nПерем Состояние;\r\nСостояние = 1;\r\n#КонецЕсли\r\n";
    fs::write(&path, input).unwrap();
    let mut documents = Vec::new();
    for (locale, reason) in [
        ("ru", "не позволяют безопасно вставить"),
        ("en", "prevent safe insertion"),
    ] {
        let output = eska(
            &root,
            locale,
            &[
                "version",
                "migrate",
                "--initial-version",
                "1.2.3",
                "--write",
                "--format",
                "json",
            ],
        );
        assert_eq!(output.status.code(), Some(1), "{output:?}");
        let document: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(
            document["error"]["code"],
            "version_migration_initialization_conflict"
        );
        assert_eq!(document["error"]["path"], path.to_string_lossy().as_ref());
        assert!(String::from_utf8_lossy(&output.stderr).contains(reason));
        documents.push(document);
        assert_eq!(fs::read_to_string(&path).unwrap(), input);
    }
    assert_eq!(documents[0], documents[1]);
}

/// Migration writes only one explicitly selected member and keeps registration's unrelated literals.
#[test]
fn migration_preview_and_write_are_locale_independent_and_idempotent() {
    let fixture = workspace("1.2.3", "2.3.4.5");
    let root = fixture.0.join("src/sales-report");
    let path = root.join("SalesReport/Ext/ObjectModule.bsl");
    let input = "\u{feff}// Сохранить\r\nФункция СведенияОВнешнейОбработке() Экспорт\r\nДанные = Новый Структура; Данные.Вставить(\"Версия\", \"1.2.3\"); Данные.Информация = \"1.2.3\"; Возврат Данные; КонецФункции\r\n";
    fs::write(&path, input).unwrap();
    let other_path = fixture
        .0
        .join("src/import-orders/ImportOrders/Ext/ObjectModule.bsl");
    let other = fs::read(&other_path).unwrap();
    let mut previews = Vec::new();
    for locale in ["ru", "en"] {
        let output = eska(
            &root,
            locale,
            &[
                "version",
                "migrate",
                "--initial-version",
                "1.2.3",
                "--format",
                "json",
            ],
        );
        assert!(output.status.success(), "{output:?}");
        assert!(output.stderr.is_empty());
        previews.push(serde_json::from_slice::<Value>(&output.stdout).unwrap());
        assert_eq!(fs::read_to_string(&path).unwrap(), input);
    }
    assert_eq!(previews[0], previews[1]);
    assert_eq!(previews[0]["schema_version"], 1);
    assert_eq!(previews[0]["operation"], "version-migration");
    assert_eq!(previews[0]["applied"], false);
    assert_eq!(previews[0]["version_scheme"], "x.y.z");
    assert_eq!(previews[0]["edits"][0]["kind"], "bind_registration");
    assert_eq!(previews[0]["edits"][0]["original"], "\"1.2.3\"");
    assert_eq!(previews[0]["edits"][0]["replacement"], "Версия()");
    let output = eska(
        &root,
        "ru",
        &[
            "version",
            "migrate",
            "--initial-version",
            "1.2.3",
            "--write",
            "--format",
            "json",
        ],
    );
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        serde_json::from_slice::<Value>(&output.stdout).unwrap()["applied"],
        true
    );
    let written = fs::read_to_string(&path).unwrap();
    assert!(written.starts_with(&input.replace("\"Версия\", \"1.2.3\"", "\"Версия\", Версия()")));
    for locale in ["ru", "en"] {
        let output = eska(
            &root,
            locale,
            &["version", "migrate", "--write", "--format", "json"],
        );
        assert!(output.status.success(), "{output:?}");
        let document: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(document["applied"], false);
        assert!(document["edits"].as_array().unwrap().is_empty());
        assert_eq!(fs::read_to_string(&path).unwrap(), written);
    }
    assert_eq!(fs::read(other_path).unwrap(), other);
}

/// Missing and unsuitable declarations expose exact diagnostics and never modify the source.
#[test]
fn migration_failures_include_path_range_and_remediation_in_both_locales() {
    for (input, code) in [
        (
            "// Модуль без функции\r\n",
            "version_initial_value_required",
        ),
        (
            "Функция Версия() Возврат Номер; КонецФункции",
            "version_function_invalid_body",
        ),
        (
            "#Если Сервер Тогда\nФункция Версия() Экспорт Возврат \"1.2.3\"; КонецФункции\n#КонецЕсли",
            "version_function_conditional",
        ),
        (
            "Функция Версия() Экспорт Возврат \"1.2.3\"; КонецФункции\nПроцедура Ошибка( КонецПроцедуры",
            "version_module_invalid_syntax",
        ),
    ] {
        let fixture = workspace("1.2.3", "2.3.4.5");
        let root = fixture.0.join("src/sales-report");
        let path = root.join("SalesReport/Ext/ObjectModule.bsl");
        fs::write(&path, input).unwrap();
        let mut documents = Vec::new();
        for locale in ["ru", "en"] {
            let output = eska(
                &root,
                locale,
                &["version", "migrate", "--write", "--format", "json"],
            );
            assert_eq!(output.status.code(), Some(1), "{output:?}");
            let document: Value = serde_json::from_slice(&output.stdout).unwrap();
            assert_eq!(document["error"]["code"], code);
            assert_eq!(document["error"]["path"], path.to_string_lossy().as_ref());
            let start = usize::try_from(document["error"]["range"]["byte_start"].as_u64().unwrap())
                .unwrap();
            let end =
                usize::try_from(document["error"]["range"]["byte_end"].as_u64().unwrap()).unwrap();
            assert!(input.get(start..end).is_some());
            assert!(document["error"]["remediation"].is_string());
            let stderr = String::from_utf8_lossy(&output.stderr);
            assert!(stderr.contains("ObjectModule.bsl"), "{stderr}");
            assert!(!stderr.contains("version-reason-"), "{stderr}");
            assert!(!stderr.contains("version-fix-"), "{stderr}");
            documents.push(document);
            assert_eq!(fs::read_to_string(&path).unwrap(), input);
        }
        assert_eq!(documents[0], documents[1]);
    }
}

/// The new command's options and no-write default are described in both supported locales.
#[test]
fn migration_help_and_human_preview_are_localized() {
    let fixture = workspace("1.2.3", "2.3.4.5");
    let root = fixture.0.join("src/sales-report");
    let path = root.join("SalesReport/Ext/ObjectModule.bsl");
    fs::write(&path, "Функция Версия() Возврат \"1.2.3\"; КонецФункции").unwrap();
    for (locale, help, preview, export) in [
        (
            "ru",
            "Подготовить контракт версии",
            "План миграции",
            "Добавить Экспорт",
        ),
        (
            "en",
            "Prepare the external report",
            "Migration preview",
            "Export the existing function",
        ),
    ] {
        let output = eska(&root, locale, &["version", "migrate", "--help"]);
        assert!(output.status.success(), "{output:?}");
        assert!(String::from_utf8_lossy(&output.stdout).contains(help));
        let output = eska(&root, locale, &["version", "migrate"]);
        assert!(output.status.success(), "{output:?}");
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains(preview), "{stdout}");
        assert!(stdout.contains(export), "{stdout}");
        assert!(stdout.contains("--write"));
        assert!(!stdout.contains('\x1b'));
        assert!(!fs::read_to_string(&path).unwrap().contains(" Экспорт"));
    }
}
