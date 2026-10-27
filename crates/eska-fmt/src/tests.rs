//! Black-box formatter tests apply the real edit protocol and require a stable second pass.

use eska_project_core::formatting::{
    Language, Options,
    protocol::{Request, Response},
};

/// Native consumers apply the same byte edits to a checked source snapshot.
fn apply(input: &str, response: &Response) -> String {
    let mut output = input.to_owned();
    for edit in response.edits.iter().rev() {
        output.replace_range(edit.range.start..edit.range.end, &edit.text);
    }
    output
}

/// Every accepted example must be idempotent and retain its significant token stream.
fn formatted(input: &str, options: Options) -> String {
    let mut request = Request {
        schema_version: 1,
        language: Language::Bsl,
        text: input.into(),
        options,
        range: None,
    };
    let response = super::format(&request).unwrap_or_else(|error| panic!("{error}: {input:?}"));
    response
        .validate(&request)
        .expect("safe response coordinates");
    let output = apply(input, &response);
    request.text.clone_from(&output);
    assert!(
        super::format(&request).unwrap().edits.is_empty(),
        "not idempotent: {output:?}"
    );
    output
}

/// Ordinary nested code uses one tab per block and canonical reserved words.
#[test]
fn default_bsl_profile_formats_blocks_calls_and_operators() {
    let input = "процедура Проверить(Знач А=Неопределено) экспорт\nесли А<>Неопределено тогда\nРезультат=Метод(А,1+2);\nиначе\nРезультат=0;\nконецесли;\nконецпроцедуры";
    let expected = "Процедура Проверить(Знач А = Неопределено) Экспорт\n\tЕсли А <> Неопределено Тогда\n\t\tРезультат = Метод(А, 1 + 2);\n\tИначе\n\t\tРезультат = 0;\n\tКонецЕсли;\nКонецПроцедуры\n";
    assert_eq!(formatted(input, Options::default()), expected);
}

/// Literal values, escaped quotes and source comments between continuation lines are opaque.
#[test]
fn multiline_literals_and_comments_are_preserved_exactly() {
    let literal =
        "\"Строка \"\"в кавычках\"\"\n // исходный комментарий с \"кавычкой\n |продолжение  \"";
    let input = format!("Процедура Тест()\nТекст={literal}; // значение  \nКонецПроцедуры\n");
    let result = formatted(&input, Options::default());
    assert!(result.contains(literal));
    assert!(result.contains("// значение  \n"));
}

/// Frozen comments cannot be forged by a matching string literal and must form complete pairs.
#[test]
fn off_markers_preserve_original_bytes() {
    let frozen = "// eska-fmt off\n   А=  1;\n// eska-fmt on";
    let input =
        format!("Процедура Тест()\n{frozen}\nБ=2;\nТекст=\"// eska-fmt off\";\nКонецПроцедуры\n");
    let output = formatted(&input, Options::default());
    assert!(output.contains(frozen));
    assert!(output.contains("\tБ = 2;"));
    for input in [
        "// eska-fmt off\nА=1;",
        "// eska-fmt on",
        "// eska-fmt off\n// eska-fmt off\n// eska-fmt on\n// eska-fmt on",
    ] {
        let request = Request {
            schema_version: 1,
            language: Language::Bsl,
            text: input.into(),
            options: Options::default(),
            range: None,
        };
        assert_eq!(
            super::format(&request).unwrap_err().code,
            "invalid_format_marker"
        );
    }
}

/// Branch indentation is independent of preprocessor nesting and region names remain identifiers.
#[test]
fn preprocessor_branches_merge_without_project_analysis() {
    let input = "#Область Test\n#Если Сервер Тогда\nпроцедура Тест()\nА=1;\nконецпроцедуры\n#Иначе\nпроцедура Тест()\nА=2;\nконецпроцедуры\n#КонецЕсли\n#КонецОбласти\n";
    let result = formatted(input, Options::default());
    assert!(result.contains("\n#Иначе\n"));
    assert!(result.contains("\n\tА = 1;\n"));
    assert!(result.contains("#Область Test"));
}

