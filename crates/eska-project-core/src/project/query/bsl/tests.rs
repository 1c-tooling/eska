use super::*;
use crate::project::query::Affinity;

/// Tests select a literal exactly as the editor cursor would, without supplying arbitrary ranges.
fn query(input: &str) -> Query {
    read_bsl(input, input.find('"').expect("quote") + 1).expect("query")
}

#[test]
fn decodes_quotes_bars_empty_lines_and_source_only_comments() {
    let input = "\u{feff}ЛюбоеИмя = \"ВЫБРАТЬ \"\"😀\"\" КАК Значение\r\n\t// BSL comment\r\n\r\n\t|\r\n\t|| // SDBL comment\r\n\t|{ГДЕ Поле = &Параметр}\"; // outside";
    let query = query(input);
    assert_eq!(
        query.text,
        "ВЫБРАТЬ \"😀\" КАК Значение\n\n| // SDBL comment\n{ГДЕ Поле = &Параметр}"
    );
    assert_eq!(query.comments.len(), 1);
    assert_eq!(query.comments[0].text, "\t// BSL comment");
    assert_eq!(
        &input[query.comments[0].source.clone()],
        query.comments[0].text
    );
    assert_eq!(
        query.comments[0].query_offset,
        query.text.find('\n').expect("line") + 1
    );
    let emoji = query.text.find('😀').expect("emoji");
    assert_eq!(
        query
            .map
            .query_to_source(&query.text, emoji, Affinity::After),
        input.find('😀')
    );
    assert_eq!(
        query
            .map
            .source_to_query(input, query.comments[0].source.start + 1),
        None
    );
    let cr = input.find('\r').expect("CRLF");
    assert_eq!(query.map.source_to_query(input, cr + 1), None);
}

#[test]
fn accepts_bilingual_constructors_returns_and_parenthesized_direct_values() {
    for input in [
        "З = Новый Запрос(\"ВЫБРАТЬ 1\");",
        "q = New QUERY(\"SELECT 1\").Execute();",
        "Возврат ((\"ВЫБРАТЬ 1\"));",
        "Return \"SELECT 1\";",
        "Other = \"\";",
    ] {
        assert!(
            read_bsl(input, input.find('"').expect("quote")).is_ok(),
            "{input}"
        );
    }
}

#[test]
fn rejects_composed_transformed_adjacent_and_conditional_expressions() {
    for input in [
        "x = \"SELECT\" + suffix;",
        "x = prefix + \"SELECT\";",
        "x = СтрШаблон(\"SELECT %1\", Value);",
        "x = StrReplace(\"SELECT\", \"A\", \"B\");",
        "x = (\"SELECT\").Replace();",
        "x = \"SELECT\" \" 1\";",
        "If x = \"SELECT\" Then",
        "x = ?(condition, \"SELECT\", other);",
    ] {
        assert_eq!(
            read_bsl(input, input.find('"').expect("quote")),
            Err(QueryError::UnsupportedExpression),
            "{input}"
        );
    }
}

#[test]
fn ignores_comment_quotes_and_rejects_incomplete_or_invalid_selection() {
    let input = "// \"not a literal\"\nx = \"SELECT\";";
    assert_eq!(read_bsl(input, 4), Err(QueryError::NotFound));
    assert_eq!(
        read_bsl("x = \"one\n no bar\";", 5),
        Err(QueryError::InvalidLiteral)
    );
    assert_eq!(
        read_bsl("x = \"unfinished", 5),
        Err(QueryError::InvalidLiteral)
    );
    assert_eq!(read_bsl("x = \"А\";", 6), Err(QueryError::InvalidPosition));
    assert_eq!(
        read_bsl("x = \"one\n// comment\n|two\";", 14),
        Err(QueryError::NotFound)
    );
}
