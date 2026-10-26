//! Reserved-token spelling is language-local; identifiers and member names are never renamed.

use eska_project_core::formatting::KeywordCase;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Key {
    If,
    Then,
    ElseIf,
    Else,
    EndIf,
    For,
    Each,
    In,
    To,
    While,
    Do,
    EndLoop,
    Procedure,
    EndProcedure,
    Function,
    EndFunction,
    Export,
    Var,
    Return,
    Break,
    Continue,
    Try,
    Except,
    EndTry,
    Raise,
    New,
    Execute,
    Goto,
    And,
    Or,
    Not,
    True,
    False,
    Undefined,
    Null,
    Async,
    Await,
    AddHandler,
    RemoveHandler,
    Val,
}

/// The fixed grammar table does not use platform metadata or infer identifier types.
const WORDS: &[(Key, &str, &str)] = &[
    (Key::If, "Если", "If"),
    (Key::Then, "Тогда", "Then"),
    (Key::ElseIf, "ИначеЕсли", "ElsIf"),
    (Key::Else, "Иначе", "Else"),
    (Key::EndIf, "КонецЕсли", "EndIf"),
    (Key::For, "Для", "For"),
    (Key::Each, "Каждого", "Each"),
    (Key::In, "Из", "In"),
    (Key::To, "По", "To"),
    (Key::While, "Пока", "While"),
    (Key::Do, "Цикл", "Do"),
    (Key::EndLoop, "КонецЦикла", "EndDo"),
    (Key::Procedure, "Процедура", "Procedure"),
    (Key::EndProcedure, "КонецПроцедуры", "EndProcedure"),
    (Key::Function, "Функция", "Function"),
    (Key::EndFunction, "КонецФункции", "EndFunction"),
    (Key::Export, "Экспорт", "Export"),
    (Key::Var, "Перем", "Var"),
    (Key::Return, "Возврат", "Return"),
    (Key::Break, "Прервать", "Break"),
    (Key::Continue, "Продолжить", "Continue"),
    (Key::Try, "Попытка", "Try"),
    (Key::Except, "Исключение", "Except"),
    (Key::EndTry, "КонецПопытки", "EndTry"),
    (Key::Raise, "ВызватьИсключение", "Raise"),
    (Key::New, "Новый", "New"),
    (Key::Execute, "Выполнить", "Execute"),
    (Key::Goto, "Перейти", "Goto"),
    (Key::And, "И", "And"),
    (Key::Or, "Или", "Or"),
    (Key::Not, "Не", "Not"),
    (Key::True, "Истина", "True"),
    (Key::False, "Ложь", "False"),
    (Key::Undefined, "Неопределено", "Undefined"),
    (Key::Null, "Null", "Null"),
    (Key::Async, "Асинх", "Async"),
    (Key::Await, "Ждать", "Await"),
    (Key::Val, "Знач", "Val"),
    (Key::AddHandler, "ДобавитьОбработчик", "AddHandler"),
    (Key::RemoveHandler, "УдалитьОбработчик", "RemoveHandler"),
];

/// Case-insensitive exact token matching does not mistake a substring for a keyword.
pub fn key(value: &str) -> Option<Key> {
    WORDS.iter().find_map(|(key, ru, en)| {
        (same(value, ru) || value.eq_ignore_ascii_case(en)).then_some(*key)
    })
}

/// Preserve the original language while normalizing only reserved words.
pub fn render(value: &str, key: Option<Key>, case: KeywordCase) -> String {
    let Some(key) = key else {
        return directive(value).map_or_else(
            || value.into(),
            |canonical| match case {
                KeywordCase::Canonical => canonical.into(),
                KeywordCase::Preserve => value.into(),
                KeywordCase::Lower => value.to_lowercase(),
                KeywordCase::Upper => value.to_uppercase(),
            },
        );
    };
    match case {
        KeywordCase::Preserve => value.into(),
        KeywordCase::Upper => value.to_uppercase(),
        KeywordCase::Lower => value.to_lowercase(),
        KeywordCase::Canonical => WORDS
            .iter()
            .find_map(|(candidate, ru, en)| {
                (*candidate == key).then_some(if value.is_ascii() { *en } else { *ru })
            })
            .unwrap_or(value)
            .into(),
    }
}

/// Known directive words use their own canonical spelling, preserving region names and language.
pub fn directive(value: &str) -> Option<&'static str> {
    if !value.starts_with('#') {
        return None;
    }
    match value.to_lowercase().as_str() {
        "#если" => Some("#Если"),
        "#иначеесли" => Some("#ИначеЕсли"),
        "#иначе" => Some("#Иначе"),
        "#конецесли" => Some("#КонецЕсли"),
        "#область" => Some("#Область"),
        "#конецобласти" => Some("#КонецОбласти"),
        "#if" => Some("#If"),
        "#elsif" => Some("#ElsIf"),
        "#else" => Some("#Else"),
        "#endif" => Some("#EndIf"),
        "#region" => Some("#Region"),
        "#endregion" => Some("#EndRegion"),
        "#вставка" => Some("#Вставка"),
        "#конецвставки" => Some("#КонецВставки"),
        "#удаление" => Some("#Удаление"),
        "#конецудаления" => Some("#КонецУдаления"),
        "#insert" => Some("#Insert"),
        "#endinsert" => Some("#EndInsert"),
        "#delete" => Some("#Delete"),
        "#enddelete" => Some("#EndDelete"),
        _ => None,
    }
}

/// Unicode lowercasing compares supported Russian spellings without changing source slices.
fn same(left: &str, right: &str) -> bool {
    left.chars()
        .flat_map(char::to_lowercase)
        .eq(right.chars().flat_map(char::to_lowercase))
}