/// Long calls are wrapped by syntax groups, with omitted argument positions retained.
#[test]
fn argument_layout_is_configurable() {
    let input = "Процедура Тест()\nОтвет=Метод(ПервыйАргумент,,ВторойАргумент,Вложенный(1,2));\nКонецПроцедуры\n";
    let output = formatted(
        input,
        Options {
            column_limit: 40,
            ..Options::default()
        },
    );
    assert!(output.contains("Метод(\n"), "{output}");
    assert!(output.contains("\n\t\t,\n"), "{output}");
}

/// Invalid syntax never produces a partial edit or silently invents a missing token.
#[test]
fn malformed_sources_fail_without_a_plan() {
    for input in [
        "А=;",
        "Если А Тогда\nА=1;",
        "А=Метод(1,2;",
        "Процедура Тест()\nА=1 2;\nКонецПроцедуры",
        "Если А Тогда\nКонецЦикла;",
        "// eska-fmt on",
    ] {
        let request = Request {
            schema_version: 1,
            language: Language::Bsl,
            text: input.into(),
            options: Options::default(),
            range: None,
        };
        assert!(super::format(&request).is_err(), "{input}");
    }
}

/// English spelling, metadata member names and dynamic constructor syntax remain independent.
#[test]
fn english_keywords_do_not_rename_identifiers_or_member_names() {
    let source = "procedure Demo(Val value=Undefined) export\nObject.если=value;\nResult=new Array;\nif value<>Undefined then\nResult.Add(value);\nendif;\nendprocedure\n";
    let output = formatted(source, Options::default());
    assert!(output.starts_with("Procedure Demo(Val value = Undefined) Export\n"));
    assert!(output.contains("Object.если = value;"));
    assert!(output.contains("Result = New Array;"));
}

/// Numeric loops, collection loops and exception branches use the actual block stack.
#[test]
fn loops_and_exception_branches_have_consistent_indentation() {
    let source = "Процедура Тест()\nДля Индекс=1 По 3 Цикл\nПопытка\nЕсли Индекс=2 Тогда Продолжить; КонецЕсли;\nВызватьИсключение \"Ошибка\";\nИсключение\nПрервать;\nКонецПопытки;\nКонецЦикла;\nДля Каждого Элемент Из Новый Массив Цикл\nМетод(Элемент);\nКонецЦикла;\nКонецПроцедуры\n";
    let output = formatted(source, Options::default());
    assert!(output.contains("\t\tИсключение\n\t\t\tПрервать;\n"));
    assert!(output.contains("Для Каждого Элемент Из Новый Массив Цикл"));
}

/// Independent spacing switches produce compact tokens while boolean words keep delimiters.
#[test]
fn spacing_and_parentheses_options_affect_real_output() {
    let source = "Процедура Тест()\nА = -1 + Метод(2, 3);\nЕсли А = 1 И Б = 2 Тогда\nМетод();\nКонецЕсли;\nКонецПроцедуры";
    let compact = formatted(
        source,
        Options {
            space_after_comma: false,
            space_around_operators: false,
            insert_final_newline: false,
            ..Options::default()
        },
    );
    assert!(compact.contains("А=-1+Метод(2,3);"), "{compact}");
    assert!(compact.contains("Если А=1 И Б=2 Тогда"));
    assert!(!compact.ends_with('\n'));
    let spaced = formatted(
        "Метод(1,2);",
        Options {
            space_inside_parentheses: true,
            space_before_call_parenthesis: true,
            ..Options::default()
        },
    );
    assert_eq!(spaced, "Метод ( 1, 2 );\n");
}

/// Assignment columns are scoped to adjacent statements and remain stable after wrapping.
#[test]
fn assignment_alignment_and_spaces_have_explicit_widths() {
    let source = "Процедура Тест()\nА=1;\nДлинноеИмя=2;\nКонецПроцедуры\n";
    let options = Options {
        indent_style: eska_project_core::formatting::IndentStyle::Space,
        indent_width: 2,
        align_assignments: true,
        ..Options::default()
    };
    assert_eq!(
        formatted(source, options),
        "Процедура Тест()\n  А          = 1;\n  ДлинноеИмя = 2;\nКонецПроцедуры\n"
    );
}

