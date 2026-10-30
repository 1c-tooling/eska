//! Object construction and subsequent population form distinct horizontal alignment groups.

use super::{Options, Range, Request, apply, formatted, options};

const SOURCE: &str = "ДокументОбъект = МенеджерОбъекта.СоздатьДокумент();\nДокументОбъект.Дата                = ТекущаяДатаСеанса();\nДокументОбъект.Комментарий = \"Ляляля\";\n";
const EXPECTED: &str = "ДокументОбъект = МенеджерОбъекта.СоздатьДокумент();\nДокументОбъект.Дата        = ТекущаяДатаСеанса();\nДокументОбъект.Комментарий = \"Ляляля\";\n";

/// The supplied example retains ordinary initializer spacing and aligns the two fields separately.
#[test]
fn object_initialization_is_not_padded_to_its_field_names() {
    assert_eq!(formatted(SOURCE, options()), EXPECTED);
}

/// Intermediate assignments do not hide the receiver dependency within the same adjacent table.
#[test]
fn initializer_is_isolated_from_both_neighboring_assignment_groups() {
    let source = "A=1;\nLong=2;\nObj=Factory();\nFlag=1;\nObj.Short=2;\nObj.LongName=3;\n";
    assert_eq!(
        formatted(
            source,
            Options {
                blank_lines_between_object_groups: 0,
                ..options()
            }
        ),
        "A    = 1;\nLong = 2;\nObj = Factory();\nFlag         = 1;\nObj.Short    = 2;\nObj.LongName = 3;\n"
    );
}

/// Nested fields and collection elements use whole token prefixes, with case-insensitive names.
#[test]
fn nested_and_indexed_initializers_are_recognized_without_analyzer_types() {
    for target in ["Context.Doc", "Objects[Index]", "Objects[\"Имя\"]"] {
        let source = format!("{target}=Factory();\n{target}.A=1;\n{target}.Long=2;\n");
        let expected = format!("{target} = Factory();\n{target}.A    = 1;\n{target}.Long = 2;\n");
        assert_eq!(formatted(&source, options()), expected);
    }
    assert_eq!(
        formatted(
            "Объект=Создать();\nобъект.Дата=1;\nобъект.Комментарий=2;\n",
            options()
        ),
        "Объект = Создать();\nобъект.Дата        = 1;\nобъект.Комментарий = 2;\n"
    );
    assert_eq!(
        formatted(
            "Values=New Array;\nValues[0]=1;\nValues[Index]=2;\n",
            options()
        ),
        "Values = New Array;\nValues[0]     = 1;\nValues[Index] = 2;\n"
    );
}

/// Similar identifier spellings and differently cased index literals are distinct receivers.
#[test]
fn prefix_collisions_and_index_literals_do_not_split_unrelated_tables() {
    let policy = Options {
        blank_lines_between_object_groups: 0,
        ..options()
    };
    assert_eq!(
        formatted("Obj=Factory();\nObj2.A=1;\nObj2.Long=2;\n", policy.clone()),
        "Obj       = Factory();\nObj2.A    = 1;\nObj2.Long = 2;\n"
    );
    assert_eq!(
        formatted(
            "Rows[\"A\"]=Factory();\nRows[\"a\"].X=1;\nRows[\"a\"].Long=2;\n",
            policy
        ),
        "Rows[\"A\"]      = Factory();\nRows[\"a\"].X    = 1;\nRows[\"a\"].Long = 2;\n"
    );
}

/// Adjacent words in an index expression cannot collide with a single longer identifier.
#[test]
fn index_expression_token_boundaries_are_distinct() {
    let source = "Rows[New Item]=Factory();\nRows[NewItem].X=1;\nRows[NewItem].Long=2;\n";
    assert_eq!(
        formatted(source, options()),
        formatted(
            source,
            Options {
                separate_object_initialization: false,
                ..options()
            }
        )
    );
}

/// Explicit false restores the old table, while disabling assignment alignment disables both rules.
#[test]
fn initialization_separation_is_optional_and_requires_assignment_alignment() {
    let previous = formatted(
        SOURCE,
        Options {
            separate_object_initialization: false,
            ..options()
        },
    );
    assert!(
        previous.starts_with("ДокументОбъект             = МенеджерОбъекта.СоздатьДокумент();"),
        "{previous}"
    );
    assert_eq!(
        formatted(
            SOURCE,
            Options {
                align_assignments: true,
                ..Options::default()
            }
        ),
        previous
    );
    let plain = formatted(
        SOURCE,
        Options {
            align_assignments: false,
            ..options()
        },
    );
    assert!(
        plain.contains("ДокументОбъект.Дата = ТекущаяДатаСеанса();"),
        "{plain}"
    );
}

/// Source-local alignment plans can format the initializer without changing any field assignment.
#[test]
fn selected_initializer_preserves_the_unselected_field_population() {
    let end = SOURCE.find('\n').unwrap();
    let input = SOURCE.replacen("ДокументОбъект =", "ДокументОбъект    =", 1);
    let request = Request {
        schema_version: 1,
        language: super::Language::Bsl,
        text: input.clone(),
        options: options(),
        range: Some(Range { start: 0, end }),
    };
    let response = crate::format(&request).unwrap();
    response.validate(&request).unwrap();
    assert_eq!(apply(&input, &response), SOURCE);
}
