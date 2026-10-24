use super::*;

/// A derived table exposes its own first projection and expands qualified wildcards using the same application metadata.
#[test]
fn nested_catalog_projects_aliases_and_wildcards_without_client_parsing() {
    use crate::backend::project::query::constructor::{Constructor, Selection};
    let (_directory, workspace, _, _) = setup();
    let constructor = Constructor::new("SELECT Q.Ref FROM (SELECT I.*, I.ИНН AS Tax FROM Catalog.Контрагенты AS I UNION ALL SELECT X.*, X.ИНН AS Other FROM Catalog.Контрагенты AS X) AS Q".into()).unwrap();
    let page = workspace.projects()[0]
        .query_constructor_catalog(
            &constructor,
            None,
            &QueryCatalogRequest {
                source: Some(Selection {
                    statement: 0,
                    clause: 1,
                    item: 0,
                }),
                qualifier: Some("Q".into()),
                ..Default::default()
            },
        )
        .unwrap();
    assert!(page.fields.iter().any(|field| field.english == "Q.Ref"));
    assert!(page.fields.iter().any(|field| field.english == "Q.Tax"));
    assert!(!page.fields.iter().any(|field| field.english == "Q.Other"));
}

/// Virtual choices depend on the register type and period setting rather than a universal client inventory.
#[test]
fn virtual_catalog_uses_real_register_availability_resources_and_parameter_positions() {
    let (directory, workspace, _, _) = setup();
    let path = directory.0.join("src/AccumulationRegisters/Остатки.xml");
    let descriptor = "<MetaDataObject xmlns='http://v8.1c.ru/8.3/MDClasses'><AccumulationRegister uuid='11111111-1111-1111-1111-111111111111'><Properties><Name>Остатки</Name><RegisterType>Balance</RegisterType></Properties><ChildObjects><Dimension uuid='22222222-2222-2222-2222-222222222222'><Properties><Name>Товар</Name></Properties></Dimension><Resource uuid='33333333-3333-3333-3333-333333333333'><Properties><Name>Количество</Name></Properties></Resource></ChildObjects></AccumulationRegister></MetaDataObject>";
    fs::write(&path, descriptor).unwrap();
    let before = bytes(&directory.0);
    let project = &workspace.projects()[0];
    let page = project
        .query_catalog(
            None,
            &QueryCatalogRequest {
                table: Some("AccumulationRegister.Остатки".into()),
                virtual_tables: true,
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(page.tables.len(), 3);
    let fields = project
        .query_catalog(
            None,
            &QueryCatalogRequest {
                table: Some("AccumulationRegister.Остатки.BalanceAndTurnovers".into()),
                qualifier: Some("О".into()),
                virtual_tables: true,
                ..Default::default()
            },
        )
        .unwrap();
    assert!(fields.found);
    assert_eq!(
        fields
            .parameters
            .iter()
            .map(|value| value.key)
            .collect::<Vec<_>>(),
        [
            "beginPeriod",
            "endPeriod",
            "periodicity",
            "complementMethod",
            "condition"
        ]
    );
    assert!(
        fields
            .fields
            .iter()
            .any(|field| field.english == "О.КоличествоOpeningBalance")
    );
    assert!(
        fields
            .fields
            .iter()
            .any(|field| field.english == "О.КоличествоTurnover")
    );
    assert!(
        !fields
            .fields
            .iter()
            .any(|field| field.english == "О.Количество")
    );
    assert_eq!(bytes(&directory.0), before);
    fs::write(&path, descriptor.replace(">Balance<", ">Turnovers<")).unwrap();
    let page = project
        .query_catalog(
            None,
            &QueryCatalogRequest {
                table: Some("AccumulationRegister.Остатки".into()),
                virtual_tables: true,
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(page.tables.len(), 1);
    let missing = project
        .query_catalog(
            None,
            &QueryCatalogRequest {
                table: Some("AccumulationRegister.Остатки.Balance".into()),
                virtual_tables: true,
                ..Default::default()
            },
        )
        .unwrap();
    assert!(!missing.found);
}
