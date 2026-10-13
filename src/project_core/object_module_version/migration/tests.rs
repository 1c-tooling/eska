use super::*;

/// Only an explicit initial value creates an exported, documented function.
#[test]
fn creates_a_missing_function_with_bom_and_existing_line_endings() {
    for input in [
        "",
        "\u{feff}",
        "\u{feff}// Сохранить\r\n",
        "#Если Сервер Тогда\nПроцедура Другая() КонецПроцедуры\n#КонецЕсли",
    ] {
        assert_eq!(
            plan_migration(input, None).unwrap_err().kind,
            ContractErrorKind::InitialVersionRequired
        );
        for value in ["1.2.3", "01.02.003.0004"] {
            let initial = ProjectVersion::parse(value).unwrap();
            let plan = plan_migration(input, Some(&initial)).unwrap();
            let output = plan.apply(input).unwrap();
            assert!(output.starts_with(input));
            assert_eq!(inspect_contract(&output).unwrap().version, initial);
            assert!(output.contains("// Возвращаемое значение:"));
            if input.contains("\r\n") {
                assert!(!output.replace("\r\n", "").contains('\n'));
            }
            assert!(plan_migration(&output, None).unwrap().edits().is_empty());
        }
    }
}

/// Module initialization may live in a region or an alternate preprocessor branch.
#[test]
fn inserts_the_function_before_initialization_without_moving_original_bytes() {
    let initial = ProjectVersion::parse("1.2.3").unwrap();
    for (prefix, initialization) in [
        (
            "\u{feff}Перем Состояние;\r\nПроцедура Другая()\r\nПерем Локальная;\r\nСообщить(1);\r\nКонецПроцедуры\r\n#Область Инициализация\r\n",
            "Состояние = 1;\r\n#КонецОбласти\r\n",
        ),
        (
            "\u{feff}// Сохранить\r\n",
            "#Если Сервер Тогда\r\nПроцедура Другая() КонецПроцедуры\r\n#Иначе\r\nВызватьИсключение \"Только сервер\";\r\n#КонецЕсли\r\n",
        ),
        ("Перем Состояние;\n", "Состояние = 1;\n"),
        (
            "#Область Инициализация\n#Область Переменные\nПерем Состояние;\n#КонецОбласти\n",
            "Состояние = 1;\n#КонецОбласти\n",
        ),
    ] {
        let input = format!("{prefix}{initialization}");
        let plan = plan_migration(&input, Some(&initial)).unwrap();
        let [edit] = plan.edits() else {
            panic!("one function insertion expected");
        };
        assert_eq!(edit.kind, EditKind::AddFunction);
        assert_eq!(edit.range, prefix.len()..prefix.len());
        let output = plan.apply(&input).unwrap();
        assert!(output.starts_with(prefix));
        assert!(output.ends_with(initialization));
        assert_eq!(inspect_contract(&output).unwrap().version, initial);
        assert!(plan_migration(&output, None).unwrap().edits().is_empty());
        if input.contains("\r\n") {
            assert!(!output.replace("\r\n", "").contains('\n'));
        }
    }
}

/// A conditional variable must not move behind the new unconditional function.
#[test]
fn refuses_mixed_conditional_variables_and_initialization() {
    let input = "\u{feff}#Если Сервер Тогда\nПерем Состояние;\nСостояние = 1;\n#КонецЕсли\n";
    let initial = ProjectVersion::parse("1.2.3").unwrap();
    let error = plan_migration(input, Some(&initial)).unwrap_err();
    assert_eq!(error.kind, ContractErrorKind::InitializationConflict);
    assert_eq!(&input[error.range], "Перем Состояние;");
}

/// Export insertion preserves comments and the authoritative literal exactly.
#[test]
fn exports_only_a_suitable_private_function() {
    for (input, export) in [
        (
            "\u{feff}Функция Версия( // Комментарий\r\n) // Сигнатура\r\nВозврат \"1.2.3\"; КонецФункции",
            " Экспорт",
        ),
        (
            "Function Версия() // signature\nReturn \"1.2.3.4\"; EndFunction",
            " Export",
        ),
    ] {
        let plan = plan_migration(input, None).unwrap();
        let [edit] = plan.edits() else {
            panic!("one export insertion expected");
        };
        assert_eq!(edit.kind, EditKind::ExportFunction);
        assert_eq!(edit.replacement, export);
        assert_eq!(
            plan.apply(input).unwrap(),
            input.replacen(')', &format!("){export}"), 1)
        );
        let other = ProjectVersion::parse("9.9.9").unwrap();
        assert_eq!(
            plan_migration(input, Some(&other)).unwrap_err().kind,
            ContractErrorKind::InitialVersionConflict
        );
    }
    for input in [
        "Функция Версия() Возврат Номер; КонецФункции",
        "Функция Версия(Аргумент) Возврат \"1.2.3\"; КонецФункции",
        "#Если Сервер Тогда\nФункция Версия() Возврат \"1.2.3\"; КонецФункции\n#КонецЕсли",
    ] {
        assert!(plan_migration(input, None).is_err());
    }
}

/// A proven BSP literal becomes a direct call while unrelated registration code is retained.
#[test]
fn links_registration_and_adds_function_in_one_snapshot_bound_plan() {
    let input = "\u{feff}#Если Сервер Тогда\r\nФункция СведенияОВнешнейОбработке() Экспорт\r\n\tРегистрация = ДополнительныеОтчетыИОбработки.СведенияОВнешнейОбработке(ВерсияБиблиотеки());\r\n\tРегистрация.Информация = \"1.2.3\";\r\n\tРегистрация.Версия = \"1.2.3\"; // Сохранить комментарий\r\n\tВозврат Регистрация;\r\nКонецФункции\r\n#КонецЕсли\r\n";
    let initial = ProjectVersion::parse("1.2.3").unwrap();
    let plan = plan_migration(input, Some(&initial)).unwrap();
    assert_eq!(
        plan.edits()
            .iter()
            .map(|edit| edit.kind)
            .collect::<Vec<_>>(),
        [EditKind::BindRegistration, EditKind::AddFunction]
    );
    let output = plan.apply(input).unwrap();
    assert!(output.starts_with(&input.replace(
        "Регистрация.Версия = \"1.2.3\"",
        "Регистрация.Версия = Версия()"
    )));
    assert_eq!(inspect_contract(&output).unwrap().version, initial);
    assert!(plan_migration(&output, None).unwrap().edits().is_empty());
    assert_eq!(
        plan.apply(&format!("{input}// concurrent change"))
            .unwrap_err()
            .kind,
        ContractErrorKind::SourceChanged
    );
}

/// Unsupported registration expressions cannot be erased by adopting the version contract.
#[test]
fn refuses_opaque_registration_instead_of_replacing_its_logic() {
    for setter in [
        "ПолучитьВерсию()",
        "\"произвольная строка\"",
        "\"1.2.\" + \"3\"",
    ] {
        let input = format!(
            "Функция Версия() Экспорт Возврат \"1.2.3\"; КонецФункции\nФункция СведенияОВнешнейОбработке() Экспорт\nРегистрация = Новый Структура; Регистрация.Версия = {setter}; Возврат Регистрация; КонецФункции"
        );
        assert_eq!(
            plan_migration(&input, None).unwrap_err().kind,
            ContractErrorKind::RegistrationUnsupported
        );
    }
}
