//! Multiline literals change source margins while retaining their complete decoded content.

use super::{IndentStyle, Options, Range, Request, apply, formatted, options};

/// Query continuation bars belong to the opening quote's column, without reformatting SDBL content.
#[test]
fn supplied_query_block_aligns_its_literal_margin() {
    let source = "Функция ОбъектыПорции(ХранилищеТаблиц, Договоры)\nЗапрос = Новый Запрос;\nЗапрос.МенеджерВременныхТаблиц = ХранилищеТаблиц;\nЗапрос.УстановитьПараметр(\"Договоры\", Договоры);\nЗапрос.Текст =\n\"ВЫБРАТЬ\n | Объекты.Договор КАК Договор, Объекты.ВидОбъекта КАК ВидОбъекта, Объекты.Ссылка КАК Ссылка\n |ИЗ ВТ_ОбъектыКПометке КАК Объекты\n |ГДЕ Объекты.Договор В (&Договоры)\";\nВозврат Запрос.Выполнить().Выгрузить();\nКонецФункции\n";
    let output = formatted(source, options());
    assert!(output.contains("\tЗапрос.Текст =\n\t\t\"ВЫБРАТЬ\n\t\t| Объекты.Договор КАК Договор, Объекты.ВидОбъекта КАК ВидОбъекта, Объекты.Ссылка КАК Ссылка\n\t\t|ИЗ ВТ_ОбъектыКПометке КАК Объекты\n\t\t|ГДЕ Объекты.Договор В (&Договоры)\";"), "{output}");
}

/// The supplied prose follows exactly the same assignment and margin rules as a query string.
#[test]
fn supplied_multiline_message_starts_after_assignment() {
    let source = "РезультатВыполнения = \"Операция завершилась с ошибкой или была отменена. Завершенные цепочки сохранены.\n\t\t|Подробности: журнал регистрации, события ПометкаДубликатовДоговоров.\";\n";
    let expected = "РезультатВыполнения =\n\t\"Операция завершилась с ошибкой или была отменена. Завершенные цепочки сохранены.\n\t|Подробности: журнал регистрации, события ПометкаДубликатовДоговоров.\";\n";
    assert_eq!(formatted(source, options()), expected);
    assert_eq!(
        formatted(
            &format!("\u{feff}{}", source.replace('\n', "\r\n")),
            options()
        ),
        format!("\u{feff}{}", expected.replace('\n', "\r\n"))
    );
}

/// Nested calls align each literal with its own opening quote, including empty content lines.
#[test]
fn nested_literals_follow_their_argument_indentation() {
    let output = formatted(
        "Outer(Inner(\"А\n|  Б\n|\", 1), \"В\n  | Г\");\n",
        options(),
    );
    assert_eq!(
        output,
        "Outer(\n\tInner(\n\t\t\"А\n\t\t|  Б\n\t\t|\",\n\t\t1\n\t),\n\t\"В\n\t| Г\"\n);\n"
    );
    let output = formatted(
        "Text = \"A\n| B\";\n",
        Options {
            indent_style: IndentStyle::Space,
            continuation_indent_width: 6,
            ..options()
        },
    );
    assert_eq!(output, "Text =\n      \"A\n      | B\";\n");
}

/// Canonical literal comparison can never hide changed content, escapes, comments or line endings.
#[test]
fn literal_equivalence_ignores_only_indentation_before_bars() {
    let original = "\"А \"\"Б\"\"\r\n // comment  \r\n |  В  \"";
    let aligned = "\"А \"\"Б\"\"\r\n // comment  \r\n\t\t|  В  \"";
    assert!(crate::literals::equivalent(original, aligned));
    for changed in [
        aligned.replace("|  В", "| В"),
        aligned.replace("В  ", "В "),
        aligned.replace(" // comment", "\t// comment"),
        aligned.replace("\r\n", "\n"),
        aligned.replace("\"\"Б\"\"", "\"\"Г\"\""),
    ] {
        assert!(
            !crate::literals::equivalent(original, &changed),
            "{changed:?}"
        );
    }
    let source = format!("Text={original};\n");
    let output = formatted(&source, options());
    assert!(output.contains(" // comment  \r\n\t|  В  \""), "{output:?}");
}

/// Explicit opt-out and the default 1C profile retain the original token bytes.
#[test]
fn literal_indentation_is_an_independent_bsl_option() {
    let source = "Text = \"A\n  |B\";\n";
    for policy in [
        Options::default(),
        Options {
            indent_multiline_strings: false,
            ..options()
        },
    ] {
        assert_eq!(formatted(source, policy), source);
    }
}

/// A selected literal statement cannot reindent a neighboring literal or a frozen source region.
#[test]
fn range_and_disabled_literals_keep_unselected_bytes() {
    let source = "First=\"A\n |B\";\nSecond=\"C\n  |D\";\n// eska-fmt off\nThird=\"E\n|F\";\n// eska-fmt on\n";
    let end = source.find("\nSecond").unwrap();
    let request = Request {
        schema_version: 1,
        language: super::Language::Bsl,
        text: source.into(),
        options: Options {
            align_assignments: false,
            ..options()
        },
        range: Some(Range { start: 0, end }),
    };
    let response = crate::format(&request).unwrap();
    response.validate(&request).unwrap();
    let output = apply(source, &response);
    assert_eq!(output, format!("First =\n\t\"A\n\t|B\";{}", &source[end..]));
}
