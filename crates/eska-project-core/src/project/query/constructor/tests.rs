use super::*;

mod having;
mod nested;
mod packages;
mod totals;
mod unions;
mod virtuals;

/// Implicit source names retain complete qualification; nested property names are not aliases to another source.
#[test]
fn implicit_source_qualification_and_nested_properties_survive_alias_changes() {
    let mut value =
        constructor("SELECT Catalog.Items.Ref FROM Catalog.Items WHERE Catalog.Items.Code = &Code");
    assert_eq!(
        value.model().statements[0].source_bindings(value.text())[0].alias,
        "Catalog.Items"
    );
    apply(
        &mut value,
        &Action::Compose {
            statement: 0,
            selection: Some(selection(1, 0)),
            expression: Expression::Source {
                table: "Catalog.Items".into(),
                alias: "I".into(),
            },
        },
    );
    assert!(
        value
            .text()
            .contains("SELECT I.Ref FROM Catalog.Items AS I WHERE I.Code")
    );
    let mut value =
        constructor("SELECT T.Ref, P.T.Ref FROM Catalog.Items AS T, Catalog.Other AS P");
    apply(
        &mut value,
        &Action::Compose {
            statement: 0,
            selection: Some(selection(1, 0)),
            expression: Expression::Source {
                table: "Catalog.Items".into(),
                alias: "I".into(),
            },
        },
    );
    assert!(value.text().contains("SELECT I.Ref, P.T.Ref"));
}

