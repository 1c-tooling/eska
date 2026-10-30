//! Supplied module patterns exercise sibling groups, compact exits and multiline boundaries together.

use super::{Options, formatted, options};

/// The supplied background-task constructor is isolated and excluded from adjacent assignment tables.
#[test]
fn supplied_multiline_constructor_has_sibling_separators() {
    let source = "Начало=0;\n// Создает параметры потока.\nДанныеПотока    = Новый Структура(\n\"Договоры, Объекты, ДатаОкончания, ИдентификаторГлавногоЗадания\",\nДоговоры,\nОбъекты,\nПараметры.ДатаОкончания,\nКонтекст.ИдентификаторГлавногоЗадания\n);\nОбертка=Подготовить(ДанныеПотока);\nАдресРезультата=Сохранить();\n";
    let expected = "Начало = 0;\n\n// Создает параметры потока.\nДанныеПотока = Новый Структура(\n\t\"Договоры, Объекты, ДатаОкончания, ИдентификаторГлавногоЗадания\",\n\tДоговоры,\n\tОбъекты,\n\tПараметры.ДатаОкончания,\n\tКонтекст.ИдентификаторГлавногоЗадания\n);\n\nОбертка         = Подготовить(ДанныеПотока);\nАдресРезультата = Сохранить();\n";
    assert_eq!(formatted(source, options()), expected);
}

/// Newly wrapped instructions create their separators in the first pass; adjacent gaps merge once.
#[test]
fn newly_wrapped_and_adjacent_multiline_statements_share_one_gap() {
    let source =
        "Start();\nFirst=Call(\"abcdefghij\",\"klmnopqrst\");\nSecond=Call(\n1,\n2);\nFinish();\n";
    let output = formatted(
        source,
        Options {
            column_limit: 30,
            ..options()
        },
    );
    assert_eq!(
        output,
        "Start();\n\nFirst = Call(\n\t\"abcdefghij\",\n\t\"klmnopqrst\"\n);\n\nSecond = Call(\n\t1,\n\t2\n);\n\nFinish();\n"
    );
    assert!(!output.contains("\n\n\n"));
}

/// A query literal separates setup from execution, and the following standalone assignment has no pad.
#[test]
fn query_assignment_ends_horizontal_tables_even_when_gaps_are_disabled() {
    let source = "Запрос.Текст =\n\"ВЫБРАТЬ\n| 1\";\nВыборка      = Запрос.Выполнить().Выбрать();\nВыборка.Следующий();\n";
    let output = formatted(source, options());
    assert!(
        output.contains("\t| 1\";\n\nВыборка = Запрос.Выполнить().Выбрать();"),
        "{output}"
    );
    let output = formatted(
        source,
        Options {
            blank_lines_around_multiline_statements: 0,
            ..options()
        },
    );
    assert!(
        output.contains("\t| 1\";\nВыборка = Запрос.Выполнить().Выбрать();"),
        "{output}"
    );
}

/// The original single-return condition has no internal gap in either source language.
#[test]
fn supplied_single_return_branch_is_compact() {
    let source = "Функция СовпадаетСостав(Снимок, Текущие)\nЕсли Снимок.Количество() <> Текущие.Количество() Тогда\n\nВозврат Ложь;\n\nКонецЕсли;\nВозврат Истина;\nКонецФункции\n";
    let expected = "Функция СовпадаетСостав(Снимок, Текущие)\n\n\tЕсли Снимок.Количество() <> Текущие.Количество() Тогда\n\t\tВозврат Ложь;\n\tКонецЕсли;\n\n\tВозврат Истина;\n\nКонецФункции\n";
    assert_eq!(formatted(source, options()), expected);
    let output = formatted(
        "Procedure Test()\nIf Stop Then\n// Finished.\nReturn;\nElse\nWork();\nReturn;\nEndIf;\nEndProcedure\n",
        options(),
    );
    assert!(output.contains("If Stop Then\n\t\t// Finished.\n\t\tReturn;\n\tElse\n\n\t\tWork();\n\n\t\tReturn;\n\n\tEndIf;"), "{output}");
}

