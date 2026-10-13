use crate::project_core::object_module_version::{ContractErrorKind, inspect_contract};

const VERSION: &str = "Функция Версия() Экспорт Возврат \"1.2.3\"; КонецФункции\n";

/// Direct assignment and `Insert` agree with both the empty structure and BSP factory profiles.
#[test]
fn accepts_bound_registration_without_changing_its_other_fields() {
    for constructor in [
        "Новый Структура",
        "New Structure()",
        "ДополнительныеОтчетыИОбработки.СведенияОВнешнейОбработке(СтандартныеПодсистемыСервер.ВерсияБиблиотеки())",
    ] {
        for setter in [
            "Данные.Версия = Версия();",
            "Данные.Insert(\"Версия\", вЕрСиЯ());",
        ] {
            let module = format!(
                "{VERSION}#Если Сервер Тогда\nФункция СведенияОВнешнейОбработке() Экспорт\nДанные = {constructor};\nДанные.Вид = \"Отчет\";\n{setter}\nДанные.Вставить(\"Команды\", ПолучитьКоманды());\nВозврат Данные;\nКонецФункции\n#КонецЕсли"
            );
            assert_eq!(
                inspect_contract(&module).unwrap().version.to_string(),
                "1.2.3",
                "{module}"
            );
        }
    }
    assert!(inspect_contract(VERSION).is_ok());
}

/// Independent literals and member calls cannot claim a shared source of the number.
#[test]
fn requires_a_direct_call_in_the_version_field() {
    for expression in [
        "\"1.2.3\"",
        "Номер",
        "Модуль.Версия()",
        "Версия(1)",
        "(Версия())",
    ] {
        let module = format!(
            "{VERSION}Функция СведенияОВнешнейОбработке() Экспорт\nДанные = Новый Структура; Данные.Версия = {expression}; Возврат Данные; КонецФункции"
        );
        let error = inspect_contract(&module).unwrap_err();
        assert_eq!(
            error.kind,
            ContractErrorKind::RegistrationUnbound,
            "{module}"
        );
        assert_eq!(&module[error.range], expression);
    }
}

/// Alias escapes, later overwrites and conditional versions cannot prove the returned value.
#[test]
fn refuses_unconfirmed_registration_dataflow() {
    for (before, body, returned) in [
        (
            "",
            "Данные = ПолучитьДанные(); Данные.Версия = Версия();",
            "Данные",
        ),
        (
            "",
            "Данные = Новый Структура(\"Версия\", \"1.2.3\"); Данные.Версия = Версия();",
            "Данные",
        ),
        ("", "Данные = Новый Структура;", "Данные"),
        (
            "",
            "Данные = Новый Структура; Если Истина Тогда Данные.Версия = Версия(); КонецЕсли;",
            "Данные",
        ),
        (
            "",
            "Данные = Новый Структура; Данные.Версия = Версия(); Данные.Версия = \"9.9.9\";",
            "Данные",
        ),
        (
            "",
            "Данные = Новый Структура; Данные.Версия = Версия(); Данные.Вставить(\"Версия\", Версия());",
            "Данные",
        ),
        (
            "",
            "Данные = Новый Структура; Данные.Версия = Версия(); Изменить(Данные);",
            "Данные",
        ),
        (
            "",
            "Данные = Новый Структура; Данные.Версия = Версия(); Копия = Данные;",
            "Данные",
        ),
        (
            "",
            "Данные = Новый Структура; Данные.Версия = Версия(); Данные.Очистить();",
            "Данные",
        ),
        (
            "",
            "Данные = Новый Структура; Данные.Версия = Версия(); Данные.Вставить(Ключ, \"1.2.3\");",
            "Данные",
        ),
        (
            "",
            "Данные = Новый Структура; Данные.Версия = Версия(); Выполнить(\"Данные.Версия = 1\");",
            "Данные",
        ),
        (
            "",
            "Данные = Новый Структура; Данные.Версия = Версия(); Версия = \"9.9.9\";",
            "Данные",
        ),
        (
            "",
            "Данные = Новый Структура; Данные.Версия = Версия(); Данные = Новый Структура;",
            "Данные",
        ),
        (
            "",
            "Данные.Версия = Версия(); Данные = Новый Структура;",
            "Данные",
        ),
        (
            "",
            "Данные = Новый Структура; Данные.Версия = Версия(); Если Истина Тогда Возврат Данные; КонецЕсли;",
            "Данные",
        ),
        (
            "",
            "Данные = Новый Структура; Данные.Версия = Версия();",
            "Новый Структура",
        ),
        (
            "Перем Данные;\n",
            "Данные = Новый Структура; Данные.Версия = Версия();",
            "Данные",
        ),
        (
            "Перем Версия;\n",
            "Данные = Новый Структура; Данные.Версия = Версия();",
            "Данные",
        ),
    ] {
        let module = format!(
            "{before}{VERSION}Функция СведенияОВнешнейОбработке() Экспорт\n{body}\nВозврат {returned};\nКонецФункции"
        );
        let error = inspect_contract(&module).unwrap_err();
        assert_eq!(
            error.kind,
            ContractErrorKind::RegistrationUnsupported,
            "{module}"
        );
        assert!(module.get(error.range).is_some());
    }
}

/// Duplicate or non-callable registration declarations are never selected by their ordering.
#[test]
fn refuses_ambiguous_and_unsupported_registration_signatures() {
    let valid = "Функция СведенияОВнешнейОбработке() Экспорт\nДанные = Новый Структура; Данные.Версия = Версия(); Возврат Данные; КонецФункции";
    let error = inspect_contract(&format!("{VERSION}{valid}\n{valid}")).unwrap_err();
    assert_eq!(error.kind, ContractErrorKind::RegistrationAmbiguous);
    for declaration in [
        valid.replace("() Экспорт", "(Параметр) Экспорт"),
        valid.replace(" Экспорт", ""),
        format!("&НаСервере\n{valid}"),
        "Процедура СведенияОВнешнейОбработке() Экспорт КонецПроцедуры".to_owned(),
    ] {
        assert_eq!(
            inspect_contract(&format!("{VERSION}{declaration}"))
                .unwrap_err()
                .kind,
            ContractErrorKind::RegistrationUnsupported
        );
    }
}
