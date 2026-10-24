use super::*;

/// Aggregate filters are branch-local clauses after GROUP BY, independently of the interface language.
#[test]
fn having_is_typed_ordered_and_lossless_in_both_query_languages() {
    for (input, having, where_word) in [
        (
            "SELECT T.Kind, SUM(T.Amount) AS Total FROM Document.Sales AS T GROUP BY T.Kind ORDER BY Total",
            "HAVING",
            "WHERE",
        ),
        (
            "ВЫБРАТЬ T.Kind, СУММА(T.Amount) КАК Total ИЗ Документ.Продажи КАК T СГРУППИРОВАТЬ ПО T.Kind УПОРЯДОЧИТЬ ПО Total",
            "ИМЕЮЩИЕ",
            "ГДЕ",
        ),
    ] {
        let mut value = constructor(input);
        assert!(value.finish().unwrap().is_empty());
        for expression in [
            Expression::Having {
                left: "SUM(T.Amount)".into(),
                comparison: Comparison::Greater,
                right: "&Min".into(),
            },
            Expression::Condition {
                left: "T.Active".into(),
                comparison: Comparison::Equal,
                right: "TRUE".into(),
            },
        ] {
            apply(
                &mut value,
                &Action::Compose {
                    statement: 0,
                    selection: None,
                    expression,
                },
            );
        }
        let owner = &value.model().statements[0];
        assert!(owner.clauses.iter().map(|c| c.kind).eq([
            ClauseKind::Fields,
            ClauseKind::Sources,
            ClauseKind::Conditions,
            ClauseKind::Group,
            ClauseKind::Having,
            ClauseKind::Order
        ]));
        assert!(
            value
                .text()
                .contains(&format!("{having} SUM(T.Amount) > &Min"))
        );
        assert!(value.text().contains(where_word));
        let clause = owner
            .clauses
            .iter()
            .position(|c| c.kind == ClauseKind::Having)
            .unwrap();
        let selection = Selection {
            statement: 0,
            clause,
            item: 0,
        };
        let expression = owner.clauses[clause].items[0]
            .visual(value.text(), &owner.clauses[clause])
            .unwrap();
        assert!(matches!(expression, Expression::Having { .. }));
        assert!(
            value
                .apply(
                    value.revision(),
                    &Action::Compose {
                        statement: 0,
                        selection: Some(selection),
                        expression
                    }
                )
                .unwrap()
                .is_empty()
        );
    }
}

/// Comment decisions and item movement preserve CRLF; deleting the last aggregate filter removes only its clause.
#[test]
fn having_comments_moves_and_removal_preserve_other_clauses() {
    let input = "SELECT SUM(T.Amount) AS Total FROM T GROUP BY T.Kind HAVING // first\r\nSUM(T.Amount)>0 AND COUNT(*)>1 ORDER BY Total";
    let mut value = constructor(input);
    let clause = value.model().statements[0]
        .clauses
        .iter()
        .position(|c| c.kind == ClauseKind::Having)
        .unwrap();
    assert_eq!(
        value
            .apply(
                1,
                &Action::Remove {
                    selection: selection(clause, 0),
                    comments: None
                }
            )
            .unwrap_err(),
        Error::CommentsRequireDecision
    );
    assert_eq!(value.text(), input);
    apply(
        &mut value,
        &Action::Move {
            selection: selection(clause, 1),
            to: 0,
        },
    );
    assert!(value.text().contains("// first\r\n"));
    apply(
        &mut value,
        &Action::Remove {
            selection: selection(clause, 1),
            comments: Some(CommentDecision::Keep),
        },
    );
    apply(
        &mut value,
        &Action::Remove {
            selection: selection(clause, 0),
            comments: Some(CommentDecision::Keep),
        },
    );
    assert!(!value.text().contains("HAVING"));
    assert!(value.text().contains("GROUP BY T.Kind"));
    assert!(value.text().contains("ORDER BY Total"));
    assert!(value.text().contains("// first"));
}

/// UNION branches and nested SELECTs keep independent aggregate filters and reject fragment escapes atomically.
#[test]
fn having_stays_in_selected_nested_or_union_branch() {
    let mut value = constructor(
        "SELECT Q.Total FROM (SELECT SUM(T.Amount) AS Total FROM T GROUP BY T.Kind) AS Q UNION ALL SELECT 1 AS Total",
    );
    let nested = value.model().scopes[1].branches[0];
    let expression = Expression::Having {
        left: "SUM(T.Amount)".into(),
        comparison: Comparison::GreaterOrEqual,
        right: "0".into(),
    };
    apply(
        &mut value,
        &Action::Compose {
            statement: nested,
            selection: None,
            expression,
        },
    );
    assert!(value.text().contains("HAVING SUM(T.Amount) >= 0"));
    let before = value.text().to_owned();
    assert_eq!(
        value
            .apply(
                value.revision(),
                &Action::Compose {
                    statement: nested,
                    selection: None,
                    expression: Expression::Having {
                        left: "1".into(),
                        comparison: Comparison::Equal,
                        right: "0; SELECT 9".into()
                    }
                }
            )
            .unwrap_err(),
        Error::InvalidExpression
    );
    assert_eq!(value.text(), before);
    assert!(
        !value.model().statements[0]
            .clauses
            .iter()
            .any(|c| c.kind == ClauseKind::Having)
    );
    let mut global = constructor("SELECT SUM(1) AS Total");
    apply(
        &mut global,
        &Action::Compose {
            statement: 0,
            selection: None,
            expression: Expression::Having {
                left: "SUM(1)".into(),
                comparison: Comparison::Greater,
                right: "0".into(),
            },
        },
    );
    assert!(global.text().contains("HAVING SUM(1) > 0"));
}
