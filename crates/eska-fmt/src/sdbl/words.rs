//! SQL spelling depends on lexical roles, so aliases, parameters and metadata paths stay intact.

use crate::lex::{Kind, Token};
use eska_project_core::formatting::KeywordCase;

const KEYWORDS: &[&str] = &[
    "SELECT",
    "ВЫБРАТЬ",
    "ALLOWED",
    "РАЗРЕШЕННЫЕ",
    "DISTINCT",
    "РАЗЛИЧНЫЕ",
    "TOP",
    "ПЕРВЫЕ",
    "FROM",
    "ИЗ",
    "AS",
    "КАК",
    "INTO",
    "ПОМЕСТИТЬ",
    "LEFT",
    "ЛЕВОЕ",
    "RIGHT",
    "ПРАВОЕ",
    "FULL",
    "ПОЛНОЕ",
    "INNER",
    "ВНУТРЕННЕЕ",
    "OUTER",
    "ВНЕШНЕЕ",
    "JOIN",
    "СОЕДИНЕНИЕ",
    "ON",
    "ПО",
    "WHERE",
    "ГДЕ",
    "GROUP",
    "СГРУППИРОВАТЬ",
    "BY",
    "ORDER",
    "УПОРЯДОЧИТЬ",
    "HAVING",
    "ИМЕЮЩИЕ",
    "DESC",
    "УБЫВ",
    "ASC",
    "ВОЗР",
    "TOTALS",
    "ИТОГИ",
    "OVERALL",
    "ОБЩИЕ",
    "HIERARCHY",
    "ИЕРАРХИЯ",
    "PERIODS",
    "ПЕРИОДАМИ",
    "INDEX",
    "ИНДЕКСИРОВАТЬ",
    "UNION",
    "ОБЪЕДИНИТЬ",
    "ALL",
    "ВСЕ",
    "DROP",
    "УНИЧТОЖИТЬ",
    "AUTOORDER",
    "АВТОУПОРЯДОЧИВАНИЕ",
    "FOR",
    "ДЛЯ",
    "UPDATE",
    "ИЗМЕНЕНИЯ",
    "CASE",
    "ВЫБОР",
    "WHEN",
    "КОГДА",
    "THEN",
    "ТОГДА",
    "ELSE",
    "ИНАЧЕ",
    "END",
    "КОНЕЦ",
    "AND",
    "И",
    "OR",
    "ИЛИ",
    "NOT",
    "НЕ",
    "IN",
    "В",
    "BETWEEN",
    "МЕЖДУ",
    "LIKE",
    "ПОДОБНО",
    "ESCAPE",
    "СПЕЦСИМВОЛ",
    "IS",
    "ЕСТЬ",
    "NULL",
    "TRUE",
    "ИСТИНА",
    "FALSE",
    "ЛОЖЬ",
    "REFS",
    "ССЫЛКА",
    "ONLY",
    "ТОЛЬКО",
];

const FUNCTIONS: &[&str] = &[
    "SUM",
    "СУММА",
    "COUNT",
    "КОЛИЧЕСТВО",
    "MIN",
    "МИНИМУМ",
    "MAX",
    "МАКСИМУМ",
    "AVG",
    "СРЕДНЕЕ",
    "ISNULL",
    "ЕСТЬNULL",
    "CAST",
    "ВЫРАЗИТЬ",
    "VALUE",
    "ЗНАЧЕНИЕ",
    "TYPE",
    "ТИП",
    "VALUETYPE",
    "ТИПЗНАЧЕНИЯ",
    "PRESENTATION",
    "ПРЕДСТАВЛЕНИЕ",
    "REFPRESENTATION",
    "ПРЕДСТАВЛЕНИЕССЫЛКИ",
    "DATETIME",
    "ДАТАВРЕМЯ",
    "YEAR",
    "ГОД",
    "QUARTER",
    "КВАРТАЛ",
    "MONTH",
    "МЕСЯЦ",
    "DAYOFYEAR",
    "ДЕНЬГОДА",
    "DAY",
    "ДЕНЬ",
    "WEEK",
    "НЕДЕЛЯ",
    "WEEKDAY",
    "ДЕНЬНЕДЕЛИ",
    "HOUR",
    "ЧАС",
    "MINUTE",
    "МИНУТА",
    "SECOND",
    "СЕКУНДА",
    "BEGINOFPERIOD",
    "НАЧАЛОПЕРИОДА",
    "ENDOFPERIOD",
    "КОНЕЦПЕРИОДА",
    "DATEADD",
    "ДОБАВИТЬКДАТЕ",
    "DATEDIFF",
    "РАЗНОСТЬДАТ",
    "SUBSTRING",
    "ПОДСТРОКА",
    "STRLENGTH",
    "ДЛИНАСТРОКИ",
    "LOWER",
    "НРЕГ",
    "UPPER",
    "ВРЕГ",
    "TRIMALL",
    "СОКРЛП",
    "TRIML",
    "СОКРЛ",
    "TRIMR",
    "СОКРП",
    "ROUND",
    "ОКР",
    "INT",
    "ЦЕЛ",
    "POW",
    "СТЕПЕНЬ",
    "SQRT",
    "КОРЕНЬ",
    "ABS",
    "МОДУЛЬ",
    "LOG",
    "ЛОГ",
    "EXP",
    "ЭКСП",
    "NUMBER",
    "ЧИСЛО",
    "STRING",
    "СТРОКА",
    "BOOLEAN",
    "БУЛЕВО",
    "DATE",
    "ДАТА",
];

