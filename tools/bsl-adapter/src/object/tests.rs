//! Native controls cover object fields while a known whitespace-caption bug is checked separately.

use super::*;
use bsl_metadata::AttributeType;
use bsl_metadata::xml_parser;
use eska::project_core::metadata_parser::{self, PropertiesMode};

/// The same native input feeds the shared property model and pinned reference parser independently.
fn xml(kind: &str, properties: &str, children: &str) -> String {
    format!(
        r#"<MetaDataObject xmlns="http://v8.1c.ru/8.3/MDClasses" xmlns:v8="http://v8.1c.ru/8.1/data/core" xmlns:xs="http://www.w3.org/2001/XMLSchema" xmlns:cfg="http://v8.1c.ru/8.1/data/enterprise/current-config" xmlns:xr="http://v8.1c.ru/8.3/xcf/readable" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance" version="2.20"><{kind} uuid="10000000-0000-4000-8000-000000000001"><Properties><Name>Items</Name>{properties}</Properties><ChildObjects>{children}</ChildObjects></{kind}></MetaDataObject>"#
    )
}

/// Convert only a fully loaded, issue-free descriptor.
fn model(text: &str) -> Result<MetadataObject, AdapterError> {
    metadata_object(&metadata_parser::parse(text, None, PropertiesMode::All).unwrap())
}

/// Numeric/string codes, standard-field conditions, owner unions and real user types match the core.
#[test]
fn catalogs_match_native_standard_and_user_fields() {
    let children = r#"<Attribute uuid="20000000-0000-4000-8000-000000000001"><Properties><Name>Active</Name><Type><v8:Type>xs:boolean</v8:Type></Type></Properties></Attribute><TabularSection uuid="30000000-0000-4000-8000-000000000001"><Properties><Name>Rows</Name><Use>ForItemAndFolder</Use></Properties><ChildObjects><Attribute uuid="40000000-0000-4000-8000-000000000001"><Properties><Name>Owner</Name><Type><v8:Type>cfg:CatalogRef.Owners</v8:Type></Type></Properties></Attribute></ChildObjects></TabularSection>"#;
    for properties in [
        "",
        "<CodeLength>9</CodeLength><CodeType>Number</CodeType><DescriptionLength>150</DescriptionLength><Hierarchical>true</Hierarchical><CheckUnique>true</CheckUnique><CodeSeries>WithinSubordination</CodeSeries>",
        "<CodeLength>0</CodeLength><DescriptionLength>0</DescriptionLength><Owners><xr:Item xsi:type=\"xr:MDObjectRef\">Catalog.Owners</xr:Item><xr:Item xsi:type=\"xr:MDObjectRef\">Catalog.OtherOwners</xr:Item></Owners><CodeSeries>WithinOwner</CodeSeries>",
        "<CodeLength>4294967295</CodeLength><CodeType>Number</CodeType><ObjectBelonging>Adopted</ObjectBelonging><ExtendedConfigurationObject>50000000-0000-4000-8000-000000000001</ExtendedConfigurationObject>",
    ] {
        let text = xml("Catalog", properties, children);
        assert_eq!(
            model(&text).unwrap(),
            xml_parser::parse_catalog_xml(&text).unwrap()
        );
    }
}

/// Document register targets and numeric numbers remain ordered; processors/reports share field rules.
#[test]
fn documents_processors_and_reports_match_native_models() {
    let children = r#"<Attribute uuid="20000000-0000-4000-8000-000000000001"><Properties><Name>Value</Name><Type><v8:Type>xs:decimal</v8:Type></Type></Properties></Attribute>"#;
    let text = xml(
        "Document",
        "<NumberLength>12</NumberLength><NumberType>Number</NumberType><RegisterRecords><xr:Item xsi:type=\"xr:MDObjectRef\">AccumulationRegister.Balance</xr:Item><xr:Item xsi:type=\"xr:MDObjectRef\">InformationRegister.Rates</xr:Item></RegisterRecords>",
        children,
    );
    assert_eq!(
        model(&text).unwrap(),
        xml_parser::parse_document_xml(&text).unwrap()
    );
    let text = xml("DataProcessor", "", children);
    assert_eq!(
        model(&text).unwrap(),
        xml_parser::parse_data_processor_xml(&text).unwrap()
    );
    let text = xml("Report", "", children);
    assert_eq!(
        model(&text).unwrap(),
        xml_parser::parse_report_xml(&text).unwrap()
    );
}

