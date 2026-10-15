use super::*;
use eska::project_core::metadata_parser::{self, PropertiesMode};

/// Native XML passes through the shared descriptor parser before sequence projection.
fn converted(properties: &str, children: &str) -> Result<Sequence, AdapterError> {
    let text = format!(
        "<MetaDataObject xmlns='http://v8.1c.ru/8.3/MDClasses' xmlns:r='http://v8.1c.ru/8.3/xcf/readable' xmlns:v8='http://v8.1c.ru/8.1/data/core' xmlns:xs='http://www.w3.org/2001/XMLSchema' version='2.20'><Sequence uuid='70000000-0000-4000-8000-000000000001'><Properties><Name>Chain</Name>{properties}</Properties><ChildObjects>{children}</ChildObjects></Sequence></MetaDataObject>"
    );
    sequence(&metadata_parser::parse(&text, None, PropertiesMode::All).unwrap())
}

/// Own dimensions use actual UUIDs and the same native `TypeDescription` as other typed fields.
#[test]
fn sequence_preserves_typed_dimensions_and_relations() {
    let value=converted("<Documents><r:Item>Document.Orders</r:Item></Documents><RegisterRecords><r:Item>InformationRegister.State</r:Item><r:Item>AccumulationRegister.Stock</r:Item></RegisterRecords>","<Dimension uuid='80000000-0000-4000-8000-000000000001'><Properties><Name>KeyЯ</Name><Type><v8:Type>xs:string</v8:Type></Type></Properties></Dimension>").unwrap();
    assert_eq!(value.documents().unwrap(), &["Orders"]);
    assert_eq!(value.register_records().unwrap().len(), 2);
    assert_eq!(
        value.dimensions()[0].uuid.to_string(),
        "80000000-0000-4000-8000-000000000001"
    );
    assert_eq!(value.dimensions()[0].name, "KeyЯ");
    assert!(matches!(
        value.dimensions()[0].attribute_type,
        Some(bsl_metadata::AttributeType::String { .. })
    ));
    assert!(value.estimated_heap_size() > 0);
}

/// Inheritance preserves the original type, while a wrong borrowed dimension UUID cannot replace it.
#[test]
fn sequence_borrowed_dimension_requires_original_identity() {
    let base=converted("<Documents><r:Item>Document.Orders</r:Item></Documents><RegisterRecords/>","<Dimension uuid='80000000-0000-4000-8000-000000000001'><Properties><Name>KeyЯ</Name><Type><v8:Type>xs:string</v8:Type></Type></Properties></Dimension>").unwrap();
    let identity = "<ObjectBelonging>Adopted</ObjectBelonging><ExtendedConfigurationObject>70000000-0000-4000-8000-000000000001</ExtendedConfigurationObject>";
    let child = "<Dimension uuid='80000000-0000-4000-8000-000000000002'><Properties><Name>KeyЯ</Name><ObjectBelonging>Adopted</ObjectBelonging><ExtendedConfigurationObject>80000000-0000-4000-8000-000000000001</ExtendedConfigurationObject></Properties></Dimension>";
    let overlay = converted(identity, child).unwrap();
    assert!(overlay.adopts(&base));
    let mut merged = base.clone();
    merged.apply_extension_overlay(&overlay);
    assert_eq!(
        merged.dimensions()[0].attribute_type,
        base.dimensions()[0].attribute_type
    );
    assert_eq!(merged.documents(), base.documents());
    let wrong = child.replace(
        "<ExtendedConfigurationObject>80000000-0000-4000-8000-000000000001",
        "<ExtendedConfigurationObject>80000000-0000-4000-8000-000000000099",
    );
    assert!(!converted(identity, &wrong).unwrap().adopts(&base));
}

/// Malformed targets and ambiguous dimensions remain explicit unavailable inputs.
#[test]
fn sequence_rejects_incomplete_or_ambiguous_input() {
    for properties in [
        "",
        "<Documents/>",
        "<Documents><r:Item>Catalog.Items</r:Item></Documents><RegisterRecords/>",
        "<Documents/><RegisterRecords><r:Item>Document.Orders</r:Item></RegisterRecords>",
        "<Documents><r:Item>Document.Orders</r:Item><r:Item>Document.orders</r:Item></Documents><RegisterRecords/>",
    ] {
        assert!(converted(properties, "").is_err(), "{properties}");
    }
    for child in [
        "<Dimension uuid='80000000-0000-4000-8000-000000000001'><Properties><Name>Key</Name></Properties></Dimension>",
        "<Dimension uuid='80000000-0000-4000-8000-000000000001'><Properties><Name>Период</Name><Type/></Properties></Dimension>",
    ] {
        assert!(converted("<Documents/><RegisterRecords/>", child).is_err());
    }
}
