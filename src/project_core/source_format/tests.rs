use super::*;
use crate::project_core::MetadataKind;

/// A small source document exercises exact values, nil, language keys and scoped XML.
fn descriptor() -> Descriptor {
    let namespaces = vec![Namespace {
        prefix: None,
        uri: MD_NAMESPACE.into(),
    }];
    Descriptor {
        schema_version: SCHEMA_VERSION,
        rules_version: RULES_VERSION,
        profile: PROFILE.into(),
        xml: XmlProfile {
            version: "2.20".into(), namespaces: namespaces.clone(), attributes: vec![],
            bom: true, line_ending: LineEnding::CrLf,
        },
        object: MetadataNode {
            kind: MetadataKind::Configuration,
            uuid: NativeUuid::parse("10000000-0000-4000-8000-000000000001").unwrap(),
            namespaces, attributes: vec![],
            sections: vec![Section::Properties(vec![
                Property::Typed { name: PropertyName::Name, value: PropertyValue::Text("SampleConfiguration".into()) },
                Property::Typed { name: PropertyName::Comment, value: PropertyValue::Empty },
                Property::Typed { name: PropertyName::Version, value: PropertyValue::Text("1.2.3.004".into()) },
                Property::Typed { name: PropertyName::Synonym, value: PropertyValue::Localized(vec![
                    LocalizedText { language: "ru".into(), text: "Образец & точный текст\r\n".into() },
                    LocalizedText { language: "en".into(), text: "Sample".into() },
                ]) },
                Property::Preserved { fragment: XmlFragment {
                    xml: "<FutureProperty xmlns:local=\"urn:fixture:type\" xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\" xsi:type=\"local:FutureType\"><Number>123456789012345678901234567890.0001</Number></FutureProperty>".into(),
                    namespaces: vec![Namespace { prefix: None, uri: MD_NAMESPACE.into() }],
                } },
            ])],
        },
    }
}

#[test]
fn yaml_preserves_exact_values_and_native_context() {
    let original = descriptor();
    let yaml = original.to_yaml().unwrap();
    assert!(yaml.contains("1.2.3.004"));
    assert!(yaml.contains("123456789012345678901234567890.0001"));
    let restored = Descriptor::from_yaml(&yaml).unwrap();
    assert_eq!(original, restored);
    assert_eq!(yaml, restored.to_yaml().unwrap());
}

#[test]
fn distinct_states_are_not_coerced_or_defaulted() {
    let mut source = descriptor();
    assert_eq!(
        source.object.property(PropertyName::Comment),
        Some(&PropertyValue::Empty)
    );
    let Section::Properties(properties) = &mut source.object.sections[0] else {
        panic!("properties")
    };
    properties.retain(|property| {
        !matches!(
            property,
            Property::Typed {
                name: PropertyName::Comment,
                ..
            }
        )
    });
    assert_eq!(source.object.property(PropertyName::Comment), None);
    let Section::Properties(properties) = &mut source.object.sections[0] else {
        panic!("properties")
    };
    properties.push(Property::Typed {
        name: PropertyName::Comment,
        value: PropertyValue::Nil,
    });
    let restored = Descriptor::from_yaml(&source.to_yaml().unwrap()).unwrap();
    assert_eq!(
        restored.object.property(PropertyName::Comment),
        Some(&PropertyValue::Nil)
    );
}

#[test]
fn unsupported_versions_are_refused_without_migration() {
    for (key, code) in [
        ("schema_version", "source_schema_unsupported"),
        ("rules_version", "source_rules_unsupported"),
    ] {
        let yaml = descriptor().to_yaml().unwrap();
        let changed = yaml.replace(&format!("\"{key}\": 1"), &format!("\"{key}\": 999"));
        assert_eq!(Descriptor::from_yaml(&changed).unwrap_err().code, code);
    }
}

#[test]
fn yaml_profile_rejects_silent_overwrites_and_expansion() {
    for (yaml, code) in [
        ("name: one\nname: two\n", "source_yaml_key_duplicate"),
        ("name: one\n'name': two\n", "source_yaml_key_duplicate"),
        (
            "name: &base one\ncopy: *base\n",
            "source_yaml_construct_forbidden",
        ),
        ("name: !custom one\n", "source_yaml_construct_forbidden"),
        ("<<: {}\n", "source_yaml_construct_forbidden"),
        ("---\na: 1\n---\nb: 2\n", "source_yaml_document_count"),
        ("\u{feff}a: 1\n", "source_yaml_bom_forbidden"),
        ("name: >-\n  folded\n", "source_yaml_folding_forbidden"),
        (
            "18446744073709551616: value\n",
            "source_yaml_integer_overflow",
        ),
    ] {
        assert_eq!(yaml::decode(yaml).unwrap_err().code, code, "{yaml}");
    }
}

