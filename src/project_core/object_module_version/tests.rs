use super::*;
use crate::project_core::native_version::{BumpKind, VersionScheme};

/// The editable bytes belong to the real declaration, even beside misleading text.
#[test]
fn reads_both_schemes_and_preserves_bom_crlf_and_unrelated_literals() {
    for (value, scheme, next) in [
        ("1.2.3", VersionScheme::ThreePart, "1.2.4"),
        ("01.02.003.0004", VersionScheme::FourPart, "01.02.004.0001"),
    ] {
        let input = format!(
            "\u{feff}// Функция Версия() Экспорт Возврат \"9.9.9\"; КонецФункции\r\n\
             #Область ПрограммныйИнтерфейс\r\n\
             Функция вЕрСиЯ() Экспорт // Комментарий сигнатуры.\r\n\
             \t// Возвращает номер проекта.\r\n\
             \tВозврат \"{value}\"; // Сохранить.\r\n\
             КонецФункции\r\n#КонецОбласти\r\n\
             Процедура Другая()\r\n\tСообщить(\"1.2.3\");\r\nКонецПроцедуры\r\n"
        );
        let info = inspect(&input).unwrap();
        assert_eq!(info.version.to_string(), value);
        assert_eq!(info.version.scheme(), scheme);
        assert_eq!(&input[info.value_range.clone()], value);
        assert!(input[info.function_range].starts_with("Функция вЕрСиЯ"));
        let mut output = input.clone();
        output.replace_range(
            info.value_range.clone(),
            &info.version.bumped(BumpKind::Patch).unwrap().to_string(),
        );
        assert_eq!(
            output,
            input.replacen(
                &format!("Возврат \"{value}\""),
                &format!("Возврат \"{next}\""),
                1
            )
        );
        assert_eq!(inspect(&output).unwrap().version.to_string(), next);
    }
}

/// Keywords may be English; the public function name remains the agreed Cyrillic identifier.
#[test]
fn accepts_english_keywords_and_trivia_inside_signature() {
    let input = "Function ВЕРСИЯ( // no arguments\n) Export\nReturn \"0.0.0\";\nEndFunction";
    let info = inspect(input).unwrap();
    assert_eq!(info.version.to_string(), "0.0.0");
}

/// Signature and body checks reject code whose execution would choose the version.
#[test]
fn rejects_unmanaged_declarations_and_bodies() {
    let cases = [
        (
            "Функция Версия() Возврат \"1.2.3\"; КонецФункции",
            ContractErrorKind::NotExported,
        ),
        (
            "Функция Версия(Параметр = 1) Экспорт Возврат \"1.2.3\"; КонецФункции",
            ContractErrorKind::Parameters,
        ),
        (
            "Асинх Функция Версия() Экспорт Возврат \"1.2.3\"; КонецФункции",
            ContractErrorKind::AsyncFunction,
        ),
        (
            "&НаСервере\nФункция Версия() Экспорт Возврат \"1.2.3\"; КонецФункции",
            ContractErrorKind::AnnotatedFunction,
        ),
        (
            "#Если Сервер Тогда\nФункция Версия() Экспорт Возврат \"1.2.3\"; КонецФункции\n#КонецЕсли",
            ContractErrorKind::ConditionalFunction,
        ),
        (
            "#Вставка\nФункция Версия() Экспорт Возврат \"1.2.3\"; КонецФункции\n#КонецВставки",
            ContractErrorKind::ConditionalFunction,
        ),
        (
            "Процедура Версия() Экспорт КонецПроцедуры",
            ContractErrorKind::ExpectedFunction,
        ),
        (
            "Функция Версия() Экспорт Возврат \"1.2.3\"; КонецФункции\nФункция версия() Экспорт Возврат \"1.2.4\"; КонецФункции",
            ContractErrorKind::AmbiguousFunction,
        ),
        (
            "Функция Версия() Экспорт Возврат \"1.2.3\"; КонецФункции\nПроцедура ВЕРСИЯ() КонецПроцедуры",
            ContractErrorKind::AmbiguousFunction,
        ),
        (
            "Функция Версия() Экспорт КонецФункции",
            ContractErrorKind::InvalidBody,
        ),
        (
            "Функция Версия() Экспорт Возврат Номер; КонецФункции",
            ContractErrorKind::InvalidBody,
        ),
        (
            "Функция Версия() Экспорт Возврат (\"1.2.3\"); КонецФункции",
            ContractErrorKind::InvalidBody,
        ),
        (
            "Функция Версия() Экспорт Возврат \"1.2.\" + \"3\"; КонецФункции",
            ContractErrorKind::InvalidBody,
        ),
        (
            "Функция Версия() Экспорт Перем Номер; Возврат \"1.2.3\"; КонецФункции",
            ContractErrorKind::InvalidBody,
        ),
        (
            "Функция Версия() Экспорт Возврат \"1.2.3\"; Возврат \"9.9.9\"; КонецФункции",
            ContractErrorKind::InvalidBody,
        ),
        (
            "Функция Версия() Экспорт Если Истина Тогда Возврат \"1.2.3\"; КонецЕсли; КонецФункции",
            ContractErrorKind::InvalidBody,
        ),
        (
            "Функция Версия() Экспорт Возврат \"01.2.3\"; КонецФункции",
            ContractErrorKind::InvalidVersion,
        ),
        (
            "Функция Версия() Экспорт Возврат \"1.2.3\"\"\"; КонецФункции",
            ContractErrorKind::InvalidVersion,
        ),
        (
            "Функция Версия() Экспорт Возврат \"1.2.\n|3\"; КонецФункции",
            ContractErrorKind::InvalidVersion,
        ),
    ];
    for (input, kind) in cases {
        let error = inspect(input).unwrap_err();
        assert_eq!(error.kind, kind, "{input}");
        assert!(input.get(error.range).is_some(), "{input}");
    }
}

/// Misleading comments and strings cannot satisfy the declaration contract.
#[test]
fn absence_points_at_the_original_eof() {
    for input in [
        "",
        "\u{feff}// Функция Версия() Экспорт\r\n",
        "Сообщить(\"Функция Версия() Экспорт\");",
    ] {
        assert_eq!(
            inspect(input).unwrap_err(),
            ContractError {
                kind: ContractErrorKind::MissingFunction,
                range: input.len()..input.len(),
            }
        );
    }
}

/// A broken neighbouring statement must not be blessed by a valid version function.
#[test]
fn invalid_syntax_refuses_before_inspecting_the_version() {
    for input in [
        "Функция Версия( Возврат \"1.2.3\";",
        "\u{feff}Функция Версия() Экспорт Возврат \"1.2.3\"; КонецФункции\r\nПроцедура Ошибка( КонецПроцедуры",
    ] {
        let error = inspect(input).unwrap_err();
        assert_eq!(error.kind, ContractErrorKind::InvalidSyntax);
        assert!(input.get(error.range).is_some());
    }
}
