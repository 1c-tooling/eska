use super::*;

/// A first aggregate creates a complete totals section after result ordering and in the query's own language.
#[test]
fn totals_create_complete_sections_and_preserve_no_op_forms() {
    for (query, marker) in [
        (
            "SELECT Amount FROM T ORDER BY Amount",
            "TOTALS SUM(Amount)\nBY OVERALL",
        ),
        (
            "ВЫБРАТЬ Сумма ИЗ Т УПОРЯДОЧИТЬ ПО Сумма",
            "ИТОГИ СУММА(Amount)\nПО ОБЩИЕ",
        ),
    ] {
        let mut value = constructor(query);
        apply(
            &mut value,
            &Action::Compose {
                statement: 0,
                selection: None,
                expression: Expression::TotalField {
                    expression: "Amount".into(),
                    alias: String::new(),
                    aggregate: Some(Aggregate::Sum),
                },
            },
        );
        assert!(value.text().contains(marker));
        let clause = value.model().statements[0]
            .clauses
            .iter()
            .position(|clause| clause.kind == ClauseKind::Totals)
            .unwrap();
        let node = &value.model().statements[0].clauses[clause];
        let expression = node.items[0].visual(value.text(), node).unwrap();
        let before = value.text().to_owned();
        apply(
            &mut value,
            &Action::Compose {
                statement: 0,
                selection: Some(selection(clause, 0)),
                expression,
            },
        );
        assert_eq!(value.text(), before);
    }
}

/// Totals can omit aggregate expressions, but deleting the final grouping removes the whole section safely.
#[test]
fn automatic_totals_and_last_group_removal_never_leave_an_orphan_header() {
    let mut value = constructor("SELECT SUM(A) AS A FROM T GROUP BY B TOTALS BY B");
    let clauses = &value.model().statements[0].clauses;
    let index = clauses
        .iter()
        .position(|clause| clause.kind == ClauseKind::Totals)
        .unwrap();
    assert_eq!(clauses[index].items.len(), 0);
    apply(
        &mut value,
        &Action::Compose {
            statement: 0,
            selection: None,
            expression: Expression::TotalField {
                expression: "A".into(),
                alias: String::new(),
                aggregate: Some(Aggregate::Sum),
            },
        },
    );
    apply(
        &mut value,
        &Action::Remove {
            selection: selection(index, 0),
            comments: None,
        },
    );
    assert_eq!(value.model().statements[0].clauses[index].items.len(), 0);
    apply(
        &mut value,
        &Action::Remove {
            selection: selection(index + 1, 0),
            comments: None,
        },
    );
    assert!(
        value.model().statements[0]
            .clauses
            .iter()
            .all(|clause| !matches!(clause.kind, ClauseKind::Totals | ClauseKind::TotalGroups))
    );
}

/// Calendar gap filling and hierarchy remain typed values, with idempotent OVERALL placed first.
#[test]
fn totals_grouping_modifiers_and_overall_are_formatted_and_projected_losslessly() {
    let mut value = constructor("SELECT Period, Amount FROM T");
    apply(
        &mut value,
        &Action::Compose {
            statement: 0,
            selection: None,
            expression: Expression::TotalGroup {
                expression: "Period".into(),
                alias: String::new(),
                hierarchy: Hierarchy::None,
                period: Some(PeriodRange {
                    unit: PeriodUnit::Month,
                    begin: Some("&Start".into()),
                    end: Some("&End".into()),
                }),
            },
        },
    );
    assert!(
        value
            .text()
            .contains("BY Period PERIODS(MONTH, &Start, &End)")
    );
    apply(
        &mut value,
        &Action::Compose {
            statement: 0,
            selection: None,
            expression: Expression::Overall,
        },
    );
    assert!(value.text().contains("BY OVERALL,\nPeriod PERIODS"));
    let revision = value.revision();
    apply(
        &mut value,
        &Action::Compose {
            statement: 0,
            selection: None,
            expression: Expression::Overall,
        },
    );
    assert_eq!(value.revision(), revision);
    let clause = value.model().statements[0].clauses.last().unwrap();
    let expression = clause.items[1].visual(value.text(), clause).unwrap();
    assert!(matches!(
        &expression,
        Expression::TotalGroup {
            period: Some(PeriodRange {
                unit: PeriodUnit::Month,
                ..
            }),
            ..
        }
    ));
    let index = value.model().statements[0].clauses.len() - 1;
    assert!(
        value
            .apply(
                value.revision(),
                &Action::Move {
                    selection: selection(index, 1),
                    to: 0
                }
            )
            .is_err()
    );
}

/// UNION copies exclude the entire global tail; totals on inner SELECTs remain unavailable.
#[test]
fn totals_belong_to_the_union_result_and_comment_decisions_cover_the_whole_section() {
    let mut value = constructor("SELECT A FROM T ORDER BY A TOTALS SUM(A) // keep\n BY OVERALL");
    apply(
        &mut value,
        &Action::AddUnion {
            statement: 0,
            all: true,
        },
    );
    assert_eq!(value.text().matches("TOTALS").count(), 1);
    let last = value.model().statements[1].clauses.len() - 1;
    assert_eq!(
        value
            .apply(
                value.revision(),
                &Action::Remove {
                    selection: Selection {
                        statement: 1,
                        clause: last,
                        item: 0
                    },
                    comments: None
                }
            )
            .unwrap_err(),
        Error::CommentsRequireDecision
    );
    apply(
        &mut value,
        &Action::Remove {
            selection: Selection {
                statement: 1,
                clause: last,
                item: 0,
            },
            comments: Some(CommentDecision::Keep),
        },
    );
    assert!(value.text().contains("// keep"));
    let value = constructor("SELECT Q.A FROM (SELECT A FROM T) AS Q");
    assert_eq!(
        value.model().statements[1].append_unavailable(ClauseKind::Totals),
        Some(Error::Unsupported)
    );
}
