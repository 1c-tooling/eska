//! Standard values cannot infer their type from arbitrary saved XML annotations.

use super::*;

/// Inspect a single standard property with its actual metadata owner and namespace chain.
fn choices(class: &str, name: &str, properties: &str) -> Option<Vec<EditableValueType>> {
    let xml = format!(
        "<{class} xmlns='{MD}' xmlns:r='{READABLE}'><Properties><Name>Owner</Name>{properties}<StandardAttributes><r:StandardAttribute name='{name}'><r:FillValue/></r:StandardAttribute></StandardAttributes></Properties></{class}>"
    );
    let document = Document::parse(&xml).unwrap();
    value_types(
        document
            .descendants()
            .find(|node| node.has_tag_name((READABLE, "FillValue")))
            .unwrap(),
    )
}

/// Codes and numbers inherit both precision and sign from EDT's `MdTypeUtil` factory.
#[test]
fn code_domains_follow_owner_qualifiers() {
    for (class, name) in [
        ("Catalog", "Code"),
        ("ChartOfCalculationTypes", "Code"),
        ("Document", "Number"),
        ("Task", "Number"),
        ("BusinessProcess", "Number"),
    ] {
        let values = choices(
            class,
            name,
            &format!("<{name}Type>Number</{name}Type><{name}Length>3</{name}Length>"),
        )
        .unwrap();
        assert_eq!(values[0].key.name, "decimal");
        assert!(values[0].validate("123").is_ok());
        for bad in ["-1", "1000", "1.5"] {
            assert!(values[0].validate(bad).is_err());
        }
        let strings = choices(
            class,
            name,
            &format!("<{name}Type>String</{name}Type><{name}Length>3</{name}Length>"),
        )
        .unwrap();
        assert!(strings[0].validate("ABC").is_ok());
        assert!(strings[0].validate("ABCD").is_err());
    }
}

/// Merely having a `FillValue` element does not make system-managed standard attributes editable.
#[test]
fn unsupported_standard_fields_and_disabled_lengths_stay_read_only() {
    for (class, name, properties) in [
        ("Catalog", "Ref", ""),
        ("Catalog", "Predefined", ""),
        ("Catalog", "IsFolder", ""),
        ("Enum", "Order", ""),
        ("DocumentJournal", "Date", ""),
        ("AccumulationRegister", "Active", ""),
        ("TabularSection", "LineNumber", ""),
        (
            "Catalog",
            "Code",
            "<CodeType>String</CodeType><CodeLength>0</CodeLength>",
        ),
        ("Catalog", "Code", "<CodeLength>3</CodeLength>"),
        (
            "Catalog",
            "Description",
            "<DescriptionLength>0</DescriptionLength>",
        ),
    ] {
        assert!(choices(class, name, properties).is_none(), "{class}.{name}");
    }
}

/// Implicit types still use the existing primitive validators and concrete reference selectors.
#[test]
fn primitive_and_parent_domains_are_contextual() {
    let values = choices(
        "Catalog",
        "Description",
        "<DescriptionLength>4</DescriptionLength>",
    )
    .unwrap();
    assert!(values[0].validate("Тест").is_ok());
    assert!(values[0].validate("Длинно").is_err());
    for (class, name) in [
        ("Catalog", "DeletionMark"),
        ("Document", "Posted"),
        ("Task", "Executed"),
        ("BusinessProcess", "Started"),
        ("BusinessProcess", "Completed"),
    ] {
        assert_eq!(choices(class, name, "").unwrap()[0].key.name, "boolean");
    }
    assert_eq!(
        choices("Document", "Date", "").unwrap()[0].key.name,
        "dateTime"
    );
    assert!(choices("Catalog", "Parent", "<Hierarchical>false</Hierarchical>").is_none());
    assert_eq!(
        choices("Catalog", "Parent", "<Hierarchical>true</Hierarchical>").unwrap()[0]
            .key
            .name,
        "CatalogRef.Owner"
    );
    assert_eq!(
        choices("Catalog", "Parent", "<Hierarchical>1</Hierarchical>").unwrap()[0]
            .key
            .name,
        "CatalogRef.Owner"
    );
    assert_eq!(
        choices("ChartOfAccounts", "Parent", "").unwrap()[0]
            .key
            .name,
        "ChartOfAccountsRef.Owner"
    );
}

/// An explicit-looking Type child must not turn an unsupported standard field into an ordinary attribute.
#[test]
fn nested_type_cannot_override_a_standard_attribute_domain() {
    let xml = format!(
        "<Catalog xmlns='{MD}' xmlns:r='{READABLE}' xmlns:v='{}' xmlns:x='{XS}' xmlns:s='{}'><Properties><Name>Owner</Name><StandardAttributes><r:StandardAttribute name='Ref'><Type><v:Type>x:string</v:Type></Type><r:FillValue s:nil='true'/></r:StandardAttribute></StandardAttributes></Properties></Catalog>",
        super::super::schema::CORE,
        super::super::schema::XSI
    );
    let document = Document::parse(&xml).unwrap();
    let value = document
        .descendants()
        .find(|node| node.has_tag_name((READABLE, "FillValue")))
        .unwrap();
    assert!(value_schema::schema(value).is_none());
}
