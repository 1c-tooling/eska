use super::*;

/// Creation/destruction lifetimes, nested visibility and later recreation are independent of query spelling.
#[test]
fn temporary_tables_follow_package_order_and_new_lifetimes() {
    for input in [
        "SELECT 1 AS A INTO Temp; SELECT X.A FROM Temp AS X; DESTROY Temp; SELECT 2 AS B INTO Temp; SELECT X.B FROM Temp AS X",
        "ВЫБРАТЬ 1 КАК A ПОМЕСТИТЬ Temp; ВЫБРАТЬ X.A ИЗ Temp КАК X; УНИЧТОЖИТЬ Temp; ВЫБРАТЬ 2 КАК B ПОМЕСТИТЬ Temp; ВЫБРАТЬ X.B ИЗ Temp КАК X",
    ] {
        let value = constructor(input);
        let model = value.model();
        assert!(model.statements.iter().all(|s| s.unavailable.is_none()));
        assert!(model.temporary_tables_at(input, 0).unwrap().is_empty());
        assert_eq!(
            model
                .temporary_source(input, 1, "TEMP")
                .unwrap()
                .unwrap()
                .statement,
            0
        );
        assert!(model.temporary_tables_at(input, 3).unwrap().is_empty());
        assert_eq!(
            model
                .temporary_source(input, 4, "Temp")
                .unwrap()
                .unwrap()
                .statement,
            3
        );
        assert!(model.package_issues(input).is_empty());
        assert!(value.finish().unwrap().is_empty());
    }
}

/// Renaming one creation updates its qualified references and drop, excluding aliases, parameters and the next lifetime.
#[test]
fn temporary_rename_is_atomic_scoped_and_comment_preserving() {
    let original = "SELECT 1 AS A INTO Temp; SELECT Temp.A, \"Temp.A\" AS Text, &Temp.A AS Parameter FROM Temp; SELECT X.A FROM Temp AS X; SELECT Temp.A FROM Temp AS Temp; DESTROY Temp; SELECT 2 AS B INTO Temp; SELECT Temp.B FROM Temp";
    let mut value = constructor(original);
    apply(
        &mut value,
        &Action::SetInto {
            statement: 0,
            name: Some("Renamed".into()),
            comments: None,
        },
    );
    assert!(
        value
            .text()
            .contains("SELECT Renamed.A, \"Temp.A\" AS Text, &Temp.A AS Parameter FROM Renamed")
    );
    assert!(value.text().contains(
        "SELECT X.A FROM Renamed AS X; SELECT Temp.A FROM Renamed AS Temp; DESTROY Renamed"
    ));
    assert!(
        value
            .text()
            .ends_with("SELECT 2 AS B INTO Temp; SELECT Temp.B FROM Temp")
    );
    let before = value.text().to_owned();
    let revision = value.revision();
    assert_eq!(
        value
            .apply(
                revision,
                &Action::SetInto {
                    statement: 0,
                    name: None,
                    comments: None
                }
            )
            .unwrap_err(),
        Error::InvalidExpression
    );
    assert_eq!(value.text(), before);
    assert_eq!(value.revision(), revision);
    assert_eq!(
        value
            .apply(
                revision,
                &Action::SetInto {
                    statement: 0,
                    name: Some("Bad; SELECT 9".into()),
                    comments: None
                }
            )
            .unwrap_err(),
        Error::InvalidExpression
    );
}

/// Removing/reordering producers or inserting an early drop cannot strand existing consumers.
#[test]
fn package_dependency_failures_leave_text_revision_and_focus_unchanged() {
    let original = "SELECT 1 AS A INTO Temp; SELECT Temp.A FROM Temp; SELECT 9 AS B";
    for action in [
        Action::RemoveStatement {
            statement: 0,
            comments: None,
        },
        Action::MoveStatement {
            statement: 0,
            to: 1,
        },
        Action::AddStatement {
            statement: 0,
            operation: PackageOperation::Destroy,
            name: Some("Temp".into()),
        },
    ] {
        let mut value = constructor(original);
        assert_eq!(
            value.apply(1, &action).unwrap_err(),
            Error::InvalidExpression
        );
        assert_eq!(value.text(), original);
        assert_eq!(value.revision(), 1);
        assert_eq!(value.focus(), Some(0));
    }
    let mut value = constructor(original);
    apply(
        &mut value,
        &Action::AddStatement {
            statement: 1,
            operation: PackageOperation::Destroy,
            name: Some("Temp".into()),
        },
    );
    assert!(value.text().contains("DESTROY Temp"));
    assert_eq!(
        value.model().package_operation(value.focus().unwrap()),
        Some(PackageOperation::Destroy)
    );
    let selected = value.focus().unwrap();
    apply(
        &mut value,
        &Action::RemoveStatement {
            statement: selected,
            comments: None,
        },
    );
    assert_eq!(value.text(), original);
}

