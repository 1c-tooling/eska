use super::*;

/// A copied producer has its own fresh destination; existing consumers retain their original lifetime.
#[test]
fn copying_a_root_operator_preserves_nested_bodies_comments_and_original_consumers() {
    let text = "\u{feff}SELECT Q.N AS N INTO Temp FROM (SELECT 7 AS N) AS Q\r\n// producer 😀\r\n; SELECT Temp.N FROM Temp; DROP Temp";
    let mut value = constructor(text);
    apply(&mut value, &Action::CopyStatement { statement: 0 });
    assert_eq!(value.revision(), 2);
    assert_eq!(value.text().matches('\u{feff}').count(), 1);
    assert_eq!(value.text().matches("(SELECT 7 AS N)").count(), 2);
    assert_eq!(value.text().matches("// producer 😀").count(), 2);
    assert!(value.text().contains("INTO Temp2"));
    assert!(value.text().contains("SELECT Temp.N FROM Temp; DROP Temp"));
    let focus = value.focus().unwrap();
    assert_eq!(
        value
            .model()
            .temporary_destination(value.text(), value.model().statements[focus].scope)
            .as_deref(),
        Some("Temp2")
    );
    assert_eq!(value.model().package_issues(value.text()).len(), 0);
    apply(&mut value, &Action::CopyStatement { statement: 0 });
    assert!(value.text().contains("INTO Temp3"));
    let child = value
        .model()
        .scopes
        .iter()
        .find(|scope| scope.parent == Some(0))
        .unwrap()
        .branches[0];
    assert_eq!(value.statement_path(child), vec![0, child]);
}

/// Deletion and movement retain their dependency guard; copying a DROP cannot introduce a second destruction.
#[test]
fn package_copy_refuses_nested_or_drop_operators_without_any_partial_text() {
    let mut value = constructor(
        "SELECT Q.N INTO Tmp FROM (SELECT 1 AS N) AS Q; SELECT Tmp.N FROM Tmp; DROP Tmp",
    );
    let before = value.text().to_owned();
    let nested = value
        .model()
        .scopes
        .iter()
        .find(|scope| scope.parent.is_some())
        .unwrap()
        .branches[0];
    assert_eq!(
        value
            .apply(1, &Action::CopyStatement { statement: nested })
            .unwrap_err(),
        Error::Unsupported
    );
    assert_eq!(value.text(), before);
    let drop = value
        .model()
        .statements
        .iter()
        .position(|owner| {
            owner
                .clauses
                .iter()
                .any(|clause| clause.kind == ClauseKind::Destroy)
        })
        .unwrap();
    assert_eq!(
        value
            .apply(1, &Action::CopyStatement { statement: drop })
            .unwrap_err(),
        Error::Unsupported
    );
    assert_eq!(value.text(), before);
    assert_eq!(value.revision(), 1);
}
