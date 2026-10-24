use super::*;

/// Keep the full XML owner, including bytes outside the chosen scalar, in every patch assertion.
fn xml(content: &str) -> String {
    format!(
        "\u{feff}<?xml version='1.0'?>\r\n<DataCompositionSchema xmlns='http://v8.1c.ru/8.1/data-composition-system/schema' xmlns:xsi='http://www.w3.org/2001/XMLSchema-instance'>\r\n<dataSet xsi:type='DataSetQuery'><name>A</name><query>{content}</query><other>unchanged</other></dataSet>\r\n</DataCompositionSchema>\r\n"
    )
}

/// Select through the same readers used by a query session, rather than trusting test ranges.
fn query(input: &str) -> Query {
    if input.starts_with('\u{feff}') && input.contains("DataCompositionSchema") {
        crate::project::query::read_xml(input)
            .expect("XML")
            .remove(0)
    } else {
        crate::project::query::read_bsl(input, input.find('"').expect("quote")).expect("BSL")
    }
}

/// An extraction is also the source of the exact-byte snapshot sent by an editor.
fn edit(input: &str, value: &str, decisions: &[CommentPlacement]) -> Result<EditPlan, QueryError> {
    plan(
        input,
        &metadata_edit::snapshot(input),
        &query(input).identity,
        value,
        decisions,
        None,
    )
}

#[test]
fn unchanged_apply_preserves_every_source_byte_and_creates_no_replacements() {
    for input in [
        xml("<!--before--><![CDATA[ВЫБРАТЬ \"😀\"\r\n{ПОЛЕ}]]>&amp;<!--after-->"),
        xml("A<!--between-->B"),
        xml(""),
        "\u{feff}\tq = \"ВЫБРАТЬ \"\"😀\"\"\r\n\t// BSL\r\n\r\n\t|\r\n\t|| // SDBL\"; // outside\r\n".to_owned(),
    ] {
        let result = edit(&input, &query(&input).text, &[]).expect("no-op");
        assert_eq!(result.output().as_bytes(), input.as_bytes());
        assert!(result.is_empty());
    }
}

#[test]
fn xml_changes_only_the_query_scalar_and_escapes_values_with_existing_patch() {
    let input = xml("<!--before--><![CDATA[old]]><!--after-->");
    let value = "ВЫБРАТЬ \"😀\"\n{ГДЕ A < 2 & B}\r\n";
    let result = edit(&input, value, &[]).expect("edit");
    assert_eq!(
        result.output(),
        input.replace(
            "<![CDATA[old]]>",
            "ВЫБРАТЬ \"😀\"\n{ГДЕ A &lt; 2 &amp; B}&#13;\n"
        )
    );
    assert_eq!(result.replacements().len(), 1);
    assert_eq!(query(result.output()).text, value);
}

/// A typed DCS change survives the real Designer XML reader/encoder without touching unrelated owner bytes.
#[test]
fn typed_dcs_constructor_round_trips_designer_xml_with_cdata_and_entities() {
    use crate::project::query::constructor::{Action, Comparison, Constructor, DcsKind, DcsValue};
    let input = xml(&format!(
        "<!--source--><![CDATA[{}]]>",
        include_str!("../constructor/tests/fixtures/buhmfo-price-query.sdbl")
    ));
    let extracted = query(&input);
    let mut constructor = Constructor::new(extracted.text.into()).unwrap();
    assert!(constructor.finish().unwrap().is_empty());
    constructor
        .apply(
            constructor.revision(),
            &Action::SetDcs {
                statement: 0,
                node: Some(
                    constructor.dcs_model().unwrap().statements[0]
                        .nodes
                        .iter()
                        .find(|node| node.kind == DcsKind::Where)
                        .unwrap()
                        .index,
                ),
                target: DcsKind::Where,
                values: vec![DcsValue::Condition {
                    left: "СправочникНоменклатура.Код".into(),
                    comparison: Comparison::Less,
                    right: "&Граница".into(),
                }],
                enabled: true,
                comments: None,
            },
        )
        .unwrap();
    let output = edit(&input, constructor.text(), &[]).unwrap();
    assert_eq!(query(output.output()).text, constructor.text());
    assert_eq!(output.replacements().len(), 1);
    assert!(
        output
            .output()
            .starts_with(input.split("<![CDATA[").next().unwrap())
    );
    assert!(
        output.output().ends_with(
            "</query><other>unchanged</other></dataSet>\r\n</DataCompositionSchema>\r\n"
        )
    );
    assert!(
        output
            .output()
            .contains("СправочникНоменклатура.Код &lt; &amp;Граница")
    );
}

