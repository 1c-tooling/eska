//! Primitive objects retain identity, optional types and declaration order through shared properties.

use super::*;
use bsl_metadata::{AttributeType, ObjectBelonging};
use eska::project_core::metadata_parser::{self, PropertiesMode};

/// Native XML feeds the common semantic parser and the selected native reference independently.
fn xml(kind: &str, properties: &str, children: &str) -> String {
    format!(
        r#"<MetaDataObject xmlns="http://v8.1c.ru/8.3/MDClasses" xmlns:v8="http://v8.1c.ru/8.1/data/core" xmlns:cfg="http://v8.1c.ru/8.1/data/enterprise/current-config" xmlns:xs="http://www.w3.org/2001/XMLSchema" version="2.20"><{kind} uuid="10000000-0000-4000-8000-000000000001"><Properties><Name>Value</Name>{properties}</Properties><ChildObjects>{children}</ChildObjects></{kind}></MetaDataObject>"#
    )
}

/// Absent, empty and qualified/composite constant types match the complete native model.
#[test]
fn constant_types_and_ownership_match_native_reader() {
    for properties in [
        "",
        "<Type/>",
        "<Type><v8:Type>xs:string</v8:Type><v8:StringQualifiers><v8:Length>25</v8:Length></v8:StringQualifiers></Type>",
        "<ObjectBelonging>Adopted</ObjectBelonging><ExtendedConfigurationObject>20000000-0000-4000-8000-000000000001</ExtendedConfigurationObject><Type><v8:Type>cfg:CatalogRef.Items</v8:Type><v8:TypeSet>cfg:DefinedType.Amount</v8:TypeSet></Type>",
    ] {
        let text = xml("Constant", properties, "");
        let descriptor = metadata_parser::parse(&text, None, PropertiesMode::All).unwrap();
        assert_eq!(
            constant_object(&descriptor).unwrap(),
            bsl_metadata::xml_parser::parse_constant_xml(&text).unwrap()
        );
    }
    let descriptor =
        metadata_parser::parse(&xml("Constant", "", ""), None, PropertiesMode::All).unwrap();
    assert_eq!(constant_object(&descriptor).unwrap().constant_type, None);
    let descriptor =
        metadata_parser::parse(&xml("Constant", "<Type/>", ""), None, PropertiesMode::All).unwrap();
    assert_eq!(
        constant_object(&descriptor).unwrap().constant_type,
        Some(AttributeType::Unknown)
    );
}

/// The enum value order and original UUID spelling survive independently of builder defaults.
#[test]
fn enum_values_match_native_model_in_declaration_order() {
    let children = r#"<EnumValue uuid="2ABCDEF0-0000-4000-8000-000000000001"><Properties><Name>Second</Name></Properties></EnumValue><EnumValue uuid="30000000-0000-4000-8000-000000000001"><Properties><Name>First</Name></Properties></EnumValue>"#;
    let text = xml("Enum", "<ObjectBelonging>Own</ObjectBelonging>", children);
    let descriptor = metadata_parser::parse(&text, None, PropertiesMode::All).unwrap();
    let model = enum_object(&descriptor).unwrap();
    assert_eq!(
        model,
        bsl_metadata::xml_parser::parse_enum_xml(&text).unwrap()
    );
    assert_eq!(model.object_belonging, ObjectBelonging::Own);
    assert_eq!(model.enum_values[0].name, "Second");
    assert_eq!(
        model.enum_values[0].uuid,
        "2ABCDEF0-0000-4000-8000-000000000001"
    );
}

/// Invalid child identities and base references are located failures, not discarded fields.
#[test]
fn invalid_enum_value_and_ownership_do_not_produce_partial_success() {
    let text = xml(
        "Enum",
        "",
        r#"<EnumValue uuid="invalid"><Properties><Name>Broken</Name></Properties></EnumValue>"#,
    );
    let descriptor = metadata_parser::parse(&text, None, PropertiesMode::All).unwrap();
    let error = enum_object(&descriptor).unwrap_err();
    assert_eq!(error.issue, AdapterIssue::InvalidUuid);
    assert!(text[error.range].contains("uuid=\"invalid\""));
    for fields in [
        "<ObjectBelonging>Future</ObjectBelonging>",
        "<ExtendedConfigurationObject>invalid</ExtendedConfigurationObject>",
    ] {
        let descriptor =
            metadata_parser::parse(&xml("Constant", fields, ""), None, PropertiesMode::All)
                .unwrap();
        assert!(constant_object(&descriptor).is_err());
    }
}

/// A summary or partial descriptor cannot be published as a complete primitive object.
#[test]
fn primitive_objects_reject_incomplete_and_wrong_inputs() {
    let descriptor =
        metadata_parser::parse(&xml("Constant", "", ""), None, PropertiesMode::Summary).unwrap();
    assert_eq!(
        constant_object(&descriptor).unwrap_err().issue,
        AdapterIssue::PropertiesNotLoaded
    );
    let descriptor =
        metadata_parser::parse(&xml("Enum", "", "<Future/>"), None, PropertiesMode::All).unwrap();
    assert_eq!(
        enum_object(&descriptor).unwrap_err().issue,
        AdapterIssue::IncompleteDescriptor
    );
    let descriptor =
        metadata_parser::parse(&xml("Enum", "", ""), None, PropertiesMode::All).unwrap();
    assert_eq!(
        constant_object(&descriptor).unwrap_err().issue,
        AdapterIssue::WrongKind {
            actual: MetadataKind::Enum
        }
    );
}
