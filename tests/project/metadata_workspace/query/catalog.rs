use super::*;
use crate::backend::project::metadata_workspace::QueryCatalogRequest;

mod overlays;
mod virtuals;

/// Correspondence columns use the actual debit/credit layout rather than inventing a shared Account or unbalanced field.
#[test]
fn query_catalog_distinguishes_balanced_and_correspondence_accounting_columns() {
    let (directory, workspace, _, _) = setup();
    fs::write(directory.0.join("src/AccountingRegisters/Проводки.xml"), "<MetaDataObject xmlns='http://v8.1c.ru/8.3/MDClasses'><AccountingRegister uuid='11111111-1111-1111-1111-111111111111'><Properties><Name>Проводки</Name><Correspondence>true</Correspondence><ChartOfAccounts>ChartOfAccounts.Accounts</ChartOfAccounts></Properties><ChildObjects><Dimension uuid='22222222-2222-2222-2222-222222222222'><Properties><Name>Организация</Name><Balance>true</Balance></Properties></Dimension><Dimension uuid='33333333-3333-3333-3333-333333333333'><Properties><Name>Валюта</Name><Balance>false</Balance></Properties></Dimension><Resource uuid='44444444-4444-4444-4444-444444444444'><Properties><Name>Количество</Name><Balance>false</Balance></Properties></Resource></ChildObjects></AccountingRegister></MetaDataObject>").unwrap();
    let page = workspace.projects()[0]
        .query_catalog(
            None,
            &QueryCatalogRequest {
                table: Some("AccountingRegister.Проводки".into()),
                ..Default::default()
            },
        )
        .unwrap();
    let names: Vec<_> = page
        .fields
        .iter()
        .map(|field| field.english.as_str())
        .collect();
    for name in [
        "Организация",
        "ВалютаDr",
        "ВалютаCr",
        "КоличествоDr",
        "КоличествоCr",
        "AccountDr",
        "AccountCr",
    ] {
        assert!(names.contains(&name), "{name}");
    }
    for name in ["Валюта", "Количество", "Account", "ExtDimensionDr1"] {
        assert!(!names.contains(&name), "{name}");
    }
}

/// Root choices are paged from declarations and do not open an unrelated missing descriptor or write source/cache files.
#[test]
fn query_catalog_lists_real_tables_lazily_and_preserves_every_source_byte() {
    let (directory, workspace, _, _) = setup();
    let before = bytes(&directory.0);
    let project = &workspace.projects()[0];
    let first = project
        .query_catalog(
            None,
            &QueryCatalogRequest {
                limit: Some(1),
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(first.tables.len(), 1);
    assert_eq!(first.next_offset, Some(1));
    let second = project
        .query_catalog(
            None,
            &QueryCatalogRequest {
                offset: 1,
                limit: Some(1),
                ..Default::default()
            },
        )
        .unwrap();
    assert_ne!(first.tables[0].english, second.tables[0].english);
    let found = project
        .query_catalog(
            None,
            &QueryCatalogRequest {
                search: "КОНТРА".into(),
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(found.tables.len(), 1);
    assert_eq!(found.tables[0].russian, "Справочник.Контрагенты");
    assert_eq!(bytes(&directory.0), before);
}

/// Standard fields, custom attributes and tabular section sources share core-produced qualification in either query language.
#[test]
fn query_catalog_qualifies_actual_fields_and_reads_only_the_requested_descriptor() {
    let (directory, workspace, _, _) = setup();
    let before = bytes(&directory.0);
    let project = &workspace.projects()[0];
    let fields = project
        .query_catalog(
            None,
            &QueryCatalogRequest {
                table: Some("Справочник.Контрагенты".into()),
                qualifier: Some("К".into()),
                ..Default::default()
            },
        )
        .unwrap();
    assert!(fields.found);
    assert!(fields.fields.iter().any(|field| field.russian == "К.ИНН"));
    assert!(fields.fields.iter().any(|field| field.english == "К.Ref"));
    assert!(
        fields
            .tables
            .iter()
            .any(|table| table.english == "Catalog.Контрагенты.Контакты")
    );
    let tabular = project
        .query_catalog(
            None,
            &QueryCatalogRequest {
                table: Some("Catalog.Контрагенты.Контакты".into()),
                ..Default::default()
            },
        )
        .unwrap();
    assert!(
        tabular
            .fields
            .iter()
            .any(|field| field.russian == "Телефон")
    );
    assert!(!tabular.fields.iter().any(|field| field.russian == "ИНН"));
    assert_eq!(bytes(&directory.0), before);
}

/// A failed descriptor is never represented as a successful empty metadata response.
#[test]
fn query_catalog_distinguishes_unknown_tables_missing_descriptors_and_invalid_qualification() {
    let (_directory, workspace, _, _) = setup();
    let project = &workspace.projects()[0];
    let unknown = project
        .query_catalog(
            None,
            &QueryCatalogRequest {
                table: Some("Catalog.Unknown".into()),
                ..Default::default()
            },
        )
        .unwrap();
    assert!(!unknown.found);
    assert!(
        project
            .query_catalog(
                None,
                &QueryCatalogRequest {
                    table: Some("Catalog.БезФайла".into()),
                    ..Default::default()
                }
            )
            .is_err()
    );
    assert!(
        project
            .query_catalog(
                None,
                &QueryCatalogRequest {
                    qualifier: Some("Bad..Name".into()),
                    ..Default::default()
                }
            )
            .is_err()
    );
}