/// Editor EOL normalization must not change the byte plan or the canonical decoded XML query.
#[test]
fn editor_line_endings_match_new_xml_lines_without_rewriting_owner_or_no_op() {
    let input = xml("<![CDATA[old]]>");
    let identity = query(&input).identity;
    for ending in [LineEnding::Lf, LineEnding::CrLf] {
        let value = "SELECT 1\nFROM Table";
        let result = plan(
            &input,
            &metadata_edit::snapshot(&input),
            &identity,
            value,
            &[],
            Some(ending),
        )
        .expect("editor XML plan");
        assert_eq!(
            result.output(),
            input.replace("<![CDATA[old]]>", &ending.render(value))
        );
        assert_eq!(query(result.output()).text, value);
        let no_op = plan(
            &input,
            &metadata_edit::snapshot(&input),
            &identity,
            "old",
            &[],
            Some(ending),
        )
        .expect("no-op");
        assert_eq!(no_op.output(), input);
        assert!(no_op.is_empty());
    }
}

/// Repositioned XML comments retain exact source bytes when rendering physical CRLF query lines.
#[test]
fn editor_line_endings_preserve_split_source_comments_and_logical_cr_entities() {
    let input = xml("A<!--\r\nkeep\r\n-->B");
    let value = "N\nW\r";
    let result = plan(
        &input,
        &metadata_edit::snapshot(&input),
        &query(&input).identity,
        value,
        &[CommentPlacement {
            comment: 0,
            query_offset: Some(1),
        }],
        Some(LineEnding::CrLf),
    )
    .expect("comment placement");
    assert_eq!(
        result.output(),
        input.replace("A<!--\r\nkeep\r\n-->B", "N<!--\r\nkeep\r\n-->\r\nW&#13;")
    );
    assert_eq!(query(result.output()).text, value);
}

/// A one-line BSL literal has no local newline to infer; the editor supplies the owner style.
#[test]
fn editor_line_endings_control_new_bsl_continuations() {
    let input = "\u{feff}// owner\r\nx = \"SELECT 1\"; // untouched\r\n";
    let value = "SELECT 1\nFROM T";
    let result = plan(
        input,
        &metadata_edit::snapshot(input),
        &query(input).identity,
        value,
        &[],
        Some(LineEnding::CrLf),
    )
    .expect("BSL CRLF");
    assert_eq!(
        result.output(),
        input.replace("SELECT 1", "SELECT 1\r\n\t|FROM T")
    );
    assert_eq!(query(result.output()).text, value);
}

#[test]
fn bsl_changes_only_differing_bytes_and_preserves_comment_lines_and_undo() {
    let input =
        "\u{feff}\tWhatever = \"ВЫБРАТЬ 1\r\n\t// keep\r\n\r\n\t|ИЗ Таблица\"; // outside\r\n";
    let value = "ВЫБРАТЬ 2\nИЗ Таблица";
    let result = edit(input, value, &[]).expect("edit");
    assert_eq!(result.output(), input.replacen("ВЫБРАТЬ 1", "ВЫБРАТЬ 2", 1));
    assert_eq!(result.replacements()[0].text, "2");
    let history = result.history();
    assert_eq!(
        history
            .replay(result.output(), true)
            .expect("undo")
            .output(),
        input
    );
}

#[test]
fn bsl_handles_inserted_removed_and_empty_final_lines_without_leaking_service_bars() {
    for (input, value) in [
        ("x = \"old\";", "\"😀\"\n\n| // query comment\n"),
        ("x = \"old\r\n\t|\";", ""),
        ("x = \"a\n\t|b\";", "a\nnew\nb"),
        ("x = \"a\n\t|\";", "a\nend"),
        ("x = \"\";", "a\n\n"),
    ] {
        let original = query(input);
        let result = edit(input, value, &[]).expect("edit");
        let updated = query(result.output());
        assert_eq!(updated.text, value, "{}", result.output());
        assert_eq!(
            &result.output()[..original.source.start],
            &input[..original.source.start]
        );
        assert_eq!(
            &result.output()[updated.source.end..],
            &input[original.source.end..]
        );
    }
}