/// Every structured page formats in the query language while retaining the same lossless source model.
#[test]
fn structured_pages_format_operands_aliases_aggregates_and_direction_in_both_languages() {
    for (original, sum, condition, group, order) in [
        (
            "SELECT T.Amount FROM Document.Sales AS T",
            "SUM(T.Amount) AS Total",
            "WHERE T.Amount >= &Limit",
            "GROUP BY T.Code",
            "ORDER BY T.Code DESC",
        ),
        (
            "ВЫБРАТЬ Т.Сумма ИЗ Документ.Продажа КАК Т",
            "СУММА(T.Amount) КАК Total",
            "ГДЕ T.Amount >= &Limit",
            "СГРУППИРОВАТЬ ПО T.Code",
            "УПОРЯДОЧИТЬ ПО T.Code УБЫВ",
        ),
    ] {
        let mut value = constructor(original);
        for expression in [
            Expression::Field {
                expression: "T.Amount".into(),
                alias: "Total".into(),
                aggregate: Some(Aggregate::Sum),
            },
            Expression::Condition {
                left: "T.Amount".into(),
                comparison: Comparison::GreaterOrEqual,
                right: "&Limit".into(),
            },
            Expression::Group {
                expression: "T.Code".into(),
            },
            Expression::Order {
                expression: "T.Code".into(),
                descending: true,
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
        for fragment in [sum, condition, group, order] {
            assert!(
                value.text().contains(fragment),
                "{fragment}: {}",
                value.text()
            );
        }
    }
}

/// Failed Add All is atomic and successful Add All does not duplicate selected field expressions.
#[test]
fn selecting_all_fields_is_atomic_deduplicated_and_revision_checked() {
    let mut value = constructor("SELECT T.Ref FROM Catalog.Items AS T");
    let before = value.text().to_owned();
    let revision = value.revision();
    assert!(
        value
            .apply(
                revision,
                &Action::AppendFields {
                    statement: 0,
                    fields: vec!["T.Code".into(), "FROM Bad".into()]
                }
            )
            .is_err()
    );
    assert_eq!(value.text(), before);
    assert_eq!(value.revision(), revision);
    apply(
        &mut value,
        &Action::AppendFields {
            statement: 0,
            fields: vec!["t.ref".into(), "T.Code".into(), "T.Code".into()],
        },
    );
    assert_eq!(value.text().matches("T.Code").count(), 1);
    assert_eq!(value.text().matches("T.Ref").count(), 1);
    assert!(matches!(
        value.apply(
            revision,
            &Action::AppendFields {
                statement: 0,
                fields: vec![]
            }
        ),
        Err(Error::StaleRevision)
    ));
}

/// Structured source renaming changes qualified references, preserving metadata paths, strings and comments.
#[test]
fn source_alias_renaming_preserves_comments_and_updates_only_proven_references() {
    let mut value = constructor(
        "SELECT T.Ref, \"T.Ref\" AS Text FROM Catalog.T AS T WHERE T.Code = &Code // T.Ref\nORDER BY T.Ref",
    );
    apply(
        &mut value,
        &Action::Compose {
            statement: 0,
            selection: Some(selection(1, 0)),
            expression: Expression::Source {
                table: "Catalog.T".into(),
                alias: "Items".into(),
            },
        },
    );
    assert!(value.text().contains("SELECT Items.Ref, \"T.Ref\" AS Text"));
    assert!(value.text().contains("FROM Catalog.T AS Items"));
    assert!(value.text().contains("WHERE Items.Code"));
    assert!(value.text().contains("// T.Ref"));
    assert!(value.text().contains("ORDER BY Items.Ref"));
}

/// JOIN type edits retain header comments and structured metadata aliases are unique within a statement.
#[test]
fn structured_joins_preserve_header_comments_and_automatically_qualify_the_new_table_field() {
    let mut value = constructor(
        "SELECT T.Ref FROM Catalog.Items AS T LEFT /* header 😀 */ JOIN Catalog.Other AS O ON T.Ref = O.Ref",
    );
    apply(
        &mut value,
        &Action::Compose {
            statement: 0,
            selection: Some(selection(2, 0)),
            expression: Expression::Join {
                table: "Catalog.Other".into(),
                alias: "Other".into(),
                join: JoinKind::Right,
                left: "T.Ref".into(),
                comparison: Comparison::Equal,
                right: "Ref".into(),
            },
        },
    );
    assert!(value.text().contains("RIGHT /* header 😀 */ JOIN"));
    assert!(value.text().contains("ON T.Ref = Other.Ref"));
    apply(
        &mut value,
        &Action::Compose {
            statement: 0,
            selection: None,
            expression: Expression::Join {
                table: "Catalog.Other".into(),
                alias: String::new(),
                join: JoinKind::Left,
                left: "T.Ref".into(),
                comparison: Comparison::Equal,
                right: "Ref".into(),
            },
        },
    );
    assert!(value.text().contains("AS Other2 ON T.Ref = Other2.Ref"));
}

/// Accepting unchanged structured forms is byte-exact, even with original keyword casing and aggregate spelling.
#[test]
fn unchanged_structured_forms_and_compound_aggregate_expressions_remain_exact() {
    let mut value =
        constructor("select sum(T.Amount) as Total from Catalog.Items as T order by T.Code desc");
    let before = value.text().to_owned();
    for clause in 0..3 {
        let owner = &value.model().statements[0].clauses[clause];
        let expression = owner.items[0].visual(value.text(), owner).unwrap();
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
    let value = constructor("SELECT SUM(A) + MAX(B) AS Total FROM T");
    let clause = &value.model().statements[0].clauses[0];
    assert!(matches!(
        clause.items[0].visual(value.text(), clause),
        Some(Expression::Field {
            aggregate: None,
            ..
        })
    ));
}

/// Missing operands and alias payloads cannot escape a structured control into another clause.
#[test]
fn structured_controls_reject_empty_operands_and_alias_injection() {
    let mut value = constructor("SELECT T.Ref FROM Catalog.Items AS T");
    let before = value.text().to_owned();
    for expression in [
        Expression::Condition {
            left: "T.Ref".into(),
            comparison: Comparison::Equal,
            right: String::new(),
        },
        Expression::Field {
            expression: "T.Ref".into(),
            alias: "X FROM Bad".into(),
            aggregate: None,
        },
    ] {
        assert!(
            value
                .apply(
                    value.revision(),
                    &Action::Compose {
                        statement: 0,
                        selection: None,
                        expression
                    }
                )
                .is_err()
        );
        assert_eq!(value.text(), before);
    }
}

/// A fresh pure-core model has no project, analyzer, editor or filesystem dependency.
fn constructor(input: &str) -> Constructor {
    Constructor::new(Arc::from(input)).expect("bounded query model")
}

/// Selection indices belong to one exact model revision.
const fn selection(clause: usize, item: usize) -> Selection {
    Selection {
        statement: 0,
        clause,
        item,
    }
}

/// Check both the operation's output and the final minimal draft plan.
fn apply(value: &mut Constructor, action: &Action) {
    let original = value.text().to_owned();
    let plan = value
        .apply(value.revision(), action)
        .expect("safe visual operation");
    assert_eq!(plan.original(), original);
    assert_eq!(plan.output(), value.text());
    assert_eq!(value.finish().expect("final plan").output(), value.text());
}

#[test]
fn opening_and_finishing_unknown_packages_is_byte_exact() {
    for text in [
        "\u{feff}ВЫБРАТЬ\r\n\t1 КАК А;\r\n// 😀\r\nВЫБРАТЬ {ВЫБРАТЬ Поле.*} ИЗ Таблица;\r\n",
        "SELECT A INTO Temp FROM Catalog.Товары;\nDESTROY Temp;\nSELECT B FROM Temp AS T",
        "SELECT \"unclosed string",
        "",
    ] {
        let value = constructor(text);
        let plan = value.finish().expect("no-op");
        assert!(plan.is_empty());
        assert_eq!(plan.output().as_bytes(), text.as_bytes());
    }
}

#[test]
fn every_first_release_clause_can_be_added_without_regenerating_other_text() {
    let mut value = constructor(
        "ВЫБРАТЬ\r\n\tТ.Артикул КАК Артикул\r\nИЗ Справочник.Товары КАК Т;\r\nУНИЧТОЖИТЬ ВТ;\r\n",
    );
    for (clause, text) in [
        (ClauseKind::Fields, "Т.Код КАК Код"),
        (ClauseKind::Sources, "Справочник.Покупатели КАК П"),
        (
            ClauseKind::Joins,
            "Справочник.Покупатели КАК С ПО Т.Ссылка = С.Ссылка",
        ),
        (ClauseKind::Conditions, "Т.Код = &Код"),
        (ClauseKind::Conditions, "Т.Артикул <> \"\""),
        (ClauseKind::Group, "Т.Артикул"),
        (ClauseKind::Order, "Т.Артикул УБЫВ"),
    ] {
        apply(
            &mut value,
            &Action::Append {
                statement: 0,
                clause,
                text: text.to_owned(),
                join: JoinKind::Left,
            },
        );
    }
    assert!(value.text().contains("ЛЕВОЕ СОЕДИНЕНИЕ"));
    assert!(value.text().contains("ГДЕ Т.Код = &Код\r\nИ Т.Артикул"));
    assert!(value.text().ends_with(";\r\nУНИЧТОЖИТЬ ВТ;\r\n"));
    assert!(!value.text().replace("\r\n", "").contains('\n'));
}

#[test]
fn comments_follow_a_reordered_field_and_expression_edits_retain_them() {
    let text = "SELECT\n// first 😀\n A,\n B // trailing B\nFROM Catalog.Товары AS T";
    let mut value = constructor(text);
    apply(
        &mut value,
        &Action::Move {
            selection: selection(0, 0),
            to: 1,
        },
    );
    assert!(value.text().contains("B // trailing B\n,"));
    assert!(value.text().contains("// first 😀\n A"));
    let before = value.text().to_owned();
    apply(
        &mut value,
        &Action::Replace {
            selection: selection(0, 1),
            text: "A AS Renamed".to_owned(),
        },
    );
    assert!(value.text().contains("// first 😀\nA AS Renamed"));
    assert_eq!(value.text().matches("// first 😀").count(), 1);
    assert_eq!(value.model().statements[0].clauses.len(), 2);
    assert_ne!(before, value.text());
}

#[test]
fn deletion_of_commented_nodes_requires_an_explicit_keep_or_discard() {
    let text = "SELECT // keep 😀\n A, B FROM T";
    let mut keep = constructor(text);
    assert_eq!(
        keep.apply(
            1,
            &Action::Remove {
                selection: selection(0, 0),
                comments: None
            }
        )
        .expect_err("decision required"),
        Error::CommentsRequireDecision
    );
    assert_eq!(keep.text(), text);
    apply(
        &mut keep,
        &Action::Remove {
            selection: selection(0, 0),
            comments: Some(CommentDecision::Keep),
        },
    );
    assert!(keep.text().contains("// keep 😀"));
    assert!(!keep.text().contains(" A,"));
    let mut discard = constructor(text);
    apply(
        &mut discard,
        &Action::Remove {
            selection: selection(0, 0),
            comments: Some(CommentDecision::Discard),
        },
    );
    assert_eq!(discard.text(), "SELECT B FROM T");
}

#[test]
fn removing_a_commented_optional_clause_keeps_the_following_clause_visible() {
    let mut value = constructor("SELECT A FROM T WHERE // predicate\n A = 1 ORDER BY A");
    assert_eq!(
        value
            .apply(
                1,
                &Action::Remove {
                    selection: selection(2, 0),
                    comments: None
                }
            )
            .expect_err("decision"),
        Error::CommentsRequireDecision
    );
    apply(
        &mut value,
        &Action::Remove {
            selection: selection(2, 0),
            comments: Some(CommentDecision::Keep),
        },
    );
    assert!(!value.text().contains("WHERE"));
    assert!(value.text().contains("// predicate\nORDER BY A"));
    assert_eq!(
        value.model().statements[0]
            .clauses
            .last()
            .expect("order")
            .kind,
        ClauseKind::Order
    );
}

#[test]
fn opaque_scd_and_unknown_statements_cannot_be_partially_edited() {
    let text = "SELECT {T.*} FROM T;\nSELECT A FROM T;\nDESTROY Temp";
    let mut value = constructor(text);
    assert_eq!(
        value
            .apply(
                1,
                &Action::Append {
                    statement: 0,
                    clause: ClauseKind::Fields,
                    text: "B".to_owned(),
                    join: JoinKind::Inner
                }
            )
            .expect_err("opaque statement"),
        Error::Unsupported
    );
    apply(
        &mut value,
        &Action::Append {
            statement: 1,
            clause: ClauseKind::Fields,
            text: "B".to_owned(),
            join: JoinKind::Inner,
        },
    );
    assert!(value.text().starts_with("SELECT {T.*} FROM T;\n"));
    assert!(value.text().ends_with(";\nDESTROY Temp"));
}

#[test]
fn nested_quoted_commas_comments_and_multibyte_text_do_not_split_rows() {
    let mut value = constructor(
        "SELECT Func(\"a,b;FROM\", 2) AS А, T.Поле FROM T WHERE (A = 1 AND B = 2) AND C = 3 ORDER BY А DESC",
    );
    assert_eq!(value.model().statements[0].clauses[0].items.len(), 2);
    assert_eq!(value.model().statements[0].clauses[2].items.len(), 2);
    apply(
        &mut value,
        &Action::Move {
            selection: selection(0, 1),
            to: 0,
        },
    );
    assert!(
        value
            .text()
            .starts_with("SELECT T.Поле, Func(\"a,b;FROM\", 2) AS А")
    );
}

#[test]
fn required_items_and_structural_injections_are_rejected_without_changing_revision() {
    let mut value = constructor("SELECT A FROM T");
    assert_eq!(
        value
            .apply(
                1,
                &Action::Remove {
                    selection: selection(0, 0),
                    comments: None
                }
            )
            .expect_err("required field"),
        Error::RequiredItem
    );
    for text in [
        "B; SELECT C",
        "B FROM Other",
        "B, C",
        "{B}",
        "\"broken",
        "// only comment",
    ] {
        assert!(
            value
                .apply(
                    1,
                    &Action::Replace {
                        selection: selection(0, 0),
                        text: text.to_owned()
                    }
                )
                .is_err(),
            "{text}"
        );
        assert_eq!(value.text(), "SELECT A FROM T");
        assert_eq!(value.revision(), 1);
    }
}

#[test]
fn no_op_operations_preserve_modifiers_whitespace_comments_and_revision() {
    let text = "SELECT DISTINCT TOP 10\t A /* keep */ FROM T\r\n";
    let mut value = constructor(text);
    apply(
        &mut value,
        &Action::Replace {
            selection: selection(0, 0),
            text: "A".to_owned(),
        },
    );
    apply(
        &mut value,
        &Action::Move {
            selection: selection(0, 0),
            to: 0,
        },
    );
    assert_eq!(value.text(), text);
    assert_eq!(value.revision(), 1);
    apply(
        &mut value,
        &Action::Append {
            statement: 0,
            clause: ClauseKind::Fields,
            text: "B".to_owned(),
            join: JoinKind::Inner,
        },
    );
    assert_eq!(
        value
            .apply(
                1,
                &Action::Remove {
                    selection: selection(0, 1),
                    comments: None
                }
            )
            .expect_err("stale row indices"),
        Error::StaleRevision
    );
    assert!(
        value
            .text()
            .starts_with("SELECT DISTINCT TOP 10\t A /* keep */")
    );
}

#[test]
fn ambiguous_condition_operators_and_unresolved_modifiers_remain_opaque() {
    for text in [
        "SELECT A FROM T WHERE A BETWEEN 1 AND 2",
        "SELECT A FROM T WHERE A OR B",
        "SELECT TOP &Count A FROM T",
        "SELECT A FROM T ORDER A",
        "SELECT A FROM T WHERE CASE WHEN A AND B THEN 1 ELSE 0 END = 1",
    ] {
        let mut value = constructor(text);
        assert_eq!(
            value.model().statements[0].unavailable,
            Some(Unavailable::UnsupportedConstruct)
        );
        assert_eq!(
            value
                .apply(
                    1,
                    &Action::Append {
                        statement: 0,
                        clause: ClauseKind::Fields,
                        text: "B".to_owned(),
                        join: JoinKind::Inner
                    }
                )
                .expect_err("ambiguous statement"),
            Error::Unsupported
        );
        assert_eq!(value.text(), text);
    }
}

#[test]
fn semicolons_inside_unknown_scd_extensions_never_expose_editable_inner_statements() {
    let text = "SELECT {UNKNOWN A; SELECT InnerField FROM InnerTable; T.*} FROM T; SELECT A FROM T";
    let mut value = constructor(text);
    assert_eq!(value.model().statements.len(), 2);
    assert_eq!(
        value.model().statements[0].unavailable,
        Some(Unavailable::UnsupportedConstruct)
    );
    apply(
        &mut value,
        &Action::Append {
            statement: 1,
            clause: ClauseKind::Fields,
            text: "B".to_owned(),
            join: JoinKind::Inner,
        },
    );
    assert!(
        value
            .text()
            .starts_with("SELECT {UNKNOWN A; SELECT InnerField FROM InnerTable; T.*} FROM T;")
    );
}

#[test]
fn appending_after_a_trailing_line_comment_cannot_swallow_the_new_separator() {
    let mut value = constructor("SELECT A // keep\nFROM T");
    apply(
        &mut value,
        &Action::Append {
            statement: 0,
            clause: ClauseKind::Fields,
            text: "B".to_owned(),
            join: JoinKind::Inner,
        },
    );
    assert_eq!(value.model().statements[0].clauses[0].items.len(), 2);
    assert_eq!(
        value.model().statements[0].clauses[1].kind,
        ClauseKind::Sources
    );
    assert!(value.text().contains("// keep\n,\nB\nFROM T"));
}

#[test]
fn empty_statement_inventory_is_scanned_once_and_remains_an_exact_no_op() {
    let text = ";".repeat(65_536);
    let value = constructor(&text);
    assert!(value.model().statements.is_empty());
    assert!(value.finish().expect("empty statements").is_empty());
}

#[test]
fn oversized_visual_lists_remain_opaque_without_quadratic_item_scans() {
    let text = format!("SELECT {} FROM T", vec!["A"; 4096].join(","));
    let value = constructor(&text);
    assert_eq!(
        value.model().statements[0].unavailable,
        Some(Unavailable::ModelLimit)
    );
    assert!(value.finish().expect("large opaque list").is_empty());
}
