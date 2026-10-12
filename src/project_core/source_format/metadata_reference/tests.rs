use super::*;
use crate::project_core::source_format::{
    Descriptor, NativeDocument, ObjectSource, PropertyValue, SourcePath, readable, yaml,
};
use std::fmt::Write;

/// Use the platform's exact namespaces with independent aliases for Item and its type annotation.
fn descriptor(owner: &str, properties: &str) -> Descriptor {
    Descriptor::from_xml(&format!(r#"<MetaDataObject xmlns="http://v8.1c.ru/8.3/MDClasses" xmlns:x="http://v8.1c.ru/8.3/xcf/readable" xmlns:t="http://v8.1c.ru/8.3/xcf/readable" xmlns:i="http://www.w3.org/2001/XMLSchema-instance" version="2.20"><{owner} uuid="10000000-0000-4000-8000-000000000001"><Properties><Name>Sample</Name>{properties}</Properties></{owner}></MetaDataObject>"#)).unwrap()
}

/// Paths preserve Unicode names and reject aliases, UUID-only identities and ambiguous separators.
#[test]
fn named_paths_keep_exact_native_spelling() {
    for value in [
        "Catalog.Товары",
        "Subsystem.First.Subsystem.Second",
        "Catalog.Goods.Form.Select",
    ] {
        assert_eq!(MetadataReference::parse(value).unwrap().as_str(), value);
    }
    for value in [
        "",
        "Catalog.Goods.",
        " Catalog.Goods",
        "Catalog.Bad Name",
        "Future.Value",
        "ExternalReport.Sample",
        "10000000-0000-4000-8000-000000000001",
    ] {
        assert!(MetadataReference::parse(value).is_err(), "{value}");
    }
}

/// Both envelopes keep scalar targets, list order, repeats and URI-checked native aliases.
#[test]
fn reference_values_round_trip_in_both_envelopes() {
    let mut value = descriptor(
        "Configuration",
        r#"<DefaultLanguage>Language.Русский</DefaultLanguage><DefaultStyle>Style.Dark</DefaultStyle><DefaultRoles><x:Item i:type="t:MDObjectRef">Role.First</x:Item><x:Item i:type="t:MDObjectRef">Role.First</x:Item><x:Item i:type="t:MDObjectRef">Role.Second</x:Item></DefaultRoles>"#,
    );
    let native = value.to_xml().unwrap();
    assert!(matches!(
        value.object.property(PropertyName::DefaultLanguage),
        Some(PropertyValue::Reference(_))
    ));
    let Some(PropertyValue::ReferenceList(list)) =
        value.object.property(PropertyName::DefaultRoles)
    else {
        panic!("reference list")
    };
    assert_eq!(list.items.len(), 3);
    assert_eq!(list.native.item_prefix, "x");
    assert_eq!(list.native.type_prefix, "t");
    assert_eq!(list.native.xsi_prefix, "i");
    for version in [1, 2] {
        value.schema_version = version;
        let text = value.to_yaml().unwrap();
        let restored = Descriptor::from_yaml(&text).unwrap();
        assert_eq!(value, restored);
        assert_eq!(restored.to_xml().unwrap(), native);
        if version == 2 {
            let wire = yaml::decode(&text).unwrap();
            assert_eq!(
                wire["object"]["properties"]["DefaultRoles"],
                serde_json::json!(["Role.First", "Role.First", "Role.Second"])
            );
            assert!(
                !serde_json::to_string(&wire["object"]["native"])
                    .unwrap()
                    .contains("Role.First")
            );
        }
    }
}

/// Unsupported targets, attributes, namespaces and mixed envelopes preserve the whole property.
#[test]
fn unknown_reference_envelopes_are_not_partly_promoted() {
    for content in [
        r#"<x:Item i:type="t:MDObjectRef">Catalog.Goods</x:Item><x:Item i:type="t:MDObjectRef">10000000-0000-4000-8000-000000000001</x:Item>"#,
        r#"<x:Item i:type="t:MDObjectRef">Role.First</x:Item>"#,
        r#"<x:Item i:type="t:Future">Catalog.Goods</x:Item>"#,
        r#"<x:Item i:type="t:MDObjectRef" extra="keep">Catalog.Goods</x:Item>"#,
        r#"<x:Item xmlns:x="http://v8.1c.ru/8.3/xcf/readable" i:type="t:MDObjectRef">Catalog.Goods</x:Item>"#,
        r#"<!--keep--><x:Item i:type="t:MDObjectRef">Catalog.Goods</x:Item>"#,
        r#"<x:Item i:type="t:MDObjectRef">Catalog.Goods</x:Item>text"#,
        r#"<x:Item i:type="t:MDObjectRef">Catalog.Goods</x:Item><t:Item i:type="t:MDObjectRef">Catalog.Other</t:Item>"#,
    ] {
        let xml = format!("<Owners>{content}</Owners>");
        let value = descriptor("Catalog", &xml);
        assert!(
            value.object.property(PropertyName::Owners).is_none(),
            "{content}"
        );
        assert!(value.to_xml().unwrap().contains(&xml));
    }
    for xml in [
        "<DefaultLanguage>Catalog.Goods</DefaultLanguage>",
        "<DefaultLanguage xmlns:x=\"http://v8.1c.ru/8.3/xcf/readable\">Language.Русский</DefaultLanguage>",
    ] {
        let value = descriptor("Configuration", xml);
        assert!(
            value
                .object
                .property(PropertyName::DefaultLanguage)
                .is_none()
        );
        assert!(value.to_xml().unwrap().contains(xml));
    }
    assert!(
        descriptor(
            "Catalog",
            "<DefaultLanguage>Language.Russian</DefaultLanguage>"
        )
        .object
        .property(PropertyName::DefaultLanguage)
        .is_none()
    );
    assert!(matches!(
        descriptor("Catalog", "<Owners/>")
            .object
            .property(PropertyName::Owners),
        Some(PropertyValue::Empty)
    ));
}

/// Readable lists require a native binding, exact strings, nonempty values and valid owner domains.
#[test]
fn readable_reference_bindings_are_strict() {
    let mut value = descriptor(
        "Catalog",
        r#"<Owners><x:Item i:type="t:MDObjectRef">ChartOfCharacteristicTypes.Properties</x:Item></Owners>"#,
    );
    value.schema_version = 2;
    let wire = serde_json::to_value(&value).unwrap();
    for items in [
        serde_json::json!([]),
        serde_json::json!(null),
        serde_json::json!([123]),
        serde_json::json!(["Role.First"]),
    ] {
        let mut changed = wire.clone();
        changed["object"]["properties"]["Owners"] = items;
        assert!(Descriptor::from_yaml(&yaml::encode(&changed).unwrap()).is_err());
    }
    let mut changed = wire.clone();
    changed["object"]["native"]["sections"][0]["items"][1] = serde_json::json!("Owners");
    assert!(Descriptor::from_yaml(&yaml::encode(&changed).unwrap()).is_err());
    let mut changed = wire;
    changed["object"]["native"]["sections"][0]["items"][1]["references"]["item_prefix"] =
        serde_json::json!("missing");
    assert!(Descriptor::from_yaml(&yaml::encode(&changed).unwrap()).is_err());
}

/// Every annotated legacy list leaf migrates to its direct value or native prefix binding.
#[test]
fn reference_migration_preserves_annotations_and_exact_values() {
    let value = descriptor(
        "Configuration",
        r#"<DefaultRoles><x:Item i:type="t:MDObjectRef">Role.First</x:Item><x:Item i:type="t:MDObjectRef">Role.Second</x:Item></DefaultRoles>"#,
    );
    let old = ObjectSource {
        document: NativeDocument {
            path: SourcePath::parse("Configuration.xml").unwrap(),
            descriptor: value,
        },
        nested: Vec::new(),
    };
    let input = old.to_yaml().unwrap().lines().enumerate().fold(
        String::new(),
        |mut output, (index, line)| {
            write!(output, "{line} # field{index}\r\n").unwrap();
            output
        },
    );
    let output = readable::upgrade_text(&input).unwrap();
    assert_eq!(comments(&input), comments(&output));
    let mut expected = old.clone();
    expected.document.descriptor.schema_version = 2;
    assert_eq!(ObjectSource::from_yaml(&output).unwrap(), expected);
    assert_eq!(
        expected.document.descriptor.to_xml().unwrap(),
        old.document.descriptor.to_xml().unwrap()
    );
    assert!(!output.replace("\r\n", "").contains('\n'));
}

/// Compare actual YAML comment tokens independently of their new source coordinates.
fn comments(input: &str) -> Vec<String> {
    let mut values: Vec<_> = yaml_edit::YamlFile::parse(input)
        .tree()
        .comments()
        .map(|comment| comment.text().to_string())
        .collect();
    values.sort();
    values
}
