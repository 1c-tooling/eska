//! Client preferences and both localized presentations share typed card data.

use eska::tool_cli::localization::Locale;

use super::*;
use crate::{HoverKind, SymbolHover};

/// Preference order is significant; unknown formats never become a guessed markup kind.
#[test]
fn markup_negotiation_follows_supported_client_order_and_plaintext_default() {
    for (formats, expected) in [
        (json!(["markdown", "plaintext"]), Format::Markdown),
        (json!(["plaintext", "markdown"]), Format::Plaintext),
        (json!(["unknown", 1, "markdown"]), Format::Markdown),
        (json!(["unknown"]), Format::Plaintext),
        (json!([]), Format::Plaintext),
        (json!("markdown"), Format::Plaintext),
        (Value::Null, Format::Plaintext),
    ] {
        assert_eq!(
            format(&json!({"capabilities":{"textDocument":{
            "hover":{"contentFormat":formats}}}})),
            expected
        );
    }
    assert_eq!(format(&json!({})), Format::Plaintext);
}

/// Authored punctuation remains literal in Markdown; BSL backticks cannot terminate the code block.
#[test]
fn localized_card_preserves_source_text_and_escapes_only_markdown_presentation() {
    let card = SymbolHover {
        symbol: "Значение".into(),
        kind: HoverKind::Method,
        declaration: Some("Функция Значение(Аргумент = \"````\") Экспорт".into()),
        documentation: Some("Я😀 **пример** [ссылка](command:run) <tag> # заголовок".into()),
        return_type: Some("String[]".into()),
        start_byte: 0,
        end_byte: 16,
    };
    for (locale, method, declaration, return_type, documentation) in [
        (
            Locale::EnUs,
            "Method",
            "Declaration",
            "Return type",
            "Documentation",
        ),
        (
            Locale::RuRu,
            "Метод",
            "Объявление",
            "Возвращаемый тип",
            "Описание",
        ),
    ] {
        let localizer = Localizer::try_new(locale).unwrap();
        let plain = presentation::render(&card, Format::Plaintext, &localizer);
        assert!(plain.starts_with(&format!("Значение ({method})\n\n")));
        assert!(plain.contains(&format!(
            "{declaration}:\n{}",
            card.declaration.as_ref().unwrap()
        )));
        assert!(plain.contains(&format!("{return_type}: String[]")));
        assert!(plain.ends_with(&format!(
            "{documentation}:\n{}",
            card.documentation.as_ref().unwrap()
        )));
        let markdown = presentation::render(&card, Format::Markdown, &localizer);
        assert!(markdown.starts_with(&format!("**Значение** ({method})\n\n")));
        assert!(markdown.contains(&format!(
            "**{declaration}:**\n`````bsl\n{}\n`````",
            card.declaration.as_ref().unwrap()
        )));
        assert!(markdown.contains(&format!("**{return_type}:** String\\[\\]")));
        assert!(markdown.ends_with(
            "Я😀 \\*\\*пример\\*\\* \\[ссылка\\]\\(command\\:run\\) \\<tag\\> \\# заголовок"
        ));
    }
}

/// Parameter and local value types use a type heading, while method return types keep their own heading.
#[test]
fn positional_value_types_have_localized_type_labels() {
    for kind in [
        HoverKind::Parameter,
        HoverKind::LocalVariable,
        HoverKind::Field,
    ] {
        let card = SymbolHover {
            symbol: "Выбор".into(),
            kind,
            declaration: None,
            documentation: None,
            return_type: Some("String[]".into()),
            start_byte: 0,
            end_byte: 10,
        };
        for (locale, label) in [(Locale::EnUs, "Type"), (Locale::RuRu, "Тип")] {
            let localizer = Localizer::try_new(locale).unwrap();
            let rendered = presentation::render(&card, Format::Plaintext, &localizer);
            assert!(
                rendered.contains(&format!("{label}: String[]")),
                "{rendered}"
            );
            assert!(
                presentation::render(&card, Format::Markdown, &localizer)
                    .contains(&format!("**{label}:** String\\[\\]"))
            );
        }
    }
}

/// Every serialized kind has a translated heading in both locale resources.
#[test]
fn all_typed_symbol_kinds_have_russian_and_english_headings() {
    for (kind, english, russian) in [
        (HoverKind::Method, "Method", "Метод"),
        (
            HoverKind::ModuleVariable,
            "Module variable",
            "Переменная модуля",
        ),
        (HoverKind::Parameter, "Parameter", "Параметр"),
        (
            HoverKind::LocalVariable,
            "Local variable",
            "Локальная переменная",
        ),
        (
            HoverKind::PlatformFunction,
            "Platform function",
            "Функция платформы",
        ),
        (HoverKind::Module, "Module", "Модуль"),
        (
            HoverKind::MetadataObject,
            "Metadata object",
            "Объект метаданных",
        ),
        (
            HoverKind::ManagerModule,
            "Manager module",
            "Модуль менеджера",
        ),
        (HoverKind::Field, "Field", "Поле"),
    ] {
        let card = SymbolHover {
            symbol: "Имя".into(),
            kind,
            declaration: None,
            documentation: None,
            return_type: None,
            start_byte: 0,
            end_byte: 6,
        };
        for (locale, heading) in [(Locale::EnUs, english), (Locale::RuRu, russian)] {
            let localizer = Localizer::try_new(locale).unwrap();
            assert_eq!(
                presentation::render(&card, Format::Plaintext, &localizer),
                format!("Имя ({heading})")
            );
        }
    }
}
