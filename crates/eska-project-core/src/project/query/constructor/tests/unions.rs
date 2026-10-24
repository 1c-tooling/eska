use super::*;

/// Cloning a branch inserts it before result ordering and preserves the selected source language.
#[test]
fn union_branch_operations_preserve_the_result_tail_and_separator_comments() {
    let mut value = constructor("SELECT I.Ref AS Ref FROM Catalog.Items AS I ORDER BY Ref DESC");
    apply(
        &mut value,
        &Action::AddUnion {
            statement: 0,
            all: true,
        },
    );
    assert!(
        value
            .text()
            .contains("UNION ALL\nSELECT I.Ref AS Ref FROM Catalog.Items AS I\nORDER BY Ref DESC")
    );
    assert_eq!(value.model().scopes[0].branches, [0, 1]);
    assert_eq!(
        value.model().statements[0].append_unavailable(ClauseKind::Order),
        Some(Error::Unsupported)
    );
    apply(
        &mut value,
        &Action::SetUnion {
            statement: 1,
            all: false,
        },
    );
    assert!(!value.text().contains("UNION ALL"));
    apply(
        &mut value,
        &Action::RemoveUnion {
            statement: 1,
            comments: None,
        },
    );
    assert!(value.text().contains("ORDER BY Ref DESC"));
    assert_eq!(value.model().scopes[0].branches, [0]);
    let mut value = constructor("ВЫБРАТЬ 1 КАК А ОБЪЕДИНИТЬ // связь\n ВСЕ ВЫБРАТЬ 2 КАК Б");
    apply(
        &mut value,
        &Action::SetUnion {
            statement: 1,
            all: false,
        },
    );
    assert!(value.text().contains("ОБЪЕДИНИТЬ // связь\n  ВЫБРАТЬ"));
}

/// Shared columns are added, moved and removed in every branch in one revision, using explicit NULL placeholders.
#[test]
fn union_projection_actions_keep_columns_aligned_atomically() {
    let mut value = constructor("SELECT A AS Shared UNION ALL SELECT B AS Ignored ORDER BY Shared");
    let revision = value.revision();
    apply(
        &mut value,
        &Action::Compose {
            statement: 1,
            selection: None,
            expression: Expression::Field {
                expression: "C".into(),
                alias: "Extra".into(),
                aggregate: None,
            },
        },
    );
    assert_eq!(value.revision(), revision + 1);
    assert!(
        value.text().contains(
            "SELECT A AS Shared,\nNULL AS Extra UNION ALL SELECT B AS Ignored,\nC AS Extra"
        )
    );
    assert_eq!(
        value.model().union_mismatches(value.text()),
        [] as [usize; 0]
    );
    apply(
        &mut value,
        &Action::Move {
            selection: Selection {
                statement: 1,
                clause: 0,
                item: 1,
            },
            to: 0,
        },
    );
    for statement in &value.model().statements {
        assert_eq!(
            statement.clauses[0].items[0].output_name(value.text(), &statement.clauses[0]),
            Some("Extra".into())
        );
    }
    apply(
        &mut value,
        &Action::Remove {
            selection: Selection {
                statement: 0,
                clause: 0,
                item: 0,
            },
            comments: None,
        },
    );
    assert!(
        value
            .model()
            .statements
            .iter()
            .all(|statement| statement.clauses[0].items.len() == 1)
    );
    assert!(value.text().ends_with("ORDER BY Shared"));
}

/// Dropping a first branch retains the union's output names, even if successor aliases originally differed.
#[test]
fn removing_first_union_branch_preserves_shared_aliases_and_requires_comment_choice() {
    let mut value = constructor(
        "SELECT A AS Public // retained 😀\nUNION ALL SELECT B AS Private ORDER BY Public",
    );
    let before = value.text().to_owned();
    assert_eq!(
        value
            .apply(
                value.revision(),
                &Action::RemoveUnion {
                    statement: 0,
                    comments: None
                }
            )
            .unwrap_err(),
        Error::CommentsRequireDecision
    );
    assert_eq!(value.text(), before);
    apply(
        &mut value,
        &Action::RemoveUnion {
            statement: 0,
            comments: Some(CommentDecision::Keep),
        },
    );
    assert!(value.text().contains("// retained 😀"));
    assert!(value.text().contains("SELECT B AS Public ORDER BY Public"));
}

/// A known arity mismatch remains repairable in private state but cannot publish a changed canonical draft.
#[test]
fn union_arity_is_checked_before_finish_and_nested_branches_keep_their_parent() {
    let mut value = constructor("SELECT 1 UNION ALL SELECT 2, 3");
    assert_ne!(
        value.model().union_mismatches(value.text()),
        [] as [usize; 0]
    );
    assert!(value.finish().unwrap().is_empty());
    value
        .apply(
            value.revision(),
            &Action::Replace {
                selection: selection(0, 0),
                text: "4".into(),
            },
        )
        .unwrap();
    assert_eq!(value.finish().unwrap_err(), Error::InvalidExpression);
    let mut value = constructor("SELECT Q.A FROM (SELECT A FROM T) AS Q");
    apply(
        &mut value,
        &Action::AddUnion {
            statement: 1,
            all: false,
        },
    );
    assert_eq!(value.model().scopes[1].parent, Some(0));
    assert_eq!(value.model().scopes[1].branches, [1, 2]);
}
