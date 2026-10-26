//! Realistic queries exercise the public protocol and require a byte-stable second pass.

use eska_project_core::formatting::{
    KeywordCase, Language, Options, Wrapping,
    protocol::{Range, Request},
};

/// Tests apply the same minimal edit that the CLI and IDE consume.
fn formatted(input: &str, options: Options) -> String {
    let mut request = Request {
        schema_version: 1,
        language: Language::Sdbl,
        text: input.into(),
        options,
        range: None,
    };
    let response = crate::format(&request).unwrap_or_else(|error| panic!("{error}: {input}"));
    let mut output = input.to_owned();
    for edit in response.edits.iter().rev() {
        output.replace_range(edit.range.start..edit.range.end, &edit.text);
    }
    request.text.clone_from(&output);
    let repeat = crate::format(&request).unwrap_or_else(|error| panic!("{error}: {output}"));
    assert!(
        repeat.edits.is_empty(),
        "unstable: {output:?} => {:?}",
        repeat.edits
    );
    output
}

/// Defaults use native SELECT layout without renaming aliases, paths or parameters.
#[test]
fn clauses_and_language_roles() {
    let input = "выбрать т.Имя как имя,т.где как Group из Справочник.Товары как т где т.Код=&where и т.Имя<>\"case\" упорядочить по т.Имя убыв";
    assert_eq!(
        formatted(input, Options::default()),
        "ВЫБРАТЬ\n\tт.Имя КАК имя,\n\tт.где КАК Group\nИЗ\n\tСправочник.Товары КАК т\nГДЕ\n\tт.Код = &where\n\tИ т.Имя <> \"case\"\nУПОРЯДОЧИТЬ ПО\n\tт.Имя УБЫВ\n"
    );
    let output = formatted(
        "select sum(t.Amount) as Total from Table as t group by t.Id having sum(t.Amount)>0 order by Total desc",
        Options::default(),
    );
    assert!(output.contains("SUM(t.Amount) AS Total"));
    assert!(output.contains("GROUP BY\n\tt.Id"));
    assert!(output.contains("HAVING\n\tSUM(t.Amount) > 0"));
}

/// One pass owns package separators, all UNION branches, nested SELECTs and result totals.
#[test]
fn packages_nested_unions_and_totals() {
    let input = "SELECT 1 AS A INTO Temp;SELECT S.A FROM (SELECT A FROM Temp UNION ALL SELECT 2 AS A) AS S ORDER BY A TOTALS SUM(A) BY OVERALL;DROP Temp";
    let output = formatted(input, Options::default());
    assert!(output.contains("INTO Temp;\n\nSELECT"));
    assert!(output.contains("\n\t\tSELECT"));
    assert!(output.contains("UNION ALL"));
    assert!(output.contains("TOTALS\n\tSUM(A)\nBY\n\tOVERALL"));
    assert!(output.ends_with("DROP Temp\n"));
}

/// CASE, SQL calls and date-part constants preserve values and spelling of ordinary identifiers.
#[test]
fn scalar_calls_case_and_date_parts() {
    let input = "select case when T.A=1 then dateadd(datetime(2026,1,1),month,1) else T.End end as Result,cast(1 as number(10,2)) as N from T";
    let output = formatted(input, Options::default());
    assert!(output.contains(
        "CASE WHEN T.A = 1 THEN DATEADD(DATETIME(2026, 1, 1), MONTH, 1) ELSE T.End END AS Result"
    ));
    assert!(output.contains("CAST(1 AS NUMBER(10, 2)) AS N"));
}

/// A real committed buhmfo fixture exercises supported SCD slots and virtual-table filters.
#[test]
fn real_dcs_virtual_table_fixture() {
    let input = include_str!(
        "../../../eska-project-core/src/project/query/constructor/tests/fixtures/buhmfo-price-query.sdbl"
    );
    let output = formatted(input, Options::default());
    assert!(output.contains("{ВЫБРАТЬ"));
    assert!(output.contains("СрезПоследних"));
}

/// Explicit layout settings affect real output and remain stable after normalized spacing.
#[test]
fn aliases_and_compact_options() {
    let output = formatted(
        "select 1 as A, 12345 as B",
        Options {
            align_aliases: true,
            ..Options::default()
        },
    );
    assert!(output.contains("1     AS A"));
    let output = formatted(
        "SELECT A,B FROM T WHERE A=1 AND B=2",
        Options {
            clause_newlines: false,
            select_items: Wrapping::Fit,
            condition_items: Wrapping::Fit,
            keyword_case: KeywordCase::Lower,
            ..Options::default()
        },
    );
    assert_eq!(output, "select A, B from T where A = 1 and B = 2\n");
}

/// Comments cannot swallow commas, UNION keywords or closing nested parentheses.
#[test]
fn comments_encoding_and_ranges() {
    let input = "\u{feff}select 1 as A, // A\r\n2 as B /* B */;select 3 as C";
    let output = formatted(input, Options::default());
    assert!(output.starts_with("\u{feff}SELECT\r\n"));
    assert!(output.contains(", // A\r\n\t2 AS B /* B */"));
    assert!(!output.replace("\r\n", "").contains('\n'));
    let request = Request {
        schema_version: 1,
        language: Language::Sdbl,
        text: input.into(),
        options: Options::default(),
        range: Some(Range { start: 3, end: 20 }),
    };
    let response = crate::format(&request).unwrap();
    assert_eq!(response.formatted_range.end, input.find(';').unwrap() + 1);
    assert!(
        response
            .edits
            .iter()
            .all(|edit| edit.range.end <= response.formatted_range.end)
    );
}