/// Forced binary layout can put operators before or after a continuation independently of fit.
#[test]
fn binary_layout_preserves_expression_precedence() {
    let source = "Результат=Первое+Второе*Третье;";
    let leading = formatted(
        source,
        Options {
            wrap_binary: eska_project_core::formatting::Wrapping::OnePerLine,
            ..Options::default()
        },
    );
    assert_eq!(leading, "Результат = Первое\n\t+ Второе\n\t* Третье;\n");
    let trailing = formatted(
        source,
        Options {
            wrap_binary: eska_project_core::formatting::Wrapping::OnePerLine,
            break_before_binary_operators: false,
            ..Options::default()
        },
    );
    assert_eq!(trailing, "Результат = Первое +\n\tВторое *\n\tТретье;\n");
}

/// Range formatting expands one complete statement and preserves every surrounding byte.
#[test]
fn range_changes_only_its_reported_unit() {
    let input = "Процедура Тест()\nА=1;\nБ=2;\nКонецПроцедуры\n";
    let start = input.find("Б=").unwrap();
    let request = Request {
        schema_version: 1,
        language: Language::Bsl,
        text: input.into(),
        options: Options::default(),
        range: Some(eska_project_core::formatting::protocol::Range {
            start,
            end: start + "Б=2;".len(),
        }),
    };
    let response = super::format(&request).unwrap();
    assert_eq!(
        apply(input, &response),
        "Процедура Тест()\nА=1;\n\tБ = 2;\nКонецПроцедуры\n"
    );
    for edit in response.edits {
        assert!(
            edit.range.start >= response.formatted_range.start
                && edit.range.end <= response.formatted_range.end
        );
    }
}

/// BOM and CRLF survive both ordinary layout changes and untouched multiline literal bodies.
#[test]
fn encoding_and_line_endings_do_not_follow_the_host() {
    let input = "\u{feff}процедура Тест()\r\nДата='20261027';\r\nконецпроцедуры\r\n";
    let output = formatted(input, Options::default());
    assert_eq!(
        output,
        "\u{feff}Процедура Тест()\r\n\tДата = '20261027';\r\nКонецПроцедуры\r\n"
    );
}

/// Annotation names and argument literals are preserved while the method stays at column zero.
#[test]
fn annotations_and_method_separation_respect_the_profile() {
    let source = "  &НаСервереБезКонтекста\nпроцедура Первая()\nКонецПроцедуры\n&После(\"ПриОткрытии\")\nпроцедура Вторая()\nКонецПроцедуры";
    let output = formatted(
        source,
        Options {
            blank_lines_between_methods: 2,
            ..Options::default()
        },
    );
    assert_eq!(
        output,
        "&НаСервереБезКонтекста\nПроцедура Первая()\nКонецПроцедуры\n\n\n&После(\"ПриОткрытии\")\nПроцедура Вторая()\nКонецПроцедуры\n"
    );
}

/// Deterministic malformed streams must return a result or error instead of indexing past input.
#[test]
fn malformed_token_streams_never_panic() {
    let pieces = [
        "Если",
        "Тогда",
        "КонецЕсли",
        "Процедура",
        "Асинх",
        "Перем",
        "Для",
        "Каждого",
        "Из",
        "Цикл",
        "(",
        ")",
        "[",
        "]",
        "=",
        ";",
        ",",
        "А",
        "1",
        "#Иначе",
        "Возврат",
    ];
    let mut seed = 1_u64;
    for _ in 0..1000 {
        let mut input = String::new();
        for _ in 0..12 {
            seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
            let index = usize::try_from(seed % u64::try_from(pieces.len()).unwrap()).unwrap();
            input.push_str(pieces[index]);
            input.push(' ');
        }
        let request = Request {
            schema_version: 1,
            language: Language::Bsl,
            text: input.clone(),
            options: Options::default(),
            range: None,
        };
        assert!(
            std::panic::catch_unwind(|| super::format(&request)).is_ok(),
            "{input}"
        );
    }
}

/// A cursor immediately after a semicolon belongs to that statement, not the following line.
#[test]
fn empty_range_at_a_statement_end_keeps_the_following_unit_unchanged() {
    let input = "А=1;\nБ=2;\n";
    let cursor = input.find(';').unwrap() + 1;
    let request = Request {
        schema_version: 1,
        language: Language::Bsl,
        text: input.into(),
        options: Options::default(),
        range: Some(eska_project_core::formatting::protocol::Range {
            start: cursor,
            end: cursor,
        }),
    };
    assert_eq!(
        apply(input, &super::format(&request).unwrap()),
        "А = 1;\nБ=2;\n"
    );
}

