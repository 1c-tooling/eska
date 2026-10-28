//! The ESKA preset is exercised through resolved public policy and the real text-edit protocol.

mod spacing;

use super::{apply, formatted};
use eska_project_core::formatting::{
    Config, IndentStyle, Language, Layer, Options, Patch, Wrapping,
    protocol::{Range, Request},
    resolve,
};
use std::path::Path;

/// The same manifest selection drives the CLI, project server and standalone library.
fn options() -> Options {
    let config = Config {
        profile: Some("eska".into()),
        ..Config::default()
    };
    resolve(
        &[Layer::new(Path::new("."), "manifest", &config, "").unwrap()],
        Path::new("Module.bsl"),
        Language::Bsl,
        &Patch::default(),
    )
    .unwrap()
    .options
}

/// Representative tables and multiline calls capture the recurring conventions in both samples.
#[test]
fn preset_formats_tables_and_multiline_calls() {
    let source = "#Область ПрограммныйИнтерфейс\n\n// Создает параметры.\nФункция Параметры() Экспорт\nПараметры=Новый Структура;\n\nПараметры.Вставить(\"Дата\",Дата(1,1,1));\nПараметры.Вставить(\"Организация\",Неопределено);\n\nКоличество=0;\nЕстьТранзакции=Ложь;\n\nЗаполнить(\nПараметры,\nКоличество\n);\nВозврат Параметры;\nКонецФункции\n\n#КонецОбласти\n";
    let expected = "#Область ПрограммныйИнтерфейс\n\n// Создает параметры.\nФункция Параметры() Экспорт\n\n\tПараметры = Новый Структура;\n\n\tПараметры.Вставить(\"Дата\"       , Дата(1, 1, 1));\n\tПараметры.Вставить(\"Организация\", Неопределено);\n\n\tКоличество     = 0;\n\tЕстьТранзакции = Ложь;\n\n\tЗаполнить(\n\t\tПараметры,\n\t\tКоличество\n\t);\n\n\tВозврат Параметры;\n\nКонецФункции\n\n#КонецОбласти\n";
    assert_eq!(formatted(source, options()), expected);
}

/// Nested calls keep their inner commas, and all outer argument columns form one stable table.
#[test]
fn call_tables_align_outer_commas_and_omitted_arguments() {
    let source = "Таблица.Добавить(\"А\",Тип(1,2),Истина);\nТаблица.Добавить(\"Длинное\",Неопределено,Ложь);\n\nМетод(,1,);\nМетод(ДлинноеИмя,,);\n";
    let output = formatted(source, options());
    assert!(
        output.contains("Таблица.Добавить(\"А\"      , Тип(1, 2)   , Истина);"),
        "{output}"
    );
    assert!(
        output.contains("Таблица.Добавить(\"Длинное\", Неопределено, Ложь);"),
        "{output}"
    );
    assert!(
        output.contains("Метод(          , 1,);\nМетод(ДлинноеИмя,  ,);"),
        "{output}"
    );
}

/// Tables stop at blank lines, comments, different receivers, different arity and block boundaries.
#[test]
fn unrelated_calls_do_not_share_columns() {
    let source = "Данные.Вставить(\"А\",1);\n\nДанные.Вставить(\"Длинное\",2);\n// граница\nДанные.Вставить(\"Б\",3);\nДругие.Вставить(\"ОченьДлинное\",4);\nДанные.Вставить(\"В\",5,6);\n";
    let output = formatted(source, options());
    for call in [
        "Данные.Вставить(\"А\", 1);",
        "Данные.Вставить(\"Длинное\", 2);",
        "Данные.Вставить(\"Б\", 3);",
        "Другие.Вставить(\"ОченьДлинное\", 4);",
        "Данные.Вставить(\"В\", 5, 6);",
    ] {
        assert!(output.contains(call), "{output}");
    }
}

/// Column width is visual, including Unicode identifiers, tabs in literals and alternate indentation.
#[test]
fn horizontal_tables_remain_stable_with_spacing_overrides() {
    let source = "Процедура Тест()\nСписок.Добавить(\"Я\"\"\",1+2,);\nСписок.Добавить(\"Длинное\",3,4);\nКонецПроцедуры\n";
    for indent_style in [IndentStyle::Tab, IndentStyle::Space] {
        for spaces in [true, false] {
            for wrap_arguments in [Wrapping::Fit, Wrapping::OnePerLine, Wrapping::Preserve] {
                for column_limit in [0, 35, 120] {
                    formatted(
                        source,
                        Options {
                            indent_style,
                            space_after_comma: spaces,
                            space_inside_parentheses: spaces,
                            space_before_call_parenthesis: spaces,
                            space_around_operators: spaces,
                            wrap_arguments,
                            column_limit,
                            ..options()
                        },
                    );
                }
            }
        }
    }
    formatted("Метод(\"\t\",1);\nМетод(\"Я\",2);", options());
}