/// Unknown statements and malformed local controls fail atomically rather than returning partial edits.
#[test]
fn unsupported_and_frozen_queries() {
    let frozen = "// eska-fmt off\n select  1\n// eska-fmt on";
    assert_eq!(formatted(frozen, Options::default()), frozen);
    for input in [
        "DELETE FROM T",
        "SELECT (1",
        "SELECT CASE WHEN 1=1 THEN 2",
        "// eska-fmt on\nSELECT 1",
        "SELECT A {UNKNOWN B} FROM T",
    ] {
        let request = Request {
            schema_version: 1,
            language: Language::Sdbl,
            text: input.into(),
            options: Options::default(),
            range: None,
        };
        assert!(crate::format(&request).is_err(), "accepted {input}");
    }
}

/// Source comments stay attached to tokens while neighboring methods and XML values remain exact.
#[test]
fn embedded_query_owners_preserve_other_bytes_and_comments() {
    use eska_project_core::project::{metadata_edit, query};
    let bsl =
        "\u{feff}q = \"select 1 as A,\r\n // источник\r\n |2 as B\"; // outside\r\nX=  1;\r\n";
    let extracted = query::read_bsl(bsl, bsl.find('"').unwrap()).unwrap();
    let plan = crate::format_query(
        bsl,
        &metadata_edit::snapshot(bsl),
        &extracted.identity,
        Options::default(),
        None,
    )
    .unwrap();
    assert!(plan.output().ends_with("; // outside\r\nX=  1;\r\n"));
    assert!(plan.output().contains("// источник"));
    let extracted = query::read_bsl(plan.output(), plan.output().find('"').unwrap()).unwrap();
    assert_eq!(extracted.comments.len(), 1);
    assert!(
        crate::format_query(
            plan.output(),
            &metadata_edit::snapshot(plan.output()),
            &extracted.identity,
            Options::default(),
            None
        )
        .unwrap()
        .is_empty()
    );
    let xml = "\u{feff}<DataCompositionSchema xmlns='http://v8.1c.ru/8.1/data-composition-system/schema' xmlns:xsi='http://www.w3.org/2001/XMLSchema-instance'><dataSet xsi:type='DataSetQuery'><name>A</name><query><![CDATA[select 1 as A,]]><!--anchor-->2 as B where 1 &lt; 2</query><other>  untouched  </other></dataSet><dataSet xsi:type='DataSetQuery'><name>B</name><query>SELECT 777</query></dataSet></DataCompositionSchema>";
    let extracted = query::read_xml(xml).unwrap();
    let plan = crate::format_query(
        xml,
        &metadata_edit::snapshot(xml),
        &extracted[0].identity,
        Options::default(),
        None,
    )
    .unwrap();
    assert!(plan.output().contains("<!--anchor-->"));
    assert!(plan.output().contains("&lt;"));
    assert!(plan.output().ends_with("<other>  untouched  </other></dataSet><dataSet xsi:type='DataSetQuery'><name>B</name><query>SELECT 777</query></dataSet></DataCompositionSchema>"));
    assert_eq!(
        query::read_xml(plan.output()).unwrap()[1].text,
        "SELECT 777"
    );
    assert!(
        crate::format_query(
            xml,
            "stale",
            &extracted[0].identity,
            Options::default(),
            None
        )
        .is_err()
    );
}

/// A comment before the first query token needs one decoded newline and a no-op second owner plan.
#[test]
fn initial_bsl_source_comment_remains_idempotent() {
    use eska_project_core::project::{metadata_edit, query};
    let source = "q = \"\n // источник\n |select 1 as A\";";
    let extracted = query::read_bsl(source, source.find('"').unwrap()).unwrap();
    let plan = crate::format_query(
        source,
        &metadata_edit::snapshot(source),
        &extracted.identity,
        Options::default(),
        None,
    )
    .unwrap();
    let query = query::read_bsl(plan.output(), plan.output().find('"').unwrap()).unwrap();
    assert!(query.text.starts_with('\n'));
    assert!(
        crate::format_query(
            plan.output(),
            &metadata_edit::snapshot(plan.output()),
            &query.identity,
            Options::default(),
            None
        )
        .unwrap()
        .is_empty()
    );
}

/// Independent list, binary, call and whitespace options produce stable output under non-default styles.
#[test]
fn configurable_lists_calls_and_whitespace() {
    let output = formatted(
        "select First,Second,Third from T group by First,Second,Third order by First,Second,Third",
        Options {
            column_limit: 24,
            select_items: Wrapping::Fit,
            group_items: Wrapping::OnePerLine,
            order_items: Wrapping::OnePerLine,
            ..Options::default()
        },
    );
    assert!(output.contains("\n\tThird"));
    assert!(output.contains("ORDER BY\n\tFirst,\n\tSecond,\n\tThird"));
    let output = formatted(
        "SELECT SUM(1+2) AS A",
        Options {
            wrap_binary: Wrapping::OnePerLine,
            break_before_binary_operators: false,
            wrap_arguments: Wrapping::OnePerLine,
            ..Options::default()
        },
    );
    assert!(output.contains("+\n"));
    assert_eq!(
        formatted(
            "select 1  \n  \n",
            Options {
                trim_trailing_whitespace: false,
                insert_final_newline: false,
                ..Options::default()
            }
        ),
        "SELECT\n\t1  \n  \n"
    );
}