const UNITS: &[&str] = &[
    "YEAR",
    "ГОД",
    "HALFYEAR",
    "ПОЛУГОДИЕ",
    "QUARTER",
    "КВАРТАЛ",
    "MONTH",
    "МЕСЯЦ",
    "TENDAYS",
    "ДЕКАДА",
    "WEEK",
    "НЕДЕЛЯ",
    "DAY",
    "ДЕНЬ",
    "HOUR",
    "ЧАС",
    "MINUTE",
    "МИНУТА",
    "SECOND",
    "СЕКУНДА",
];

#[derive(Clone, Copy, Debug)]
pub struct Role {
    pub word: &'static str,
    pub function: bool,
}

struct Frame {
    function: String,
    argument: usize,
}

/// One bounded traversal classifies only actual reserved words, calls and date-part arguments.
pub fn classify(tokens: &[Token<'_>]) -> Vec<Option<Role>> {
    let mut result = Vec::with_capacity(tokens.len());
    let mut significant = Vec::<usize>::new();
    let mut frames = Vec::<Frame>::new();
    for (index, token) in tokens.iter().enumerate() {
        let previous = significant.last().map(|at| &tokens[*at]);
        let role = if token.kind == Kind::Word && previous.is_none_or(|token| !token.is(".")) {
            let upper = token.text.to_uppercase();
            let next = tokens[index + 1..]
                .iter()
                .find(|token| token.kind != Kind::Comment);
            let function =
                next.is_some_and(|token| token.is("(")) && FUNCTIONS.contains(&upper.as_str());
            let alias = previous.is_some_and(|token| {
                ["AS", "КАК", "INTO", "ПОМЕСТИТЬ", "DROP", "УНИЧТОЖИТЬ"]
                    .contains(&token.text.to_uppercase().as_str())
            });
            let date_unit = frames
                .last()
                .is_some_and(|frame| date_argument(&frame.function) == Some(frame.argument))
                && UNITS.contains(&upper.as_str());
            let found = if function {
                FUNCTIONS.iter().copied().find(|word| *word == upper)
            } else if !alias && date_unit {
                UNITS.iter().copied().find(|word| *word == upper)
            } else if !alias {
                KEYWORDS.iter().copied().find(|word| *word == upper)
            } else {
                None
            };
            found.map(|word| Role { word, function })
        } else {
            None
        };
        result.push(role);
        match token.text {
            "(" => frames.push(Frame {
                function: previous.map_or_else(String::new, |token| token.text.to_uppercase()),
                argument: 0,
            }),
            ")" => {
                frames.pop();
            }
            "," => {
                if let Some(frame) = frames.last_mut() {
                    frame.argument += 1;
                }
            }
            _ => {}
        }
        if token.kind != Kind::Comment {
            significant.push(index);
        }
    }
    result
}

/// Type/date constants are recognized only in grammar positions that actually expect them.
fn date_argument(function: &str) -> Option<usize> {
    match function {
        "BEGINOFPERIOD"
        | "НАЧАЛОПЕРИОДА"
        | "ENDOFPERIOD"
        | "КОНЕЦПЕРИОДА"
        | "DATEADD"
        | "ДОБАВИТЬКДАТЕ" => Some(1),
        "DATEDIFF" | "РАЗНОСТЬДАТ" => Some(2),
        "PERIODS" | "ПЕРИОДАМИ" => Some(0),
        _ => None,
    }
}

/// Canonical SQL keywords are uppercase; source language and all unclassified tokens are preserved.
pub fn render(token: &Token<'_>, role: Option<Role>, case: KeywordCase) -> String {
    role.map_or_else(
        || token.text.into(),
        |role| match case {
            KeywordCase::Canonical | KeywordCase::Upper => role.word.into(),
            KeywordCase::Lower => role.word.to_lowercase(),
            KeywordCase::Preserve => token.text.into(),
        },
    )
}

/// Keyword normalization cannot excuse a changed identifier, parameter, literal or comment.
pub fn equivalent(before: &[Token<'_>], after: &[Token<'_>]) -> bool {
    if before.len() != after.len() {
        return false;
    }
    let left = classify(before);
    let right = classify(after);
    before.iter().zip(after).enumerate().all(|(index, (a, b))| {
        a.kind == b.kind
            && match (left[index], right[index]) {
                (Some(a), Some(b)) => a.word == b.word,
                (None, None) => a.text == b.text,
                _ => false,
            }
    })
}

/// Fixed keyword sets are reusable by structural printing without inspecting localized text.
pub fn is(role: Option<Role>, choices: &[&str]) -> bool {
    role.is_some_and(|role| choices.contains(&role.word))
}
