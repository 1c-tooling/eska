use super::*;

/// A renamed inner output propagates through implicit parent projections and their result ordering.
#[test]
fn nested_output_names_propagate_without_touching_literals_or_parameters() {
    let mut value = constructor(
        "SELECT Middle.Value FROM (SELECT Child.Value FROM (SELECT 1 AS Value) AS Child) AS Middle WHERE Middle.Value <> &Value ORDER BY Value",
    );
    apply(
        &mut value,
        &Action::Replace {
            selection: Selection {
                statement: 2,
                clause: 0,
                item: 0,
            },
            text: "1 AS Updated".into(),
        },
    );
    assert_eq!(
        value.text(),
        "SELECT Middle.Updated FROM (SELECT Child.Updated FROM (SELECT 1 AS Updated) AS Child) AS Middle WHERE Middle.Updated <> &Value ORDER BY Updated"
    );
}

/// A child SELECT addresses its own fields while all surrounding bytes remain exact.
#[test]
fn nested_scopes_edit_only_the_selected_query_and_preserve_the_package() {
    let original = "SELECT Q.Ref FROM (\r\n SELECT I.Ref FROM Catalog.Items AS I WHERE I.Code = &Code\r\n) AS Q WHERE Q.Ref IN (SELECT X.Ref FROM Catalog.Other AS X);\r\nDROP Temp";
    let mut value = constructor(original);
    assert_eq!(value.model().scopes.len(), 4);
    assert_eq!(value.model().statements.len(), 4);
    assert_eq!(value.model().scopes[1].parent, Some(0));
    assert_eq!(value.model().scopes[2].parent, Some(0));
    apply(
        &mut value,
        &Action::Append {
            statement: 1,
            clause: ClauseKind::Fields,
            text: "I.Code AS Code".into(),
            join: JoinKind::Inner,
        },
    );
    assert!(value.text().starts_with(
        "SELECT Q.Ref FROM (\r\n SELECT I.Ref,\r\nI.Code AS Code FROM Catalog.Items AS I"
    ));
    assert!(
        value.text().ends_with(
            ") AS Q WHERE Q.Ref IN (SELECT X.Ref FROM Catalog.Other AS X);\r\nDROP Temp"
        )
    );
}

/// Parent alias edits retain inner comments and cannot rename a shadowed child source.
#[test]
fn nested_alias_edits_preserve_inner_text_and_respect_scope_boundaries() {
    let inner = " SELECT Q.Ref // inner 😀\r\n FROM Catalog.Items AS Q WHERE Q.Code = &Code ";
    let original = format!(
        "SELECT Q.Ref FROM ({inner}) AS Q WHERE Q.Ref IN (SELECT Q.Ref FROM Catalog.Other AS Q)"
    );
    let mut value = constructor(&original);
    apply(
        &mut value,
        &Action::Compose {
            statement: 0,
            selection: Some(selection(1, 0)),
            expression: Expression::SubquerySource {
                alias: "Parent".into(),
            },
        },
    );
    assert_eq!(
        value.text(),
        format!(
            "SELECT Parent.Ref FROM ({inner}) AS Parent WHERE Parent.Ref IN (SELECT Q.Ref FROM Catalog.Other AS Q)"
        )
    );
    let clause = &value.model().statements[0].clauses[1];
    let projected = clause.items[0].visual(value.text(), clause).unwrap();
    let before = value.text().to_owned();
    apply(
        &mut value,
        &Action::Compose {
            statement: 0,
            selection: Some(selection(1, 0)),
            expression: projected,
        },
    );
    assert_eq!(value.text(), before);
}

/// A new source is created by ESKA and immediately exposes a child scope for the same visual operations.
#[test]
fn nested_sources_are_created_without_client_generated_sql() {
    for (query, alias, text) in [
        ("SELECT 1", "NestedQuery", "SELECT 1 AS Value"),
        ("ВЫБРАТЬ 1", "ВложенныйЗапрос", "ВЫБРАТЬ 1 КАК Значение"),
    ] {
        let mut value = constructor(query);
        apply(
            &mut value,
            &Action::Compose {
                statement: 0,
                selection: None,
                expression: Expression::SubquerySource {
                    alias: String::new(),
                },
            },
        );
        assert_eq!(value.model().scopes.len(), 2);
        assert!(value.text().contains(text));
        let source = &value.model().statements[0].source_bindings(value.text())[0];
        assert!(source.derived);
        assert_eq!(source.alias, alias);
        assert_eq!(value.model().source_scope(selection(1, 0)), Some(1));
    }
}

/// Joined subqueries retain their body while aliases, link operands and join headers change atomically.
#[test]
fn joined_nested_source_changes_only_its_outer_controls() {
    let body = "SELECT Ref // keep\nFROM Catalog.Items";
    let mut value = constructor(&format!(
        "SELECT Q.Ref FROM Catalog.Other AS O LEFT JOIN ({body}) AS Q ON O.Ref = Q.Ref"
    ));
    apply(
        &mut value,
        &Action::Compose {
            statement: 0,
            selection: Some(selection(2, 0)),
            expression: Expression::SubqueryJoin {
                alias: "Nested".into(),
                join: JoinKind::Inner,
                left: "O.Ref".into(),
                comparison: Comparison::Equal,
                right: "Q.Ref".into(),
            },
        },
    );
    assert!(value.text().contains(&format!(
        "INNER JOIN ({body}) AS Nested ON O.Ref = Nested.Ref"
    )));
    assert!(value.text().starts_with("SELECT Nested.Ref"));
}
