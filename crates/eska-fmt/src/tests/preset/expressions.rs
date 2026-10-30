//! Regression cases retain complete comparisons and the original expression grammar.

use super::{Options, Wrapping, formatted, options};

/// The reported return is repaired even when the input already contains broken comparison lines.
#[test]
fn supplied_contract_comparisons_are_atomic() {
    let source = "Функция РеквизитыНеИзменились(Объект, Снимок, ДатаОкончания)\nВозврат Объект.Владелец\n= Снимок.Контрагент\nИ Объект.Номер\n= Снимок.Номер\nИ Объект.Наименование\n= Снимок.Наименование\nИ Объект.Дата\n= Снимок.Дата\nИ Объект.Код\n= Снимок.Код\nИ Объект.Дата\n<= ДатаОкончания;\nКонецФункции\n";
    let expected = "Функция РеквизитыНеИзменились(Объект, Снимок, ДатаОкончания)\n\n\tВозврат Объект.Владелец = Снимок.Контрагент\n\t\tИ Объект.Номер = Снимок.Номер\n\t\tИ Объект.Наименование = Снимок.Наименование\n\t\tИ Объект.Дата = Снимок.Дата\n\t\tИ Объект.Код = Снимок.Код\n\t\tИ Объект.Дата <= ДатаОкончания;\n\nКонецФункции\n";
    assert_eq!(formatted(source, options()), expected);
    assert_eq!(
        formatted(&source.replace('\n', "\r\n"), options()),
        expected.replace('\n', "\r\n")
    );
}

/// A call followed by a comparison remains a whole logical condition beside other alternatives.
#[test]
fn call_comparison_stays_with_its_operand() {
    let source = "Если Не Проверить(Объект, Снимок)\nИли Не ПроверитьСостав(Снимок, Текущие)\nИли КоличествоОставшихсяДоговоров(Договор, ДатаОкончания)\n< 2 Тогда\nДействие();\nКонецЕсли;\n";
    let output = formatted(source, options());
    assert!(
        output.contains("Или КоличествоОставшихсяДоговоров(Договор, ДатаОкончания) < 2 Тогда"),
        "{output}"
    );
}

/// OR splits before AND, comparisons and arithmetic; parentheses retain their exact meaning.
#[test]
fn nested_priorities_and_parentheses_keep_comparisons_whole() {
    let source = "Result = First.Value = A + B And Second.Value = C * D Or (Third.Value = E And Fourth.Value <> F);\n";
    let output = formatted(
        source,
        Options {
            column_limit: 55,
            ..options()
        },
    );
    assert!(output.contains("First.Value = A + B"), "{output}");
    assert!(output.contains("And Second.Value = C * D"), "{output}");
    assert!(output.contains("Or (Third.Value = E"), "{output}");
    assert!(output.contains("And Fourth.Value <> F)"), "{output}");
}

/// An already short comparison is normalized while explicit preserve and one-per-line remain exact.
#[test]
fn comparison_normalization_respects_explicit_wrapping_modes() {
    let source = "Result = Object.Value\n= Snapshot.Value;\n";
    assert_eq!(
        formatted(source, options()),
        "Result = Object.Value = Snapshot.Value;\n"
    );
    for mode in [Wrapping::Preserve, Wrapping::OnePerLine] {
        let output = formatted(
            source,
            Options {
                wrap_binary: mode,
                ..options()
            },
        );
        assert!(
            output.contains("Object.Value\n\t= Snapshot.Value"),
            "{output}"
        );
    }
    assert_eq!(
        formatted("Call(A\n= B);\n", options()),
        "Call(\n\tA = B\n);\n"
    );
}

/// Long arithmetic falls back to its own precedence tier without splitting every nested operator.
#[test]
fn arithmetic_wraps_at_addition_before_multiplication() {
    let output = formatted(
        "Result = FirstValue * SecondValue + ThirdValue * FourthValue;\n",
        Options {
            column_limit: 40,
            ..options()
        },
    );
    assert_eq!(
        output,
        "Result = FirstValue * SecondValue\n\t+ ThirdValue * FourthValue;\n"
    );
}

/// Nested expressions remain stable across widths, continuation directions and manual-break policy.
#[test]
fn expression_layout_is_stable_across_fit_options() {
    for source in [
        "Result = LongFirst = LongSecond And LongThird = LongFourth Or LongFifth = LongSixth;\n",
        "Result = (LongFirst = LongSecond Or LongThird = LongFourth) And LongFifth = LongSixth;\n",
        "Result = First(Alpha, Beta) + Second(Gamma, Delta) * Third(Epsilon, Zeta);\n",
        "Call(Object.Value = Snapshot.Value, X + Y * Z);\n",
        "Result = -FirstValue + -SecondValue * ThirdValue;\n",
        "Result = A = B\nAnd C\n<> D;\n",
    ] {
        for column_limit in [0, 24, 40, 80] {
            for break_before_binary_operators in [false, true] {
                for preserve_existing_line_breaks in [false, true] {
                    formatted(
                        source,
                        Options {
                            column_limit,
                            break_before_binary_operators,
                            preserve_existing_line_breaks,
                            ..options()
                        },
                    );
                }
            }
        }
    }
}
