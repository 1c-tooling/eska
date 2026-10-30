//! Structural blank lines use direct operations and preserve the shared source-edit safeguards.

use super::{Options, Range, Request, apply, formatted, options};

const SAMPLE: &str = "Функция ИсключаемыеСсылкиПоМестамИспользования(МестаИспользования)

	ВладельцыМест       = Новый Соответствие;
	УникальныеВладельцы = Новый Соответствие;
	Для Каждого Место Из МестаИспользования Цикл
		Владелец = ВладелецМестаИспользования(Место);
		ВладельцыМест.Вставить(Место, Владелец);
		Если Владелец <> Неопределено Тогда
			УникальныеВладельцы.Вставить(Владелец, Истина);
		КонецЕсли;
	КонецЦикла;
	СсылкиВладельцев = Новый Массив;
	Для Каждого Элемент Из УникальныеВладельцы Цикл
		СсылкиВладельцев.Добавить(Элемент.Ключ);
	КонецЦикла;
	ПометкиВладельцев = Новый Соответствие;
	Если СсылкиВладельцев.Количество() > 0 Тогда
		ПометкиВладельцев = ОбщегоНазначения.ЗначениеРеквизитаОбъектов(СсылкиВладельцев, \"ПометкаУдаления\");
	КонецЕсли;

	Результат = Новый Соответствие;
	Для Каждого Место Из МестаИспользования Цикл
		Владелец = ВладельцыМест[Место];
		Если Владелец = Неопределено Или ПометкиВладельцев[Владелец] <> Истина Тогда
			Результат.Вставить(Место.Ссылка, Истина);
		КонецЕсли;
	КонецЦикла;
	Возврат Результат;

КонецФункции
";

const EXPECTED: &str = "Функция ИсключаемыеСсылкиПоМестамИспользования(МестаИспользования)

	ВладельцыМест       = Новый Соответствие;
	УникальныеВладельцы = Новый Соответствие;

	Для Каждого Место Из МестаИспользования Цикл

		Владелец = ВладелецМестаИспользования(Место);
		ВладельцыМест.Вставить(Место, Владелец);

		Если Владелец <> Неопределено Тогда
			УникальныеВладельцы.Вставить(Владелец, Истина);
		КонецЕсли;

	КонецЦикла;

	СсылкиВладельцев = Новый Массив;

	Для Каждого Элемент Из УникальныеВладельцы Цикл
		СсылкиВладельцев.Добавить(Элемент.Ключ);
	КонецЦикла;

	ПометкиВладельцев = Новый Соответствие;

	Если СсылкиВладельцев.Количество() > 0 Тогда
		ПометкиВладельцев = ОбщегоНазначения.ЗначениеРеквизитаОбъектов(СсылкиВладельцев, \"ПометкаУдаления\");
	КонецЕсли;

	Результат = Новый Соответствие;

	Для Каждого Место Из МестаИспользования Цикл

		Владелец = ВладельцыМест[Место];

		Если Владелец = Неопределено Или ПометкиВладельцев[Владелец] <> Истина Тогда
			Результат.Вставить(Место.Ссылка, Истина);
		КонецЕсли;

	КонецЦикла;

	Возврат Результат;

КонецФункции
";

/// The supplied algorithm gains whitespace only, including the short inner IF bodies.
#[test]
fn supplied_function_matches_structural_layout_with_bom_and_crlf() {
    assert_eq!(formatted(SAMPLE, options()), EXPECTED);
    let source = format!("\u{feff}{}", SAMPLE.replace('\n', "\r\n"));
    assert_eq!(
        formatted(&source, options()),
        format!("\u{feff}{}", EXPECTED.replace('\n', "\r\n"))
    );
}

/// The extra message in the requested illustration makes that inner body a two-operation branch.
#[test]
fn two_operations_expand_a_previously_compact_branch() {
    let source = "Если Условие Тогда\nСообщить(\"Что-нибудь...\");\nРезультат.Вставить(Место.Ссылка,Истина);\nКонецЕсли;\n";
    assert_eq!(
        formatted(source, options()),
        "Если Условие Тогда\n\n\tСообщить(\"Что-нибудь...\");\n\tРезультат.Вставить(Место.Ссылка, Истина);\n\nКонецЕсли;\n"
    );
}

/// ELSEIF, ELSE and EXCEPT each use their own body count in both source languages.
#[test]
fn alternative_branches_have_independent_statement_counts() {
    let source = "Procedure Test()\nStart();\nIf A Then\n\nOne();\n\nElsIf B Then\nTwo();\nThree();\nElse\n\nFour();\n\nEndIf;\nTry\nOne();\nTwo();\nExcept\n\nThree();\n\nEndTry;\nFinish();\nEndProcedure\n";
    let expected = "Procedure Test()\n\n\tStart();\n\n\tIf A Then\n\t\tOne();\n\tElsIf B Then\n\n\t\tTwo();\n\t\tThree();\n\n\tElse\n\t\tFour();\n\tEndIf;\n\n\tTry\n\n\t\tOne();\n\t\tTwo();\n\n\tExcept\n\t\tThree();\n\tEndTry;\n\n\tFinish();\n\nEndProcedure\n";
    assert_eq!(formatted(source, options()), expected);
    let source = "Попытка\n\nПервое();\n\nИсключение\nВторое();\nТретье();\nКонецПопытки;\n";
    assert_eq!(
        formatted(source, options()),
        "Попытка\n\tПервое();\nИсключение\n\n\tВторое();\n\tТретье();\n\nКонецПопытки;\n"
    );
}

/// A sole compound operation stays compact even when its own body contains several operations.
#[test]
fn nested_blocks_and_multiline_calls_count_as_single_operations() {
    let source = "Пока Условие Цикл\n\nЕсли Флаг Тогда\nПервое();\nВторое();\nКонецЕсли;\n\nКонецЦикла;\nДля Индекс=1 По 2 Цикл\n\nМетод(\n1,\n2);\n\nКонецЦикла;\n";
    let expected = "Пока Условие Цикл\n\tЕсли Флаг Тогда\n\n\t\tПервое();\n\t\tВторое();\n\n\tКонецЕсли;\nКонецЦикла;\n\nДля Индекс = 1 По 2 Цикл\n\tМетод(\n\t\t1,\n\t\t2\n\t);\nКонецЦикла;\n";
    assert_eq!(formatted(source, options()), expected);
}

/// A sole return stays compact, while other returns retain separation and their attached comments.
#[test]
fn return_spacing_respects_compact_branches_and_method_boundaries() {
    let source = "Функция Тест()\nЕсли Условие Тогда\n// Причина выхода.\nВозврат 1;\nКонецЕсли;\nДействие();\n// Готовый результат.\nВозврат 2;\nКонецФункции\nПроцедура Выход()\nВозврат;\nКонецПроцедуры\n";
    let output = formatted(source, options());
    assert!(
        output.contains("Если Условие Тогда\n\t\t// Причина выхода.\n\t\tВозврат 1;\n\tКонецЕсли;"),
        "{output}"
    );
    assert!(
        output.contains("Действие();\n\n\t// Готовый результат.\n\tВозврат 2;"),
        "{output}"
    );
    assert!(
        output.contains("Процедура Выход()\n\n\tВозврат;\n\nКонецПроцедуры"),
        "{output}"
    );
}

/// Own-line comments describe the following block without being detached by an outer separator.
#[test]
fn comments_are_not_counted_as_operations_or_detached_from_blocks() {
    let source = "Первое();\n// Пояснение.\nЕсли Флаг Тогда\n\n// Одна операция.\nВторое();\n\nКонецЕсли;\n// Продолжение.\nТретье();\n";
    assert_eq!(
        formatted(source, options()),
        "Первое();\n\n// Пояснение.\nЕсли Флаг Тогда\n\t// Одна операция.\n\tВторое();\nКонецЕсли;\n\n// Продолжение.\nТретье();\n"
    );
}

/// Literal semicolons, nested calls and compiler alternatives do not become extra operations.
#[test]
fn literals_and_preprocessor_branches_keep_their_structure() {
    let source = "Если Флаг Тогда\nТекст=\"Первое; Второе;\n |Третье\";\nКонецЕсли;\n#Если Клиент Тогда\nЕсли Первое Тогда\n#Иначе\nЕсли Второе Тогда\n#КонецЕсли\nДействие();\nКонецЕсли;\n";
    let output = formatted(source, options());
    assert!(
        output.starts_with(
            "Если Флаг Тогда\n\tТекст =\n\t\t\"Первое; Второе;\n\t\t|Третье\";\nКонецЕсли;"
        ),
        "{output}"
    );
    assert!(
        output.contains("#КонецЕсли\n\tДействие();\nКонецЕсли;"),
        "{output}"
    );
}

/// Zero disables a new rule; ordinary maxima cannot suppress an explicit structural minimum.
#[test]
fn structural_spacing_can_be_overridden_independently_of_ordinary_blank_lines() {
    let source = "Функция Тест()\nНачало();\nЕсли Флаг Тогда\nПервое();\nВторое();\nКонецЕсли;\nВозврат 1;\nКонецФункции\n";
    let output = formatted(
        source,
        Options {
            max_empty_lines: 0,
            ..options()
        },
    );
    assert!(output.contains("\tНачало();\n\n\tЕсли Флаг Тогда\n\n\t\tПервое();\n\t\tВторое();\n\n\tКонецЕсли;\n\n\tВозврат 1;"), "{output}");
    let compact = formatted(
        source,
        Options {
            blank_lines_around_blocks: 0,
            blank_lines_inside_multistatement_blocks: 0,
            blank_lines_before_return: 0,
            ..options()
        },
    );
    assert!(compact.contains("\tНачало();\n\tЕсли Флаг Тогда\n\t\tПервое();\n\t\tВторое();\n\tКонецЕсли;\n\tВозврат 1;"), "{compact}");
    let output = formatted(
        source,
        Options {
            blank_lines_around_blocks: 2,
            blank_lines_inside_multistatement_blocks: 2,
            blank_lines_before_return: 2,
            ..options()
        },
    );
    assert!(
        output.contains("Начало();\n\n\n\tЕсли Флаг Тогда\n\n\n"),
        "{output}"
    );
}

/// A local statement selection cannot add the block gaps in unselected neighboring units.
#[test]
fn selected_operation_keeps_surrounding_units_byte_identical() {
    let source =
        "Функция Тест()\nЕсли Флаг Тогда\nА=1;\nБ=2;\nКонецЕсли;\nВозврат А;\nКонецФункции\n";
    let start = source.find("Б=2").unwrap();
    let request = Request {
        schema_version: 1,
        language: super::Language::Bsl,
        text: source.into(),
        options: options(),
        range: Some(Range {
            start,
            end: start + "Б=2;".len(),
        }),
    };
    let response = crate::format(&request).unwrap();
    response.validate(&request).unwrap();
    assert_eq!(
        apply(source, &response),
        source.replacen("Б=2;", "\t\tБ = 2;", 1)
    );
}