/// A long line stays intact, and an explicitly broken call closes at its owning block's indentation.
#[test]
fn preset_preserves_long_lines_and_explicit_breaks() {
    let long = "Если ПервыйОченьДлинныйИдентификатор = ВторойОченьДлинныйИдентификатор И ТретийОченьДлинныйИдентификатор = ЧетвертыйОченьДлинныйИдентификатор Тогда\nМетод(\n1,\n2);\nКонецЕсли;\n";
    let output = formatted(long, options());
    assert_eq!(output.lines().next(), long.lines().next());
    assert!(
        output.contains("\tМетод(\n\t\t1,\n\t\t2\n\t);\n"),
        "{output}"
    );
    let no_closing_break = formatted(
        "Метод(\n1,\n2);",
        Options {
            closing_parenthesis_on_new_line: false,
            ..options()
        },
    );
    assert_eq!(no_closing_break, "Метод(\n\t1,\n\t2);\n");
}

/// The sampled 150-column budget still wraps new oversized code while retaining intentional breaks.
#[test]
fn preset_wraps_new_overflows_without_collapsing_manual_layout() {
    let source = format!(
        "Метод(\"{}\",\"{}\");\nА = Первое\n+ Второе;\nМетод(\n1,\n2);\n",
        "А".repeat(80),
        "Б".repeat(80)
    );
    let output = formatted(&source, options());
    assert!(output.lines().all(|line| line.chars().count() <= 150));
    assert!(output.starts_with("Метод(\n\t\""));
    assert!(output.contains("А = Первое\n\t+ Второе;"));
    assert!(output.ends_with("Метод(\n\t1,\n\t2\n);\n"));
}

/// A row that will wrap already separates call tables during the first planning pass.
#[test]
fn overflowing_calls_do_not_delay_neighbor_alignment_until_a_second_pass() {
    let small = "Метод(\"А\",1);\nМетод(\"Длинное\",2);\n";
    let large = format!("Метод(\"{}\",\"{}\");\n", "А".repeat(80), "Б".repeat(80));
    for source in [
        format!("{small}{large}"),
        format!("{large}{small}"),
        format!("{small}{large}{small}"),
    ] {
        let output = formatted(&source, options());
        assert!(output.contains("Метод(\"А\"      , 1);"));
    }
}

/// Formatting a single table row can use its neighbors' columns without rewriting those neighbors.
#[test]
fn selected_table_row_preserves_surrounding_bytes() {
    let input = "Метод(\"А\",1);\nМетод(\"Длинное\",2);\n";
    let range = Range {
        start: 0,
        end: input.find('\n').unwrap(),
    };
    let request = Request {
        schema_version: 1,
        language: Language::Bsl,
        text: input.into(),
        options: options(),
        range: Some(range),
    };
    let response = crate::format(&request).unwrap();
    response.validate(&request).unwrap();
    let output = apply(input, &response);
    assert!(output.ends_with("Метод(\"Длинное\",2);\n"));
    assert!(output.starts_with("Метод(\"А\"      , 1);\n"));
}

/// The new preset cannot change query literals, disabled sections or controlled methods.
#[test]
fn preset_retains_opaque_and_protected_source() {
    let protected = "&ИзменениеИКонтроль(\"Тест\")\nПроцедура Тест()\n  А=1;\nКонецПроцедуры\n";
    let literal = "\"ВЫБРАТЬ\n    |  1 КАК А,2 КАК Б\"";
    let disabled = "// eska-fmt off\nБ=  2;\n// eska-fmt on";
    let source = format!(
        "{protected}\nФункция Запрос()\nТекст={literal};\n{disabled}\nВозврат Текст;\nКонецФункции\n"
    );
    let output = formatted(&source, options());
    assert!(output.starts_with(protected));
    assert!(output.contains(literal));
    assert!(output.contains(disabled));
}

/// Query values can start on a separate continuation line without changing any literal bytes.
#[test]
fn preset_keeps_a_separate_assignment_value_line() {
    let source = "Функция ТекстЗапроса()\nЗапрос = Новый Запрос;\nЗапрос.Текст =\n\"ВЫБРАТЬ\n        | А КАК А,Б КАК Б\";\nВозврат Запрос.Текст;\nКонецФункции\n";
    let output = formatted(source, options());
    assert!(
        output.contains("\tЗапрос.Текст =\n\t\t\"ВЫБРАТЬ\n        | А КАК А,Б КАК Б\";"),
        "{output}"
    );
}