/// The exact registration and command population example becomes two independently aligned groups.
#[test]
fn supplied_registration_and_command_form_two_groups() {
    let source = "Функция СведенияОВнешнейОбработке() Экспорт\nРегистрация = ДополнительныеОтчетыИОбработки.СведенияОВнешнейОбработке(СтандартныеПодсистемыСервер.ВерсияБиблиотеки());\nРегистрация.Вид = ДополнительныеОтчетыИОбработкиКлиентСервер.ВидОбработкиДополнительнаяОбработка();\nРегистрация.Наименование = Метаданные().Представление();\nРегистрация.БезопасныйРежим = Ложь;\nРегистрация.Версия = Версия();\nРегистрация.Информация = Метаданные().Комментарий;\nКоманда = Регистрация.Команды.Добавить();\nКоманда.Представление = Регистрация.Наименование;\nКоманда.Идентификатор = Метаданные().Имя;\nКоманда.Использование = ДополнительныеОтчетыИОбработкиКлиентСервер.ТипКомандыОткрытиеФормы();\nКоманда.ПоказыватьОповещение = Ложь;\nВозврат Регистрация;\nКонецФункции\n";
    let expected = "Функция СведенияОВнешнейОбработке() Экспорт\n\n\tРегистрация = ДополнительныеОтчетыИОбработки.СведенияОВнешнейОбработке(СтандартныеПодсистемыСервер.ВерсияБиблиотеки());\n\tРегистрация.Вид             = ДополнительныеОтчетыИОбработкиКлиентСервер.ВидОбработкиДополнительнаяОбработка();\n\tРегистрация.Наименование    = Метаданные().Представление();\n\tРегистрация.БезопасныйРежим = Ложь;\n\tРегистрация.Версия          = Версия();\n\tРегистрация.Информация      = Метаданные().Комментарий;\n\n\tКоманда = Регистрация.Команды.Добавить();\n\tКоманда.Представление        = Регистрация.Наименование;\n\tКоманда.Идентификатор        = Метаданные().Имя;\n\tКоманда.Использование        = ДополнительныеОтчетыИОбработкиКлиентСервер.ТипКомандыОткрытиеФормы();\n\tКоманда.ПоказыватьОповещение = Ложь;\n\n\tВозврат Регистрация;\n\nКонецФункции\n";
    assert_eq!(formatted(source, options()), expected);
    assert_eq!(
        formatted(&source.replace('\n', "\r\n"), options()),
        expected.replace('\n', "\r\n")
    );
}

/// Receiver paths are syntax-based, case-insensitive and include nested fields and exact index values.
#[test]
fn population_groups_keep_comments_and_distinguish_receivers() {
    for receiver in [
        "Command",
        "Context.Command",
        "Commands[Index]",
        "Commands[\"A\"]",
    ] {
        let source = format!(
            "Previous.Value=1;\n// Creates a command.\n{receiver}=Factory();\n{receiver}.Name=1;\n{receiver}.Visible=2;\n"
        );
        let output = formatted(&source, options());
        assert!(
            output.starts_with("Previous.Value = 1;\n\n// Creates a command.\n"),
            "{output}"
        );
    }
    assert_eq!(
        formatted("Before();\nObject=Factory();\nobject.A=1;\n", options()),
        "Before();\n\nObject = Factory();\nobject.A = 1;\n"
    );
    for source in [
        "Before();\nObj=Factory();\nObj2.A=1;\n",
        "Before();\nRows[\"A\"]=Factory();\nRows[\"a\"].X=1;\n",
    ] {
        let output = formatted(source, options());
        assert!(output.starts_with("Before();\n"));
        assert!(!output.starts_with("Before();\n\n"), "{output}");
    }
}

/// Disabling the new spacing rules changes only gaps; multiline rows remain table boundaries.
#[test]
fn new_spacing_rules_have_independent_overrides() {
    let output = formatted(
        "Before();\nObject=Factory();\nObject.A=1;\nCall(\n1,\n2);\nAfter();\n",
        Options {
            blank_lines_around_multiline_statements: 0,
            blank_lines_between_object_groups: 0,
            ..options()
        },
    );
    assert_eq!(
        output,
        "Before();\nObject = Factory();\nObject.A = 1;\nCall(\n\t1,\n\t2\n);\nAfter();\n"
    );
}

/// Padding must not wrap a previously flat assignment and change the next formatting pass's groups.
#[test]
fn assignment_alignment_cannot_manufacture_an_overflow() {
    let source = "A=FirstValue+SecondValue;\nVeryLongVariable=1;\n";
    assert_eq!(
        formatted(
            source,
            Options {
                column_limit: 30,
                ..options()
            }
        ),
        "A = FirstValue + SecondValue;\nVeryLongVariable = 1;\n"
    );
}