#[test]
fn source_comment_relocation_requires_explicit_valid_decisions() {
    let input = "x = \"a\n\t// owner comment\n\t|b\n\t|c\";";
    let value = "new\ntext";
    assert_eq!(
        edit(input, value, &[]).err(),
        Some(QueryError::CommentsRequireDecision)
    );
    let offset = value.find('\n').expect("line") + 1;
    let result = edit(
        input,
        value,
        &[CommentPlacement {
            comment: 0,
            query_offset: Some(offset),
        }],
    )
    .expect("explicit move");
    assert_eq!(query(result.output()).text, value);
    assert_eq!(
        query(result.output()).comments[0].text,
        "\t// owner comment"
    );
    let removed = edit(
        input,
        value,
        &[CommentPlacement {
            comment: 0,
            query_offset: None,
        }],
    )
    .expect("explicit delete");
    assert_eq!(query(removed.output()).comments.len(), 0);
    assert_eq!(
        edit(
            input,
            value,
            &[CommentPlacement {
                comment: 0,
                query_offset: Some(1)
            }]
        )
        .err(),
        Some(QueryError::CommentsRequireDecision)
    );
}

/// Several explicit relocations retain stable order at shared destinations and across later lines.
#[test]
fn relocates_multiple_source_comments_without_losing_or_duplicating_them() {
    let input = "x = \"a\n\t// first\n\t|b\n\t// second\n\t|c\";";
    let value = "new\ntext\nlast";
    let result = edit(
        input,
        value,
        &[
            CommentPlacement {
                comment: 0,
                query_offset: Some(9),
            },
            CommentPlacement {
                comment: 1,
                query_offset: Some(4),
            },
        ],
    )
    .expect("relocate");
    let decoded = query(result.output());
    assert_eq!(decoded.text, value);
    assert_eq!(
        decoded
            .comments
            .iter()
            .map(|comment| comment.text.as_str())
            .collect::<Vec<_>>(),
        ["\t// second", "\t// first"]
    );
    let result = edit(
        input,
        value,
        &[
            CommentPlacement {
                comment: 1,
                query_offset: Some(4),
            },
            CommentPlacement {
                comment: 0,
                query_offset: Some(4),
            },
        ],
    )
    .expect("shared destination");
    assert_eq!(
        query(result.output())
            .comments
            .iter()
            .map(|comment| comment.text.as_str())
            .collect::<Vec<_>>(),
        ["\t// first", "\t// second"]
    );
}

#[test]
fn split_xml_comments_are_preserved_or_require_a_decision_when_crossed() {
    let input = xml("A<!--keep-->B<![CDATA[&]]>&amp;");
    let result = edit(&input, "ZB&&", &[]).expect("unambiguous anchor");
    assert!(result.output().contains("Z<!--keep-->B&amp;&amp;"));
    assert_eq!(
        edit(&input, "new", &[]).err(),
        Some(QueryError::CommentsRequireDecision)
    );
    let result = edit(
        &input,
        "new",
        &[CommentPlacement {
            comment: 0,
            query_offset: Some(3),
        }],
    )
    .expect("move XML comment");
    assert!(result.output().contains("new<!--keep-->"));
}

#[test]
fn stale_ranges_invalid_values_and_forged_comment_targets_never_return_a_plan() {
    let input = "x = \"SELECT 1\";";
    let selected = query(input);
    assert_eq!(
        plan(input, "stale", &selected.identity, "SELECT 2", &[], None).err(),
        Some(QueryError::Conflict)
    );
    assert_eq!(
        plan(
            input,
            &metadata_edit::snapshot(input),
            &QueryIdentity::BslLiteral(6..10),
            "SELECT 2",
            &[],
            None,
        )
        .err(),
        Some(QueryError::Conflict)
    );
    assert_eq!(
        edit(input, "bad\rvalue", &[]).err(),
        Some(QueryError::InvalidValue)
    );
    assert_eq!(
        edit(
            input,
            "SELECT 2",
            &[CommentPlacement {
                comment: 1,
                query_offset: None
            }]
        )
        .err(),
        Some(QueryError::InvalidValue)
    );
    assert_eq!(
        edit(&xml("old"), "bad\0value", &[]).err(),
        Some(QueryError::InvalidValue)
    );
}
