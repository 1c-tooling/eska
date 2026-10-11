use super::*;
use crate::project_core::source_format::{Descriptor, MD_NAMESPACE, Property, Section};

/// Construct one independent native envelope with an exact scalar spelling and known owner.
fn native(owner: MetadataKind, property: &str) -> Descriptor {
    Descriptor::from_xml(&format!(
        "\u{feff}<?xml version=\"1.0\" encoding=\"UTF-8\"?>\r\n<MetaDataObject xmlns=\"{MD_NAMESPACE}\" xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\" version=\"2.20\"><{} uuid=\"10000000-0000-4000-8000-000000000001\"><Properties><Name>Sample</Name>{property}</Properties></{}></MetaDataObject>\r\n",
        owner.designer_tag(), owner.designer_tag()
    )).unwrap()
}

/// Access actual parsed source properties, independently of the editable-vocabulary lookup.
fn properties(descriptor: &Descriptor) -> &[Property] {
    let Section::Properties(values) = &descriptor.object.sections[0] else {
        panic!("properties")
    };
    values
}

/// Bool, exact integer and enum values survive both source and native serialization.
#[test]
fn scalar_domains_round_trip_with_native_encoding_and_no_numeric_coercion() {
    for (owner, name, text, expected) in [
        (
            MetadataKind::Catalog,
            PropertyName::Hierarchical,
            "false",
            PropertyValue::Boolean(false),
        ),
        (
            MetadataKind::CommonModule,
            PropertyName::ServerCall,
            "true",
            PropertyValue::Boolean(true),
        ),
        (
            MetadataKind::Catalog,
            PropertyName::CodeLength,
            "000000000000000000000000000000000000000009",
            PropertyValue::Integer(
                ExactUnsigned::parse("000000000000000000000000000000000000000009").unwrap(),
            ),
        ),
        (
            MetadataKind::TabularSection,
            PropertyName::LineNumberLength,
            "5",
            PropertyValue::Integer(ExactUnsigned::parse("5").unwrap()),
        ),
        (
            MetadataKind::Catalog,
            PropertyName::CodeType,
            "Number",
            PropertyValue::Enumeration("Number".into()),
        ),
        (
            MetadataKind::InformationRegister,
            PropertyName::InformationRegisterPeriodicity,
            "Quarter",
            PropertyValue::Enumeration("Quarter".into()),
        ),
        (
            MetadataKind::Attribute,
            PropertyName::Indexing,
            "IndexWithAdditionalOrder",
            PropertyValue::Enumeration("IndexWithAdditionalOrder".into()),
        ),
    ] {
        let original = native(
            owner,
            &format!("<{}>{text}</{}>", name.as_str(), name.as_str()),
        );
        assert_eq!(original.object.property(name), Some(&expected));
        let yaml = original.to_yaml().unwrap();
        let restored = Descriptor::from_yaml(&yaml).unwrap();
        assert_eq!(original, restored);
        let xml = restored.to_xml().unwrap();
        assert!(xml.starts_with('\u{feff}'));
        assert!(!xml.replace("\r\n", "").contains('\n'));
        assert!(xml.contains(&format!("<{}>{text}</{}>", name.as_str(), name.as_str())));
        assert_eq!(original, Descriptor::from_xml(&xml).unwrap());
    }
}

/// Unsupported spellings, owners and envelopes cannot become typed simply by sharing a tag.
#[test]
fn unknown_scalar_values_and_envelopes_remain_preserved() {
    for (owner, xml) in [
        (MetadataKind::Catalog, "<Hierarchical>1</Hierarchical>"),
        (MetadataKind::Catalog, "<Hierarchical>False</Hierarchical>"),
        (
            MetadataKind::Catalog,
            "<Hierarchical> false </Hierarchical>",
        ),
        (MetadataKind::Catalog, "<CodeLength>-1</CodeLength>"),
        (MetadataKind::Catalog, "<CodeLength>9.0</CodeLength>"),
        (MetadataKind::Catalog, "<CodeType>FutureKind</CodeType>"),
        (
            MetadataKind::Catalog,
            "<CodeType xsi:type=\"xsi:Future\">String</CodeType>",
        ),
        (
            MetadataKind::Catalog,
            "<CodeType><!--keep-->String</CodeType>",
        ),
        (MetadataKind::Catalog, "<Server>true</Server>"),
        (MetadataKind::Document, "<CodeLength>9</CodeLength>"),
        (MetadataKind::Catalog, "<Use>true</Use>"),
        (
            MetadataKind::Catalog,
            "<Hierarchical xmlns=\"urn:foreign\">true</Hierarchical>",
        ),
    ] {
        let descriptor = native(owner, xml);
        assert!(
            matches!(&properties(&descriptor)[1], Property::Preserved { fragment } if fragment.xml == xml),
            "{xml}"
        );
        assert_eq!(
            descriptor,
            Descriptor::from_yaml(&descriptor.to_yaml().unwrap()).unwrap()
        );
        assert!(descriptor.to_xml().unwrap().contains(xml), "{xml}");
    }
}

