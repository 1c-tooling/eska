use super::*;
use crate::backend::project::query::constructor::{Constructor, Selection};

/// Wildcard chains and nested consumers resolve the producing scope through the same application metadata catalog.
#[test]
fn constructor_catalog_resolves_temporary_projection_chains_and_recreation() {
    let (directory, workspace, _, _) = setup();
    let before = bytes(&directory.0);
    let constructor = Constructor::new("SELECT T.* INTO One FROM Catalog.Контрагенты AS T; SELECT O.* INTO Two FROM One AS O; SELECT Q.Ref FROM (SELECT X.* FROM Two AS X) AS Q; DESTROY One; SELECT 1 AS NewValue INTO One; SELECT N.NewValue FROM One AS N".into()).unwrap();
    let project = &workspace.projects()[0];
    let catalog = |query: QueryCatalogRequest| {
        project
            .query_constructor_catalog(&constructor, None, &query)
            .unwrap()
    };
    let visible = catalog(QueryCatalogRequest {
        at_statement: Some(1),
        ..Default::default()
    });
    assert_eq!(visible.temporary_tables.len(), 1);
    assert_eq!(visible.temporary_tables[0].name, "One");
    let chain = catalog(QueryCatalogRequest {
        source: Some(Selection {
            statement: 2,
            clause: 1,
            item: 0,
        }),
        qualifier: Some("Q".into()),
        ..Default::default()
    });
    for name in ["Q.Ref", "Q.ИНН"] {
        assert!(
            chain.fields.iter().any(|field| field.english == name),
            "{name}"
        );
    }
    let select = constructor
        .model()
        .statements
        .iter()
        .position(|s| constructor.text()[s.range.clone()].contains("SELECT N.NewValue"))
        .unwrap();
    let rebuilt = catalog(QueryCatalogRequest {
        at_statement: Some(select),
        table: Some("One".into()),
        ..Default::default()
    });
    assert_eq!(rebuilt.fields.len(), 1);
    assert_eq!(rebuilt.fields[0].english, "NewValue");
    assert_eq!(bytes(&directory.0), before);
}

/// A later table, a destroyed lifetime and an unknown name are never replaced with another projection.
#[test]
fn constructor_catalog_does_not_guess_invisible_or_unknown_temporary_tables() {
    let (_directory, workspace, _, _) = setup();
    let constructor = Constructor::new("SELECT 1 AS A INTO Temp; DESTROY Temp; SELECT X.A FROM Temp AS X; SELECT 2 AS B INTO Temp; SELECT X.B FROM Temp AS X".into()).unwrap();
    let project = &workspace.projects()[0];
    for (statement, table) in [(0, "Temp"), (2, "Temp"), (4, "Unknown")] {
        let page = project
            .query_constructor_catalog(
                &constructor,
                None,
                &QueryCatalogRequest {
                    at_statement: Some(statement),
                    table: Some(table.into()),
                    ..Default::default()
                },
            )
            .unwrap();
        assert!(!page.found);
        assert!(page.fields.is_empty());
    }
    assert!(
        project
            .query_constructor_catalog(
                &constructor,
                None,
                &QueryCatalogRequest {
                    at_statement: Some(50),
                    ..Default::default()
                }
            )
            .is_err()
    );
    assert!(
        project
            .query_constructor_catalog(
                &constructor,
                None,
                &QueryCatalogRequest {
                    at_statement: Some(0),
                    scope: Some(0),
                    ..Default::default()
                }
            )
            .is_err()
    );
}