#[test]
fn uuid_spelling_is_retained_but_collisions_ignore_case() {
    let upper = NativeUuid::parse("ABCDEFAB-0000-4000-8000-000000000001").unwrap();
    assert_eq!(upper.as_str(), "ABCDEFAB-0000-4000-8000-000000000001");
    let mut source = descriptor();
    source.object.uuid = upper;
    let mut child = source.object.clone();
    child.uuid = NativeUuid::parse("abcdefab-0000-4000-8000-000000000001").unwrap();
    source
        .object
        .sections
        .push(Section::Children(vec![Child::Inline {
            object: Box::new(child),
        }]));
    assert_eq!(source.validate().unwrap_err().code, "native_uuid_duplicate");
    for invalid in ["", "not-a-uuid", "10000000-0000-4000-8000-00000000000x"] {
        assert!(NativeUuid::parse(invalid).is_err());
    }
}

#[test]
fn malformed_preserved_xml_and_namespace_spoofing_are_rejected() {
    let mut source = descriptor();
    source.xml.namespaces.push(Namespace {
        prefix: None,
        uri: "urn:spoof".into(),
    });
    assert_eq!(
        source.validate().unwrap_err().code,
        "source_namespace_invalid"
    );
    let mut source = descriptor();
    source.object.sections.push(Section::Preserved(XmlFragment {
        xml: "<Broken>".into(),
        namespaces: vec![],
    }));
    assert_eq!(
        source.validate().unwrap_err().code,
        "source_preserved_xml_invalid"
    );
}

#[test]
fn input_formatting_does_not_change_semantic_values() {
    let original = descriptor();
    let input = format!(
        "# comment\r\n{}",
        original.to_yaml().unwrap().replace('\n', "\r\n")
    );
    assert_eq!(original, Descriptor::from_yaml(&input).unwrap());
}

#[test]
fn unknown_schema_fields_are_not_silently_ignored() {
    let input = format!(
        "unexpected_field: true\n{}",
        descriptor().to_yaml().unwrap()
    );
    assert_eq!(
        Descriptor::from_yaml(&input).unwrap_err().code,
        "source_schema_invalid"
    );
}

#[test]
fn opaque_parts_cannot_duplicate_known_sections_or_properties() {
    let mut source = descriptor();
    let Section::Properties(properties) = &mut source.object.sections[0] else {
        panic!("properties")
    };
    properties.push(Property::Preserved {
        fragment: XmlFragment {
            xml: "<Name>Spoof</Name>".into(),
            namespaces: vec![Namespace {
                prefix: None,
                uri: MD_NAMESPACE.into(),
            }],
        },
    });
    assert_eq!(
        source.validate().unwrap_err().code,
        "source_property_duplicate"
    );
    let mut source = descriptor();
    source.object.sections.push(Section::Preserved(XmlFragment {
        xml: "<ChildObjects/>".into(),
        namespaces: vec![Namespace {
            prefix: None,
            uri: MD_NAMESPACE.into(),
        }],
    }));
    assert_eq!(
        source.validate().unwrap_err().code,
        "source_preserved_slot_invalid"
    );
}

#[test]
fn yaml_control_escapes_cannot_create_invalid_native_xml() {
    let mut source = descriptor();
    let Section::Properties(properties) = &mut source.object.sections[0] else {
        panic!("properties")
    };
    properties[1] = Property::Typed {
        name: PropertyName::Comment,
        value: PropertyValue::Text("invalid\0control".into()),
    };
    assert_eq!(
        source.validate().unwrap_err().code,
        "source_xml_character_invalid"
    );
}

#[test]
fn nested_yaml_failures_retain_machine_coordinates() {
    let error =
        yaml::decode("object:\n  properties:\n    Name: first\n    'Name': second\n").unwrap_err();
    assert_eq!(error.code, "source_yaml_key_duplicate");
    assert_eq!(error.location, ["object", "properties", "Name"]);
}

#[test]
fn inherited_bindings_do_not_require_repeated_namespace_tables() {
    let mut source = descriptor();
    source.object.namespaces.clear();
    assert!(source.validate().is_ok());
    source.object.attributes.push(XmlAttribute {
        name: "unbound:value".into(),
        value: "data".into(),
    });
    assert_eq!(
        source.validate().unwrap_err().code,
        "source_attribute_namespace_missing"
    );
}

#[test]
fn documented_yaml_is_accepted_without_value_normalization() {
    let input = include_str!("../../../docs/research/eska-format/configuration.example.yaml");
    let source = Descriptor::from_yaml(input).unwrap();
    assert_eq!(
        source.object.property(PropertyName::Version),
        Some(&PropertyValue::Text("1.2.3.004".into()))
    );
    assert_eq!(
        source,
        Descriptor::from_yaml(&source.to_yaml().unwrap()).unwrap()
    );
}