/// UNION cloning never duplicates INTO; deleting its first branch retains the destination and projected field names.
#[test]
fn temporary_union_destination_survives_branch_copy_and_removal() {
    let mut value = constructor("SELECT 1 AS A INTO Temp; SELECT Temp.A FROM Temp");
    apply(
        &mut value,
        &Action::AddUnion {
            statement: 0,
            all: true,
        },
    );
    assert_eq!(value.text().matches("INTO Temp").count(), 1);
    assert!(
        value
            .model()
            .statements
            .iter()
            .all(|s| s.unavailable.is_none())
    );
    apply(
        &mut value,
        &Action::RemoveUnion {
            statement: 0,
            comments: None,
        },
    );
    assert_eq!(value.text().matches("INTO Temp").count(), 1);
    assert!(value.model().package_issues(value.text()).is_empty());
    assert_eq!(
        value
            .model()
            .temporary_source(value.text(), 1, "Temp")
            .unwrap()
            .unwrap()
            .statement,
        0
    );
}

/// Diagnostics distinguish an unavailable lifetime, duplicate creation and a proven missing output column.
#[test]
fn package_reports_dependencies_without_inventing_metadata_or_wildcard_columns() {
    let value = constructor(
        "SELECT Temp.A FROM Temp; SELECT 1 AS A INTO Temp; SELECT X.B FROM Temp AS X; SELECT 2 AS A INTO Temp; DESTROY Unknown",
    );
    let issues = value.model().package_issues(value.text());
    for kind in [
        PackageIssueKind::TemporaryMissing,
        PackageIssueKind::TemporaryDuplicate,
        PackageIssueKind::TemporaryColumn,
    ] {
        assert!(issues.iter().any(|issue| issue.code == kind), "{kind:?}");
    }
    let mut value = constructor("SELECT 1 AS A INTO Temp; SELECT X.A FROM Temp AS X");
    assert_eq!(
        value
            .apply(
                1,
                &Action::Remove {
                    selection: selection(0, 0),
                    comments: None
                }
            )
            .unwrap_err(),
        Error::RequiredItem
    );
    assert_eq!(
        value
            .apply(
                1,
                &Action::Replace {
                    selection: selection(0, 0),
                    text: "1 AS B".into()
                }
            )
            .unwrap_err(),
        Error::InvalidExpression
    );
    let wildcard =
        constructor("SELECT S.* INTO Temp FROM Catalog.Items AS S; SELECT Temp.Anything FROM Temp");
    assert!(wildcard.model().package_issues(wildcard.text()).is_empty());
    let ambiguous = constructor("SELECT 1 AS A, 2 AS A INTO Temp; SELECT Temp.A FROM Temp");
    assert!(
        ambiguous
            .model()
            .package_issues(ambiguous.text())
            .iter()
            .any(|issue| issue.code == PackageIssueKind::TemporaryAmbiguousColumns)
    );
    assert!(
        ambiguous
            .model()
            .temporary_tables_at(ambiguous.text(), 1)
            .unwrap()
            .is_empty()
    );
}

/// Package comments require a choice; INDEX SETS/UNIQUE remain exact opaque text until explicitly supported.
#[test]
fn package_controls_keep_comments_and_reject_unknown_index_shapes() {
    let mut value = constructor("SELECT 1 AS A; // keep\r\nSELECT 2 AS B; SELECT 3 AS C");
    assert_eq!(
        value
            .apply(
                1,
                &Action::RemoveStatement {
                    statement: 1,
                    comments: None
                }
            )
            .unwrap_err(),
        Error::CommentsRequireDecision
    );
    apply(
        &mut value,
        &Action::RemoveStatement {
            statement: 1,
            comments: Some(CommentDecision::Keep),
        },
    );
    assert!(value.text().contains("// keep\r\n"));
    let indexed = constructor("SELECT 1 AS A INTO Temp INDEX BY A");
    assert!(indexed.model().statements[0].unavailable.is_none());
    for input in [
        "SELECT 1 AS A INTO Temp INDEX BY SETS ((A))",
        "SELECT 1 AS A INTO Temp INDEX BY A UNIQUE",
        "SELECT (SELECT 1 INTO Bad) AS A",
    ] {
        let value = constructor(input);
        assert!(
            value
                .model()
                .statements
                .iter()
                .any(|s| s.unavailable.is_some())
        );
        assert!(value.finish().unwrap().is_empty());
    }
}