/// Disabling whitespace trimming keeps trivia on every retained line, including blank lines.
#[test]
fn whitespace_and_comment_preservation_are_explicit() {
    let source = "А=1;  \n \t\nБ=2;   ";
    let output = formatted(
        source,
        Options {
            trim_trailing_whitespace: false,
            ..Options::default()
        },
    );
    assert_eq!(output, "А = 1;  \n \t\nБ = 2;   \n");
    let commented = "   // комментарий\n А=1; // рядом\n";
    assert_eq!(
        formatted(
            commented,
            Options {
                comments: eska_project_core::formatting::Comments::Preserve,
                ..Options::default()
            }
        ),
        commented
    );
}

/// Preserve layout retains deliberate call and binary breaks even when the line would fit.
#[test]
fn preserve_modes_retain_explicit_breaks() {
    let source = "А=Метод(\n 1,\n 2);\nБ=Первое\n+Второе;\n";
    let options = Options {
        wrap_arguments: eska_project_core::formatting::Wrapping::Preserve,
        wrap_binary: eska_project_core::formatting::Wrapping::Preserve,
        ..Options::default()
    };
    assert_eq!(
        formatted(source, options),
        "А = Метод(\n\t1,\n\t2);\nБ = Первое\n\t+ Второе;\n"
    );
}

/// Inline argument comments stay with their preceding comma, value or opening delimiter.
#[test]
fn call_comments_keep_their_attachment_and_cannot_swallow_delimiters() {
    for source in [
        "Метод(1, // после первого аргумента\n2);",
        "Метод(1 // перед запятой\n,2);",
        "Метод(1,2 // перед скобкой\n);",
        "Метод(// описание вызова\n1,2);",
        "Метод(// пустой вызов\n);",
    ] {
        let output = formatted(source, Options::default());
        if source.contains("после первого") {
            assert!(
                output.contains("1, // после первого аргумента\n"),
                "{output}"
            );
        }
        if source.contains("описание вызова") {
            assert!(output.contains("Метод( // описание вызова\n"), "{output}");
        }
    }
}

/// Deleted extension control text is frozen; only inserted active syntax affects indentation.
#[test]
fn extension_change_control_preserves_the_deleted_reference_text() {
    let deleted = "#Удаление\n   Если СтароеУсловие Тогда\n#КонецУдаления";
    let input = format!(
        "&ИзменениеИКонтроль(\"Run\")\nПроцедура НовоеИмя()\n{deleted}\n#Вставка\nесли НовоеУсловие тогда\n#КонецВставки\nА=1;\nКонецЕсли;\nКонецПроцедуры\n"
    );
    let output = formatted(&input, Options::default());
    assert!(output.contains(deleted));
    assert!(
        output.contains("#Вставка\n\tЕсли НовоеУсловие Тогда\n#КонецВставки\n\t\tА = 1;\n"),
        "{output}"
    );
}

/// Empty operators are legal after a method header and remain separate significant tokens.
#[test]
fn empty_statements_after_method_headers_are_preserved() {
    assert_eq!(
        formatted(
            "Процедура Тест();\n;А=1;;\nКонецПроцедуры\n",
            Options::default()
        ),
        "Процедура Тест()\n\t;\n\t;\n\tА = 1;\n\t;\nКонецПроцедуры\n"
    );
}

/// Own-line comments in a continued condition must not become inline and shift the next pass.
#[test]
fn conditional_comments_have_stable_continuation_indentation() {
    let source = "Процедура Тест()\nЕсли (Первое = ОченьДлинныйМетод(1) // пояснение первого\n // отдельное пояснение\n Или Второе = ОченьДлинныйМетод(2)) // пояснение второго\n И Третье Тогда\nМетод();\nКонецЕсли;\nКонецПроцедуры\n";
    let output = formatted(
        source,
        Options {
            column_limit: 40,
            ..Options::default()
        },
    );
    assert!(
        output.contains("\n\t\t// отдельное пояснение\n"),
        "{output}"
    );
}
