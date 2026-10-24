use super::*;

/// Each pair retains its own fields/groups; an explicit section cannot accidentally edit a neighbour.
#[test]
fn sequential_totals_own_independent_fields_groups_and_exact_noop() {
    let input = "SELECT A, B FROM T TOTALS SUM(A) BY A TOTALS MAXIMUM(B) BY B";
    let mut value = constructor(input);
    assert!(value.model().statements[0].unavailable.is_none());
    assert_eq!(value.model().statements[0].total_sections().len(), 2);
    assert!(value.finish().unwrap().is_empty());
    let first = value.model().statements[0].total_sections()[0]
        .range
        .clone();
    let original = value.text()[first].to_owned();
    apply(
        &mut value,
        &Action::ComposeTotal {
            statement: 0,
            section: 1,
            selection: None,
            expression: Expression::TotalField {
                expression: "B".into(),
                alias: String::new(),
                aggregate: Some(Aggregate::Sum),
            },
        },
    );
    let owner = &value.model().statements[0];
    assert_eq!(
        value.text()[owner.total_sections()[0].range.clone()],
        original
    );
    assert_eq!(
        owner.clauses[owner.total_sections()[1].fields].items.len(),
        2
    );
    apply(
        &mut value,
        &Action::ComposeTotal {
            statement: 0,
            section: 1,
            selection: None,
            expression: Expression::Overall,
        },
    );
    let revision = value.revision();
    apply(
        &mut value,
        &Action::ComposeTotal {
            statement: 0,
            section: 1,
            selection: None,
            expression: Expression::Overall,
        },
    );
    assert_eq!(value.revision(), revision);
    assert_eq!(value.total_focus(), Some(1));
    assert_eq!(value.text().matches("OVERALL").count(), 1);
}

/// Creation, movement and removal operate on complete pairs, including automatic empty total expressions.
#[test]
fn total_sections_add_move_remove_and_focus_preserve_unrelated_query_text() {
    let input = "// keep 😀\r\nSELECT A, B FROM T\r\nTOTALS // first\r\nSUM(A) BY A\r\nTOTALS // second\r\nMAXIMUM(B) BY B";
    let mut value = constructor(input);
    apply(
        &mut value,
        &Action::AddTotals {
            statement: 0,
            after: Some(0),
        },
    );
    assert_eq!(value.total_focus(), Some(1));
    let owner = &value.model().statements[0];
    let section = &owner.total_sections()[1];
    assert!(owner.clauses[section.fields].items.is_empty());
    assert_eq!(
        owner.clauses[section.groups].items[0].expression(value.text()),
        "OVERALL"
    );
    apply(
        &mut value,
        &Action::MoveTotals {
            statement: 0,
            section: 2,
            to: 0,
        },
    );
    assert!(
        value
            .text()
            .starts_with("// keep 😀\r\nSELECT A, B FROM T\r\n")
    );
    assert!(value.text().find("// second").unwrap() < value.text().find("// first").unwrap());
    assert_eq!(value.total_focus(), Some(0));
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
    assert_eq!(value.model().statements[0].total_sections().len(), 2);
    assert!(value.text().contains("// second\r\n"));
    assert!(!value.text().contains("MAXIMUM(B)"));
    assert!(value.text().contains("SUM(A)"));
}

/// Removing the last group of the second pair removes that pair alone, retaining automatic fields in another pair.
#[test]
fn last_group_removal_does_not_delete_other_total_sections() {
    let mut value = constructor("SELECT A, B FROM T TOTALS BY A TOTALS SUM(B) BY B");
    let groups = value.model().statements[0].total_sections()[1].groups;
    apply(
        &mut value,
        &Action::Remove {
            selection: selection(groups, 0),
            comments: None,
        },
    );
    assert_eq!(value.model().statements[0].total_sections().len(), 1);
    assert!(value.text().contains("TOTALS BY A"));
    assert!(!value.text().contains("SUM(B)"));
    assert_eq!(value.total_focus(), Some(0));
}

/// Multiple sections stay in the UNION tail while nested totals remain readable but inactive.
#[test]
fn multiple_total_sections_follow_union_result_and_reject_nested_edits() {
    let mut value = constructor(
        "SELECT A FROM T UNION ALL SELECT B FROM U TOTALS SUM(A) BY A TOTALS SUM(A) BY OVERALL",
    );
    assert!(value.model().statements[0].total_sections().is_empty());
    assert_eq!(value.model().statements[1].total_sections().len(), 2);
    assert_eq!(
        value
            .apply(
                1,
                &Action::AddTotals {
                    statement: 0,
                    after: None
                }
            )
            .unwrap_err(),
        Error::Unsupported
    );
    apply(
        &mut value,
        &Action::RemoveUnion {
            statement: 1,
            comments: None,
        },
    );
    assert_eq!(value.model().statements[0].total_sections().len(), 2);
    let mut nested =
        constructor("SELECT Q.A FROM (SELECT A FROM T TOTALS BY A TOTALS BY OVERALL) AS Q");
    assert!(nested.finish().unwrap().is_empty());
    assert!(nested.model().statements[1].unavailable.is_some());
    assert_eq!(
        nested
            .apply(
                1,
                &Action::AddTotals {
                    statement: 1,
                    after: None
                }
            )
            .unwrap_err(),
        Error::Unsupported
    );
}

/// Section indices, expression types, headers and model limits are checked before a private draft changes.
#[test]
fn total_section_failures_are_atomic_and_bounded() {
    let original = "SELECT A, B FROM T TOTALS SUM(A) BY A TOTALS SUM(B) BY B";
    let mut value = constructor(original);
    for action in [
        Action::ComposeTotal {
            statement: 0,
            section: 1,
            selection: Some(selection(2, 0)),
            expression: Expression::TotalField {
                expression: "A".into(),
                alias: String::new(),
                aggregate: Some(Aggregate::Count),
            },
        },
        Action::AppendTotal {
            statement: 0,
            section: 1,
            clause: ClauseKind::Sources,
            text: "T".into(),
        },
        Action::AppendTotal {
            statement: 0,
            section: 1,
            clause: ClauseKind::Totals,
            text: "SUM(B) BY B TOTALS SUM(A)".into(),
        },
    ] {
        assert!(value.apply(1, &action).is_err());
        assert_eq!(value.text(), original);
        assert_eq!(value.revision(), 1);
    }
    let text = format!("SELECT A FROM T {}", "TOTALS BY A ".repeat(32));
    let mut bounded = constructor(&text);
    assert_eq!(bounded.model().statements[0].total_sections().len(), 32);
    assert_eq!(
        bounded
            .apply(
                1,
                &Action::AddTotals {
                    statement: 0,
                    after: None
                }
            )
            .unwrap_err(),
        Error::TooLarge
    );
}
