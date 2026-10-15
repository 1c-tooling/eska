//! Ordinary current common attributes contribute their types only to explicitly enabled targets.

use super::*;
use eska::project_core::metadata_parser::{self, PropertiesMode};

/// Real expanded names identify Content items; changing a namespace prefix does not change membership.
fn xml(content: &str, extra: &str) -> String {
    format!(
        r#"<MetaDataObject xmlns="http://v8.1c.ru/8.3/MDClasses" xmlns:r="http://v8.1c.ru/8.3/xcf/readable" xmlns:t="http://v8.1c.ru/8.1/data/core" xmlns:x="http://www.w3.org/2001/XMLSchema" version="2.20"><CommonAttribute uuid="10000000-0000-4000-8000-000000000001"><Properties><Name>SharedЯ</Name><Type><t:Type>x:string</t:Type><t:StringQualifiers><t:Length>40</t:Length><t:AllowedLength>Variable</t:AllowedLength></t:StringQualifiers></Type><AutoUse>DontUse</AutoUse><DataSeparation>DontUse</DataSeparation><Content>{content}</Content>{extra}</Properties></CommonAttribute></MetaDataObject>"#
    )
}

/// The existing metadata parser is the only XML/property authority used by the adapter.
fn converted(text: &str) -> Result<CommonAttribute, AdapterError> {
    let descriptor = metadata_parser::parse(text, None, PropertiesMode::All).unwrap();
    common_attribute(&descriptor.objects[0])
}

/// One common declaration supplies native object and register fields, preserving disabled targets.
#[test]
fn explicit_common_attribute_types_reach_only_current_enabled_native_fields() {
    let text = xml(
        "<r:Item><r:Metadata>Catalog.Items</r:Metadata><r:Use>Use</r:Use><r:ConditionalSeparation/></r:Item><r:Item><r:Metadata>Document.Order</r:Metadata><r:Use>Auto</r:Use></r:Item><r:Item><r:Metadata>InformationRegister.Rows</r:Metadata><r:Use>Use</r:Use></r:Item>",
        "",
    );
    let attribute = converted(&text).unwrap();
    let mut object = bsl_metadata::MetadataObject::new(MdoType::Catalog, "items");
    object.apply_common_attribute(&attribute);
    assert_eq!(
        object.find_attribute("sharedЯ").unwrap().attr_type,
        bsl_metadata::AttributeType::String { length: Some(40) }
    );
    let mut unrelated = bsl_metadata::MetadataObject::new(MdoType::Document, "Order");
    unrelated.apply_common_attribute(&attribute);
    assert!(unrelated.find_attribute("SharedЯ").is_none());
    let mut register = bsl_metadata::Register::builder()
        .mdo_type(MdoType::InformationRegister)
        .name("Rows")
        .build();
    register.apply_common_attribute(&attribute);
    assert_eq!(*register.attributes()[0].uuid(), attribute.uuid());
    assert_eq!(
        register.attributes()[0].attr_type(),
        Some(attribute.attribute_type())
    );
    object.apply_common_attribute(&attribute);
    assert_eq!(
        object.find_attribute("SharedЯ").unwrap().attr_type,
        bsl_metadata::AttributeType::Unknown
    );
    register.apply_common_attribute(&attribute);
    assert_eq!(
        register.attributes()[0].attr_type(),
        Some(&bsl_metadata::AttributeType::Unknown)
    );
}

/// A proven adopted contribution changes only supplied membership without losing base targets.
#[test]
fn adopted_common_attribute_membership_keeps_uuid_relationship_and_other_targets() {
    let mut base = converted(&xml("<r:Item><r:Metadata>Catalog.Items</r:Metadata><r:Use>Use</r:Use></r:Item><r:Item><r:Metadata>Document.Order</r:Metadata><r:Use>Use</r:Use></r:Item>", "")).unwrap();
    let text = xml("<r:Item><r:Metadata>Catalog.Items</r:Metadata><r:Use>DontUse</r:Use></r:Item>", "<ObjectBelonging>Adopted</ObjectBelonging><ExtendedConfigurationObject>10000000-0000-4000-8000-000000000001</ExtendedConfigurationObject>").replace("uuid=\"10000000-0000-4000-8000-000000000001\"", "uuid=\"20000000-0000-4000-8000-000000000001\"");
    let overlay = converted(&text).unwrap();
    assert!(overlay.adopts(&base));
    base.apply_extension_overlay(&overlay);
    assert!(!base.applies_to(MdoType::Catalog, "Items"));
    assert!(base.applies_to(MdoType::Document, "Order"));
    assert!(
        !converted(&text.replace(
            "<ExtendedConfigurationObject>10000000",
            "<ExtendedConfigurationObject>30000000"
        ))
        .unwrap()
        .adopts(&base)
    );
}

/// Unsupported controls, malformed content, namespace impostors and duplicate targets never gain coverage.
#[test]
fn unsupported_or_ambiguous_current_common_attribute_membership_is_refused() {
    let item = "<r:Item><r:Metadata>Catalog.Items</r:Metadata><r:Use>Use</r:Use></r:Item>";
    let valid = xml(item, "");
    for text in [
        valid.replace("<AutoUse>DontUse</AutoUse>", ""),
        valid.replace("<AutoUse>DontUse</AutoUse>", "<AutoUse>Use</AutoUse>"),
        valid.replace(
            "<DataSeparation>DontUse</DataSeparation>",
            "<DataSeparation>Use</DataSeparation>",
        ),
        valid.replace("<Content>", "<Content><r:Unknown/>"),
        valid.replace(
            "<r:Use>Use</r:Use>",
            "<r:Use>Use</r:Use><r:Use>DontUse</r:Use>",
        ),
        valid.replace("<r:Use>Use</r:Use>", "<r:Use>Future</r:Use>"),
        valid.replace("Catalog.Items", "Catalog.Items.Nested"),
        valid.replace("Catalog.Items", "Unknown.Items"),
        valid.replace("Catalog.Items", "CommonModule.Tools"),
        valid.replace(
            "</r:Item>",
            "<r:ConditionalSeparation>Constant.Flag</r:ConditionalSeparation></r:Item>",
        ),
        valid.replace("http://v8.1c.ru/8.3/xcf/readable", "urn:foreign"),
        xml(
            &format!("{item}{}", item.replace("Catalog.Items", "Catalog.items")),
            "",
        ),
        xml(item, "<ObjectBelonging>Adopted</ObjectBelonging>"),
    ] {
        assert!(converted(&text).is_err(), "{text}");
    }
    assert!(converted(&xml("", "")).unwrap().targets().is_empty());
}
