use super::*;

/// A set move preserves raw comments, separators, UTF-8 and one public revision.
#[test]
fn grouped_move_and_copy_publish_once_and_keep_stable_selection_order() {
    let mut value = constructor("\u{feff}SELECT A,\r\n// б 😀\r\nB, C, D FROM T");
    let revision = value.revision();
    apply(
        &mut value,
        &Action::EditItems {
            statement: 0,
            clause: 0,
            items: vec![2, 0],
            operation: ItemSetOperation::Move { to: 3 },
            comments: None,
        },
    );
    assert_eq!(value.revision(), revision + 1);
    let fields = &value.model().statements[0].clauses[0];
    assert_eq!(
        fields
            .items
            .iter()
            .map(|item| item.expression(value.text()))
            .collect::<Vec<_>>(),
        ["B", "A", "C", "D"]
    );
    assert!(value.text().contains("// б 😀\r\n"));
    assert_eq!(
        value
            .item_focus()
            .iter()
            .map(|item| item.item)
            .collect::<Vec<_>>(),
        [1, 2]
    );
    let revision = value.revision();
    apply(
        &mut value,
        &Action::EditItems {
            statement: 0,
            clause: 0,
            items: vec![1, 2],
            operation: ItemSetOperation::Copy { to: 0 },
            comments: None,
        },
    );
    assert_eq!(value.revision(), revision + 1);
    assert!(value.text().contains("A AS A2"));
    assert!(value.text().contains("C AS C2"));
    assert_eq!(
        value
            .item_focus()
            .iter()
            .map(|item| item.item)
            .collect::<Vec<_>>(),
        [0, 1]
    );
}

/// A rejected member cannot leave earlier removals or copies committed.
#[test]
fn grouped_removal_refuses_required_rows_comments_and_stale_or_duplicate_indices_atomically() {
    let original = "SELECT A, // keep 😀\nB, C FROM T";
    for (items, operation, expected) in [
        (vec![0, 1, 2], ItemSetOperation::Remove, Error::RequiredItem),
        (
            vec![0, 1],
            ItemSetOperation::Remove,
            Error::CommentsRequireDecision,
        ),
        (
            vec![0, 0],
            ItemSetOperation::Remove,
            Error::InvalidSelection,
        ),
        (
            vec![0, 99],
            ItemSetOperation::Copy { to: 0 },
            Error::InvalidSelection,
        ),
        (
            vec![0, 1],
            ItemSetOperation::Move { to: 4 },
            Error::InvalidSelection,
        ),
    ] {
        let mut value = constructor(original);
        assert_eq!(
            value
                .apply(
                    value.revision(),
                    &Action::EditItems {
                        statement: 0,
                        clause: 0,
                        items,
                        operation,
                        comments: None
                    }
                )
                .unwrap_err(),
            expected
        );
        assert_eq!(value.text(), original);
        assert_eq!(value.revision(), 1);
    }
    let mut value = constructor(original);
    apply(
        &mut value,
        &Action::EditItems {
            statement: 0,
            clause: 0,
            items: vec![0, 1],
            operation: ItemSetOperation::Remove,
            comments: Some(CommentDecision::Keep),
        },
    );
    assert!(value.text().contains("// keep 😀"));
    assert!(value.text().contains('C'));
    let snapshot = value.snapshot();
    assert_eq!(
        value
            .apply(
                1,
                &Action::EditItems {
                    statement: 0,
                    clause: 0,
                    items: vec![0],
                    operation: ItemSetOperation::Remove,
                    comments: None
                }
            )
            .unwrap_err(),
        Error::StaleRevision
    );
    assert_eq!(value.snapshot(), snapshot);
}

/// Source copies preserve the complete nested query rather than generating the default child template.
#[test]
fn source_copy_keeps_nested_body_and_allocates_an_independent_alias() {
    let mut value = constructor("SELECT Q.N FROM (SELECT // inner 😀\n 42 AS N) AS Q");
    apply(
        &mut value,
        &Action::EditItems {
            statement: 0,
            clause: 1,
            items: vec![0],
            operation: ItemSetOperation::Copy { to: 1 },
            comments: None,
        },
    );
    assert_eq!(value.text().matches("42 AS N").count(), 2);
    assert_eq!(value.text().matches("// inner 😀").count(), 2);
    assert!(value.text().contains("AS Q2"));
    assert!(value.text().starts_with("SELECT Q.N FROM"));
}

/// Every UNION branch follows a grouped result move and receives copied output columns atomically.
#[test]
fn grouped_result_edits_synchronize_union_columns() {
    let mut value =
        constructor("SELECT 1 AS A, 2 AS B, 3 AS C UNION ALL SELECT 4 AS X, 5 AS Y, 6 AS Z");
    apply(
        &mut value,
        &Action::EditItems {
            statement: 0,
            clause: 0,
            items: vec![0, 2],
            operation: ItemSetOperation::Move { to: 3 },
            comments: None,
        },
    );
    assert!(value.text().contains("SELECT 2 AS B, 1 AS A, 3 AS C"));
    assert!(value.text().contains("SELECT 5 AS Y, 4 AS X, 6 AS Z"));
    apply(
        &mut value,
        &Action::EditItems {
            statement: 0,
            clause: 0,
            items: vec![0, 1],
            operation: ItemSetOperation::Copy { to: 3 },
            comments: None,
        },
    );
    assert!(value.text().contains("2 AS B2"));
    assert!(value.text().contains("1 AS A2"));
    assert!(value.text().contains("NULL AS B2"));
    assert!(value.text().contains("NULL AS A2"));
    assert_eq!(
        value.model().union_mismatches(value.text()),
        Vec::<usize>::new()
    );
}