/// Absence, empty, nil and an explicit scalar default remain four different source states.
#[test]
fn scalar_states_never_infer_native_defaults() {
    for (xml, expected) in [
        ("", None),
        ("<Hierarchical/>", Some(PropertyValue::Empty)),
        ("<Hierarchical xsi:nil=\"true\"/>", Some(PropertyValue::Nil)),
        (
            "<Hierarchical>false</Hierarchical>",
            Some(PropertyValue::Boolean(false)),
        ),
    ] {
        let descriptor = native(MetadataKind::Catalog, xml);
        assert_eq!(
            descriptor.object.property(PropertyName::Hierarchical),
            expected.as_ref()
        );
        assert_eq!(
            descriptor,
            Descriptor::from_xml(&descriptor.to_xml().unwrap()).unwrap()
        );
    }
}

/// Manual and deserialized values pass the same owner/domain validation as native imports.
#[test]
fn invalid_typed_domains_and_numeric_yaml_tokens_are_refused() {
    for value in [
        PropertyValue::Text("false".into()),
        PropertyValue::Enumeration("false".into()),
        PropertyValue::Integer(ExactUnsigned::parse("0").unwrap()),
    ] {
        let mut descriptor = native(MetadataKind::Catalog, "<Hierarchical>false</Hierarchical>");
        let Section::Properties(values) = &mut descriptor.object.sections[0] else {
            panic!("properties")
        };
        values[1] = Property::Typed {
            name: PropertyName::Hierarchical,
            value,
        };
        assert_eq!(
            descriptor.validate().unwrap_err().code,
            "source_property_type_invalid"
        );
    }
    for value in ["", "-1", "+1", "1.0", "1e4", " 9", "９"] {
        assert!(ExactUnsigned::parse(value).is_err(), "{value}");
    }
    for value in [
        serde_json::json!(9),
        serde_json::json!(true),
        serde_json::Value::Null,
    ] {
        assert!(serde_json::from_value::<ExactUnsigned>(value).is_err());
    }
    let descriptor = native(MetadataKind::Catalog, "<CodeLength>009</CodeLength>");
    assert!(Descriptor::from_yaml(&descriptor.to_yaml().unwrap().replace("\"009\"", "9")).is_err());
}

/// Every enum domain rejects future or misspelled values without treating them as arbitrary text.
#[test]
fn enum_validation_uses_both_property_and_owner() {
    for (owner, property) in [
        (MetadataKind::Catalog, PropertyName::CodeType),
        (MetadataKind::Document, PropertyName::NumberType),
        (MetadataKind::Catalog, PropertyName::CodeAllowedLength),
        (MetadataKind::Document, PropertyName::NumberAllowedLength),
        (
            MetadataKind::InformationRegister,
            PropertyName::InformationRegisterPeriodicity,
        ),
        (MetadataKind::CommonModule, PropertyName::ReturnValuesReuse),
        (MetadataKind::Attribute, PropertyName::FillChecking),
        (MetadataKind::Attribute, PropertyName::Indexing),
    ] {
        let mut descriptor = native(owner, "");
        let Section::Properties(values) = &mut descriptor.object.sections[0] else {
            panic!("properties")
        };
        values.push(Property::Typed {
            name: property,
            value: PropertyValue::Enumeration("FutureValue".into()),
        });
        assert_eq!(
            descriptor.validate().unwrap_err().code,
            "source_property_type_invalid"
        );
    }
    let mut descriptor = native(MetadataKind::Catalog, "");
    let Section::Properties(values) = &mut descriptor.object.sections[0] else {
        panic!("properties")
    };
    values.push(Property::Typed {
        name: PropertyName::Server,
        value: PropertyValue::Boolean(true),
    });
    assert_eq!(
        descriptor.validate().unwrap_err().code,
        "source_property_type_invalid"
    );
}
