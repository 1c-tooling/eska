//! Opt-in real-platform acceptance compares evaluated baseline and formatted modules.

use eska_project_core::formatting::{Language, Options, Wrapping, protocol::Request};
use std::{
    io::Write,
    path::Path,
    process::{Command, Stdio},
};

/// The native harness receives exact BOM/CRLF snapshots produced by the real library.
#[test]
#[ignore = "requires ESKA_FMT_NATIVE_SEED, ESKA_FMT_NATIVE_BUILDER, ESKA_FMT_NATIVE_PLATFORM and ESKA_FMT_NATIVE_DISTROBOX"]
fn platform_evaluates_the_same_results_before_and_after_formatting() {
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let mut payload = Vec::new();
    for (name, source, expected, narrow) in cases() {
        let before = format!(
            "\u{feff}// Проверка сохранения результата форматирования.\r\n{}",
            source.replace('\n', "\r\n")
        );
        let options = Options {
            column_limit: if narrow { 32 } else { 120 },
            wrap_binary: if narrow {
                Wrapping::OnePerLine
            } else {
                Wrapping::Fit
            },
            ..Options::default()
        };
        let request = Request {
            schema_version: 1,
            language: Language::Bsl,
            text: before.clone(),
            options,
            range: None,
        };
        let response = eska_fmt::format(&request).unwrap();
        let mut after = before.clone();
        for edit in response.edits.iter().rev() {
            after.replace_range(edit.range.start..edit.range.end, &edit.text);
        }
        assert!(!response.edits.is_empty(), "{name}");
        let repeat = Request {
            text: after.clone(),
            ..request
        };
        assert!(
            eska_fmt::format(&repeat).unwrap().edits.is_empty(),
            "{name}"
        );
        payload.push(
            serde_json::json!({"name":name,"before":before,"after":after,"expected":expected}),
        );
    }
    let mut child = Command::new("python3")
        .arg(repository.join("tests/native_formatting.py"))
        .args([
            "--seed-xml",
            &required("ESKA_FMT_NATIVE_SEED"),
            "--build-binary",
            &required("ESKA_FMT_NATIVE_BUILDER"),
            "--platform-version",
            &required("ESKA_FMT_NATIVE_PLATFORM"),
            "--distrobox",
            &required("ESKA_FMT_NATIVE_DISTROBOX"),
        ])
        .current_dir(repository.parent().unwrap().join("eska-playground"))
        .stdin(Stdio::piped())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(&serde_json::to_vec(&payload).unwrap())
        .unwrap();
    assert!(child.wait().unwrap().success());
}

/// Missing test configuration must fail rather than silently choosing a user's live infobase.
fn required(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("missing {name}"))
}

/// Each independent module returns a scalar value that the platform checks against a fixed result.
fn cases() -> Vec<(&'static str, &'static str, i64, bool)> {
    vec![
        (
            "empty_statements",
            "функция Run() экспорт;\n;А=1;;Возврат А;конецфункции\n",
            1,
            false,
        ),
        (
            "call_comments",
            "// Складывает два аргумента.\nфункция Сложить(А,Б)\nВозврат А+Б;конецфункции\n// Проверяет комментарии после запятой и перед закрывающей скобкой.\nфункция Run() экспорт\nВозврат Сложить(1, // первый аргумент\n2 // второй аргумент\n);конецфункции\n",
            3,
            false,
        ),
        (
            "loops_ru",
            "функция Run() экспорт\nИтог=0;Для Индекс=1 По 3 Цикл Итог=Итог+Индекс;КонецЦикла;Возврат Итог;конецфункции\n",
            6,
            false,
        ),
        (
            "loops_en",
            "function Run() export\nResult=0;For Index=1 To 3 Do Result=Result+Index;EndDo;Return Result;endfunction\n",
            6,
            false,
        ),
        (
            "omitted_arguments",
            "// Складывает переданные значения и значения по умолчанию.\nфункция Сложить(Первое=1,Второе=2,Третье=3)\nвозврат Первое+Второе+Третье;конецфункции\n// Возвращает результат вызова с пропущенными аргументами.\nфункция Run() экспорт\nвозврат Сложить(,10,);конецфункции\n",
            14,
            true,
        ),
        (
            "precedence",
            "функция Run() экспорт\nПервое=2;Второе=3;Третье=4;Результат=Первое+Второе*Третье;возврат Результат;конецфункции\n",
            14,
            true,
        ),
        (
            "conditions",
            "функция Run() экспорт\nА=1;Б=2;Если А=1 И (Б=2 Или Б=3) Тогда Возврат 42;Иначе Возврат 0;КонецЕсли;конецфункции\n",
            42,
            true,
        ),
        (
            "try_except",
            "функция Run() экспорт\nПопытка ВызватьИсключение \"Проверка\";Исключение Возврат 7;КонецПопытки;Возврат 0;конецфункции\n",
            7,
            false,
        ),
        (
            "dates",
            "функция Run() экспорт\nДатаПроверки='20261027';Возврат Год(ДатаПроверки);конецфункции\n",
            2026,
            false,
        ),
        (
            "multiline_literal",
            "функция Run() экспорт\nТекст=\"А\"\"Б\n // комментарий с \"кавычкой\n |В\";Возврат СтрДлина(Текст);конецфункции\n",
            5,
            false,
        ),
        (
            "constructors",
            "функция Run() экспорт\nМассив=Новый Массив;Массив.Добавить(1);Массив.Добавить(2);Возврат Массив.Количество();конецфункции\n",
            2,
            false,
        ),
        (
            "preprocessor",
            "#Область Проверка\n#Если Клиент Тогда\nфункция Run() экспорт\nВозврат 11;конецфункции\n#Иначе\nфункция Run() экспорт\nВозврат 0;конецфункции\n#КонецЕсли\n#КонецОбласти\n",
            11,
            false,
        ),
        (
            "disabled_range",
            "функция Run() экспорт\n// eska-fmt off\n  А= 2;\n// eska-fmt on\nВозврат А;конецфункции\n",
            2,
            false,
        ),
    ]
}