/// A missing source and several fields are added together with the backend's collision-free alias.
#[test]
fn adding_a_source_and_fields_is_atomic_and_uses_the_new_alias() {
    for (input, table, expected) in [
        (
            "SELECT T.Ref FROM Catalog.Items AS T",
            "Catalog.Items",
            "Items",
        ),
        (
            "ВЫБРАТЬ Т.Ссылка ИЗ Справочник.Товары КАК Т",
            "Справочник.Товары",
            "Товары",
        ),
    ] {
        let mut value = constructor(input);
        apply(
            &mut value,
            &Action::AddSourceFields {
                statement: 0,
                source: Expression::Source {
                    table: table.into(),
                    alias: String::new(),
                },
                fields: vec!["Code".into(), "Ref".into()],
            },
        );
        assert!(value.text().contains(&format!("{expected}.Code")));
        assert!(value.text().contains(&format!("{expected}.Ref")));
        assert_eq!(value.revision(), 2);
        assert_eq!(value.item_focus().len(), 2);
    }
    let mut value = constructor("SELECT 1");
    assert_eq!(
        value
            .apply(
                1,
                &Action::AddSourceFields {
                    statement: 0,
                    source: Expression::Source {
                        table: "Catalog.Items".into(),
                        alias: String::new()
                    },
                    fields: vec!["Code; SELECT 2".into()]
                }
            )
            .unwrap_err(),
        Error::InvalidExpression
    );
    assert_eq!(value.text(), "SELECT 1");
    assert_eq!(value.revision(), 1);
}

/// Fixed overall placement, wildcards and unsupported source syntax keep their original bytes on refusal.
#[test]
fn unsupported_group_copies_and_noop_moves_preserve_exact_input() {
    let mut value = constructor("SELECT T.* FROM Catalog.Items AS T");
    assert_eq!(
        value
            .apply(
                1,
                &Action::EditItems {
                    statement: 0,
                    clause: 0,
                    items: vec![0],
                    operation: ItemSetOperation::Copy { to: 1 },
                    comments: None
                }
            )
            .unwrap_err(),
        Error::Unsupported
    );
    let before = value.text().to_owned();
    let plan = value
        .apply(
            1,
            &Action::EditItems {
                statement: 0,
                clause: 0,
                items: vec![0],
                operation: ItemSetOperation::Move { to: 0 },
                comments: None,
            },
        )
        .unwrap();
    assert!(plan.is_empty());
    assert_eq!(value.text(), before);
    assert_eq!(value.revision(), 1);
}

/// Even a failure after the first successful staged mutation cannot escape the transaction.
#[test]
fn late_copy_and_source_field_failures_leave_the_original_snapshot() {
    let mut value = constructor("SELECT 1 AS A, T.* FROM Catalog.Items AS T");
    let before = value.snapshot();
    assert_eq!(
        value
            .apply(
                1,
                &Action::EditItems {
                    statement: 0,
                    clause: 0,
                    items: vec![0, 1],
                    operation: ItemSetOperation::Copy { to: 2 },
                    comments: None
                }
            )
            .unwrap_err(),
        Error::Unsupported
    );
    assert_eq!(value.snapshot(), before);
    assert_eq!(value.revision(), 1);
    let mut value = constructor("SELECT 1");
    assert_eq!(
        value
            .apply(
                1,
                &Action::AddSourceFields {
                    statement: 0,
                    source: Expression::Source {
                        table: "Catalog.Items".into(),
                        alias: String::new()
                    },
                    fields: vec!["Code".into(), "X".repeat(16_384)]
                }
            )
            .unwrap_err(),
        Error::TooLarge
    );
    assert_eq!(value.text(), "SELECT 1");
    assert_eq!(value.revision(), 1);
}

/// Explicit source aliases cannot collide, and copied total aliases remain independently named.
#[test]
fn source_alias_collisions_and_total_copies_add_matching_output_columns() {
    let mut value = constructor("SELECT T.Ref FROM Catalog.Items AS T");
    assert_eq!(
        value
            .apply(
                1,
                &Action::AddSourceFields {
                    statement: 0,
                    source: Expression::Source {
                        table: "Catalog.Other".into(),
                        alias: "t".into()
                    },
                    fields: vec!["Code".into()]
                }
            )
            .unwrap_err(),
        Error::InvalidExpression
    );
    assert_eq!(value.revision(), 1);
    let mut value = constructor("SELECT 1 AS N TOTALS SUM(N) AS N BY OVERALL");
    for _ in 0..2 {
        apply(
            &mut value,
            &Action::EditItems {
                statement: 0,
                clause: 1,
                items: vec![0],
                operation: ItemSetOperation::Copy { to: 1 },
                comments: None,
            },
        );
    }
    assert!(value.text().contains("AS N2"));
    assert!(value.text().contains("AS N3"));
    let groups = &value.model().statements[0].clauses[2];
    assert_eq!(
        value.copy_unavailable(selection(2, 0)),
        Some(Error::Unsupported)
    );
    assert_eq!(groups.items.len(), 1);
}