/// The single kernel caption uses actual Russian/first text and is independent of XML whitespace.
#[test]
fn tabular_captions_are_real_localized_values_not_native_whitespace() {
    let children = r#"<TabularSection uuid="30000000-0000-4000-8000-000000000001"><Properties><Name>Rows</Name><Synonym>
    <v8:item><v8:lang>en</v8:lang><v8:content>Lines</v8:content></v8:item>
    <v8:item><v8:lang>ru</v8:lang><v8:content>Строки</v8:content></v8:item>
</Synonym></Properties></TabularSection>"#;
    let text = xml("Catalog", "", children);
    let converted = model(&text).unwrap();
    assert_eq!(converted.tabular_sections[0].synonym(), Some("Строки"));
    let reference = xml_parser::parse_catalog_xml(&text).unwrap();
    assert_eq!(reference.tabular_sections[0].synonym(), Some("\n    "));
    let compact = text
        .replace("\n    ", "")
        .replace("\n</Synonym>", "</Synonym>");
    assert_eq!(model(&compact).unwrap(), converted);
    let english = text.replace(
        "<v8:item><v8:lang>ru</v8:lang><v8:content>Строки</v8:content></v8:item>",
        "",
    );
    assert_eq!(
        model(&english).unwrap().tabular_sections[0].synonym(),
        Some("Lines")
    );
}

/// Invalid modes/lengths, references and missing types cannot produce an apparently complete object.
#[test]
fn malformed_settings_references_and_field_types_fail_at_their_source() {
    for properties in [
        "<CodeLength>-1</CodeLength>",
        "<NumberLength>4294967296</NumberLength>",
        "<CodeType>Future</CodeType>",
        "<Hierarchical>TRUE</Hierarchical>",
        "<CodeSeries>Future</CodeSeries>",
        "<Owners><xr:Item>Future.Owner</xr:Item></Owners>",
        "<Owners><xr:Item>Catalog.</xr:Item></Owners>",
        "<Owners><xr:Future>Catalog.Owner</xr:Future></Owners>",
        "<RegisterRecords><xr:Item>Catalog.Items</xr:Item></RegisterRecords>",
    ] {
        let text = xml("Document", properties, "");
        let error = model(&text).unwrap_err();
        assert!(matches!(error.issue, AdapterIssue::InvalidProperty { .. }));
        assert!(text[error.range].contains(properties));
    }
    let text = xml(
        "Catalog",
        "",
        r#"<Attribute uuid="20000000-0000-4000-8000-000000000001"><Properties><Name>Missing</Name></Properties></Attribute>"#,
    );
    assert_eq!(
        model(&text).unwrap_err().issue,
        AdapterIssue::MissingProperty {
            name: "Type".into()
        }
    );
    let text = xml(
        "Catalog",
        "",
        r#"<TabularSection uuid="invalid"><Properties><Name>Rows</Name></Properties></TabularSection>"#,
    );
    assert_eq!(model(&text).unwrap_err().issue, AdapterIssue::InvalidUuid);
}

/// Foreign same-named fields do not control standard attributes; duplicate real fields fail.
#[test]
fn standard_settings_respect_semantic_namespace_and_duplicates() {
    let text = xml(
        "Catalog",
        "<other:CodeLength xmlns:other=\"urn:foreign\">invalid</other:CodeLength><CodeLength>10</CodeLength>",
        "",
    );
    let converted = model(&text).unwrap();
    assert!(converted.attributes.iter().any(|field| field.name == "Код"
        && field.attr_type == AttributeType::String { length: Some(10) }));
    let text = xml(
        "Catalog",
        "<CodeLength>10</CodeLength><CodeLength>20</CodeLength>",
        "",
    );
    assert_eq!(
        model(&text).unwrap_err().issue,
        AdapterIssue::DuplicateProperty {
            name: "CodeLength".into()
        }
    );
}

/// Summary/unsupported kinds and recoverable parse errors stay distinct from ready models.
#[test]
fn object_descriptors_require_supported_kinds_and_loaded_properties() {
    let descriptor =
        metadata_parser::parse(&xml("Catalog", "", ""), None, PropertiesMode::Summary).unwrap();
    assert_eq!(
        metadata_object(&descriptor).unwrap_err().issue,
        AdapterIssue::PropertiesNotLoaded
    );
    assert_eq!(
        model(&xml("Enum", "", "")).unwrap_err().issue,
        AdapterIssue::WrongKind {
            actual: MetadataKind::Enum
        }
    );
    assert_eq!(
        model(&xml("Catalog", "", "<Future/>")).unwrap_err().issue,
        AdapterIssue::IncompleteDescriptor
    );
}
