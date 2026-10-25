use super::*;

/// Native 8.3.27 rejects repeated TOTALS even though the bundled grammar describes repetition.
#[test]
fn repeated_totals_remain_exact_opaque_text_without_edit_permissions() {
    for input in [
        "SELECT A, B FROM T TOTALS SUM(A) BY A TOTALS MAXIMUM(B) BY B",
        "ВЫБРАТЬ A, B ИЗ T ИТОГИ СУММА(A) ПО A ИТОГИ МАКСИМУМ(B) ПО B",
        "SELECT A FROM T UNION ALL SELECT B FROM U TOTALS BY A TOTALS BY OVERALL",
    ] {
        let mut value = constructor(input);
        let index = value.model().statements.len() - 1;
        assert!(value.model().statements[index].unavailable.is_some());
        assert!(value.model().statements[index].total_sections().is_empty());
        assert!(value.finish().unwrap().is_empty());
        assert!(
            value
                .apply(
                    1,
                    &Action::AddTotals {
                        statement: index,
                        after: None
                    }
                )
                .is_err()
        );
        assert_eq!(value.text(), input);
        assert_eq!(value.revision(), 1);
    }
}

/// One section supports independent aggregates and grouping levels without generating a second header.
#[test]
fn one_total_section_owns_multiple_fields_and_grouping_levels() {
    let mut value = constructor("SELECT A, B FROM T TOTALS SUM(A) BY OVERALL");
    for expression in [
        Expression::TotalField {
            expression: "B".into(),
            alias: String::new(),
            aggregate: Some(Aggregate::Maximum),
        },
        Expression::TotalGroup {
            expression: "B".into(),
            alias: String::new(),
            hierarchy: Hierarchy::None,
            period: None,
        },
    ] {
        apply(
            &mut value,
            &Action::ComposeTotal {
                statement: 0,
                section: 0,
                selection: None,
                expression,
            },
        );
    }
    let owner = &value.model().statements[0];
    let section = &owner.total_sections()[0];
    assert_eq!(owner.clauses[section.fields].items.len(), 2);
    assert_eq!(owner.clauses[section.groups].items.len(), 2);
    let before = value.text().to_owned();
    for after in [None, Some(0)] {
        assert_eq!(
            value
                .apply(
                    value.revision(),
                    &Action::AddTotals {
                        statement: 0,
                        after
                    }
                )
                .unwrap_err(),
            Error::Unsupported
        );
        assert_eq!(value.text(), before);
    }
    apply(
        &mut value,
        &Action::ComposeTotal {
            statement: 0,
            section: 0,
            selection: None,
            expression: Expression::Overall,
        },
    );
    assert_eq!(value.text(), before);
    assert_eq!(value.total_focus(), Some(0));
}

/// Adding and removing the single section preserves comments and uses revision-bound section indices.
#[test]
fn total_section_lifecycle_keeps_comments_and_unrelated_query_text() {
    let input = "// keep 😀\r\nSELECT A, B FROM T";
    let mut value = constructor(input);
    apply(
        &mut value,
        &Action::AddTotals {
            statement: 0,
            after: None,
        },
    );
    assert_eq!(value.model().statements[0].total_sections().len(), 1);
    assert_eq!(value.total_focus(), Some(0));
    apply(
        &mut value,
        &Action::AppendTotal {
            statement: 0,
            section: 0,
            clause: ClauseKind::Totals,
            text: "SUM(A) // total\r\n".into(),
        },
    );
    let before = value.text().to_owned();
    assert_eq!(
        value
            .apply(
                value.revision(),
                &Action::RemoveTotals {
                    statement: 0,
                    section: 0,
                    comments: None
                }
            )
            .unwrap_err(),
        Error::CommentsRequireDecision
    );
    assert_eq!(value.text(), before);
    apply(
        &mut value,
        &Action::RemoveTotals {
            statement: 0,
            section: 0,
            comments: Some(CommentDecision::Keep),
        },
    );
    assert!(value.model().statements[0].total_sections().is_empty());
    assert!(value.text().starts_with(input));
    assert!(value.text().contains("// total"));
    assert_eq!(value.total_focus(), None);
}

/// Totals stay on the UNION result; nested totals and forged section indices never gain write authority.
#[test]
fn single_total_section_respects_union_scope_and_rejects_invalid_actions() {
    let mut value =
        constructor("SELECT A FROM T UNION ALL SELECT B FROM U TOTALS SUM(A) BY OVERALL");
    assert!(value.model().statements[0].total_sections().is_empty());
    assert_eq!(value.model().statements[1].total_sections().len(), 1);
    let before = value.text().to_owned();
    for action in [
        Action::AddTotals {
            statement: 0,
            after: None,
        },
        Action::MoveTotals {
            statement: 1,
            section: 0,
            to: 1,
        },
        Action::ComposeTotal {
            statement: 1,
            section: 1,
            selection: None,
            expression: Expression::Overall,
        },
        Action::AppendTotal {
            statement: 1,
            section: 0,
            clause: ClauseKind::Totals,
            text: "SUM(B) BY B TOTALS SUM(A) BY OVERALL".into(),
        },
    ] {
        assert!(value.apply(value.revision(), &action).is_err());
        assert_eq!(value.text(), before);
        assert_eq!(value.revision(), 1);
    }
    apply(
        &mut value,
        &Action::MoveTotals {
            statement: 1,
            section: 0,
            to: 0,
        },
    );
    assert_eq!(value.text(), before);
    apply(
        &mut value,
        &Action::RemoveUnion {
            statement: 1,
            comments: None,
        },
    );
    assert_eq!(value.model().statements[0].total_sections().len(), 1);
    let mut nested = constructor("SELECT Q.A FROM (SELECT A FROM T TOTALS BY A) AS Q");
    assert!(nested.finish().unwrap().is_empty());
    assert!(
        nested
            .apply(
                1,
                &Action::AddTotals {
                    statement: 1,
                    after: None
                }
            )
            .is_err()
    );
}
