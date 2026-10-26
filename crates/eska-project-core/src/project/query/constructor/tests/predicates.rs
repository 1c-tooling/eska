use super::*;
use crate::project::query::constructor::Predicate;

/// Keyword strings and BETWEEN's conjunction cannot change boolean precedence or row ownership.
#[test]
fn boolean_projection_and_unchanged_reapplication_preserve_exact_bytes() {
    let text = "\u{feff}SELECT 1 AS N\r\nWHERE // condition 😀\r\nN BETWEEN 1 AND 3 OR NOT (N = 4 AND \"OR AND\" = \"OR AND\")";
    let mut value = constructor(text);
    assert!(value.model().statements[0].unavailable.is_none());
    assert_eq!(value.model().statements[0].clauses[1].items.len(), 1);
    let predicate = value.predicate(0, 1).unwrap();
    assert!(matches!(predicate, Predicate::Any { .. }));
    let plan = value.apply(
        value.revision(),
        &Action::SetPredicate {
            statement: 0,
            clause: ClauseKind::Conditions,
            selection: None,
            predicate,
        },
    );
    assert!(plan.unwrap().is_empty());
    assert_eq!(value.text(), text);
    assert_eq!(value.revision(), 1);
}

/// Each JOIN has an independent ON range; editing one leaves other joins and source spellings exact.
#[test]
fn grouped_conditions_and_join_predicates_are_independent_and_atomic() {
    let mut value = constructor(
        "SELECT A.N FROM (SELECT 1 AS N) AS A LEFT JOIN (SELECT 1 AS N) AS B ON A.N = B.N RIGHT JOIN (SELECT 2 AS N) AS C ON C.N = A.N",
    );
    let predicate = Predicate::Any {
        children: vec![
            Predicate::Compare {
                left: "A.N".into(),
                comparison: Comparison::Equal,
                right: "B.N".into(),
            },
            Predicate::IsNull {
                expression: "B.N".into(),
                negated: false,
            },
        ],
    };
    apply(
        &mut value,
        &Action::SetPredicate {
            statement: 0,
            clause: ClauseKind::Joins,
            selection: Some(selection(2, 0)),
            predicate,
        },
    );
    assert!(value.text().contains("ON (A.N = B.N OR B.N IS NULL)"));
    assert!(
        value
            .text()
            .contains("RIGHT JOIN (SELECT 2 AS N) AS C ON C.N = A.N")
    );
    let relation = value.join_context(0, 2).unwrap();
    assert_eq!(relation.kind, JoinKind::Left);
    assert_eq!(relation.left, vec!["A"]);
    assert_eq!(relation.right, "B");
    apply(
        &mut value,
        &Action::SetJoinKind {
            selection: selection(2, 0),
            join: JoinKind::Full,
        },
    );
    assert!(value.text().contains("FULL JOIN (SELECT 1 AS N) AS B ON"));
    let before = value.text().to_owned();
    let revision = value.revision();
    assert_eq!(
        value
            .apply(
                revision,
                &Action::SetPredicate {
                    statement: 0,
                    clause: ClauseKind::Conditions,
                    selection: None,
                    predicate: Predicate::Compare {
                        left: "N".into(),
                        comparison: Comparison::Equal,
                        right: String::new()
                    }
                }
            )
            .unwrap_err(),
        Error::InvalidExpression
    );
    assert_eq!(value.text(), before);
    assert_eq!(value.revision(), revision);
}

/// An explicit move transfers comments once and rejects outer aggregates without confusing a nested SELECT.
#[test]
fn predicate_moves_preserve_comments_and_aggregate_scope() {
    let mut value = constructor("SELECT COUNT(*) AS N FROM T HAVING // keep 😀\r\nCOUNT(*) > 0");
    let before = value.text().to_owned();
    assert_eq!(
        value
            .apply(
                1,
                &Action::MovePredicate {
                    selection: selection(2, 0),
                    target: ClauseKind::Conditions
                }
            )
            .unwrap_err(),
        Error::InvalidExpression
    );
    assert_eq!(value.text(), before);
    assert_eq!(value.revision(), 1);
    let mut value =
        constructor("SELECT N FROM T WHERE // keep 😀\r\nN IN (SELECT COUNT(*) FROM Other)");
    apply(
        &mut value,
        &Action::MovePredicate {
            selection: selection(2, 0),
            target: ClauseKind::Having,
        },
    );
    assert_eq!(value.text().matches("// keep 😀").count(), 1);
    assert!(!value.text().contains("WHERE"));
    assert!(value.text().contains("HAVING"));
    assert_eq!(value.revision(), 2);
    let clause = value.model().statements[0]
        .clauses
        .iter()
        .position(|clause| clause.kind == ClauseKind::Having)
        .unwrap();
    apply(
        &mut value,
        &Action::MovePredicate {
            selection: selection(clause, 0),
            target: ClauseKind::Conditions,
        },
    );
    assert_eq!(value.text().matches("// keep 😀").count(), 1);
}

/// Adding an AND condition to an existing OR group must preserve its entire old meaning.
#[test]
fn appending_after_or_keeps_the_old_group_parenthesized() {
    let mut value = constructor("SELECT 1 WHERE A = 1 OR B = 2");
    apply(
        &mut value,
        &Action::Append {
            statement: 0,
            clause: ClauseKind::Conditions,
            text: "C = 3".into(),
            join: JoinKind::Inner,
        },
    );
    assert!(
        matches!(value.predicate(0,1),Some(Predicate::All{children}) if matches!(children[0],Predicate::Any{..}))
    );
}
