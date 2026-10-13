use super::*;
use crate::project_core::source_format::{NativeDocument, ObjectSource, SourcePath};
use std::fmt::Write;

/// The public fixture includes inline identities, exact scalars, namespaces and unknown XML.
fn owner() -> ObjectSource {
    ObjectSource {
        form: None,
        document: NativeDocument {
            path: SourcePath::parse("Catalogs/Sample.xml").unwrap(),
            descriptor: Descriptor::from_xml(include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/fixtures/eska-format/designer/Catalogs/Sample.xml"
            )))
            .unwrap(),
        },
        nested: Vec::new(),
    }
}

/// Schema change affects representation only, including every opaque/native ordinal slot.
#[test]
fn named_values_round_trip_exact_native_model_and_unknown_fragments() {
    let mut owner = owner();
    let native = owner.document.descriptor.to_xml().unwrap();
    let legacy = owner.to_yaml().unwrap();
    let readable = upgrade_text(&legacy).unwrap();
    upgrade(&mut owner);
    assert_eq!(ObjectSource::from_yaml(&readable).unwrap(), owner);
    assert_eq!(owner.document.descriptor.to_xml().unwrap(), native);
    assert_eq!(upgrade_text(&readable).unwrap(), readable);
    let wire = serde_json::to_value(&owner.document.descriptor).unwrap();
    assert_eq!(wire["object"]["properties"]["Name"], "Sample");
    assert_eq!(wire["object"]["properties"]["CodeLength"], "9");
    assert_eq!(wire["object"]["properties"]["Hierarchical"], false);
    assert_eq!(
        wire["object"]["properties"]["Comment"],
        serde_json::json!({"state":"empty"})
    );
    assert!(wire["object"].get("sections").is_none());
    assert!(wire.get("xml").is_none());
    assert!(
        wire["object"]["native"]["sections"][1]["items"]
            .as_array()
            .unwrap()
            .contains(&Value::String("Name".into()))
    );
    assert!(readable.find("\"properties\":").unwrap() < readable.find("\"native\":").unwrap());
    assert!(readable.contains("FutureProperty"));
    assert_eq!(
        ObjectSource::from_yaml(&legacy).unwrap().to_yaml().unwrap(),
        legacy
    );
}

/// A missing/extra/repeated binding, wrong owner, null or incompatible lexical type cannot bypass validation.
#[test]
fn readable_bindings_and_property_domains_are_strict() {
    let mut owner = owner();
    upgrade(&mut owner);
    let original = serde_json::to_value(&owner.document.descriptor).unwrap();
    let mut cases = Vec::new();
    let mut value = original.clone();
    value["object"]["properties"]
        .as_object_mut()
        .unwrap()
        .remove("Name");
    cases.push(value);
    let mut value = original.clone();
    value["object"]["native"]["sections"][1]["items"]
        .as_array_mut()
        .unwrap()
        .push(Value::String("Name".into()));
    cases.push(value);
    let mut value = original.clone();
    value["object"]["properties"]["ReturnValuesReuse"] = Value::String("DontUse".into());
    cases.push(value);
    for invalid in [
        Value::Null,
        Value::Bool(false),
        Value::from(9),
        Value::String("9.0".into()),
        serde_json::json!({"state":"integer", "value":"9"}),
    ] {
        let mut value = original.clone();
        value["object"]["properties"]["CodeLength"] = invalid;
        cases.push(value);
    }
    let mut value = original.clone();
    value["object"]["sections"] = serde_json::json!([]);
    cases.push(value);
    for value in cases {
        let decoded = serde_json::from_value::<Descriptor>(value);
        assert!(decoded.is_err() || decoded.unwrap().validate().is_err());
    }
    let mut precise = original;
    precise["object"]["properties"]["CodeLength"] =
        Value::String("0001234567890123456789012345678901234567890".into());
    let decoded: Descriptor = serde_json::from_value(precise).unwrap();
    decoded.validate().unwrap();
    assert_eq!(
        decoded.object.property(PropertyName::CodeLength),
        Some(&PropertyValue::Integer(
            ExactUnsigned::parse("0001234567890123456789012345678901234567890").unwrap()
        ))
    );
}

/// Migration retains annotations; readable input remains an exact no-op and CRLF stays transport.
#[test]
fn explicit_envelope_migration_keeps_annotations_transport_and_exact_noop() {
    let legacy = owner().to_yaml().unwrap();
    let annotated = upgrade_text(&format!("# Authored note\n{legacy}")).unwrap();
    assert!(annotated.starts_with("# Authored note\n"));
    let readable = upgrade_text(&legacy.replace('\n', "\r\n")).unwrap();
    assert!(readable.contains("\r\n"));
    assert!(!readable.replace("\r\n", "").contains('\n'));
    let annotated = format!("# Authored note\r\n{readable}");
    assert_eq!(upgrade_text(&annotated).unwrap(), annotated);
}

/// Every fixture field has an annotation, including wrappers, inline descendants and native XML.
#[test]
fn migration_retains_every_field_annotation_and_exact_values() {
    let source = owner();
    let legacy = source.to_yaml().unwrap();
    let mut annotated = String::from("# Header\n");
    for (index, line) in legacy.lines().enumerate() {
        writeln!(annotated, "{line} # field-{index}").unwrap();
    }
    annotated.push_str("# Footer\n");
    assert_eq!(ObjectSource::from_yaml(&annotated).unwrap(), source);
    let upgraded = upgrade_text(&annotated).unwrap();
    assert_eq!(
        ObjectSource::from_yaml(&upgraded)
            .unwrap()
            .document
            .descriptor
            .to_xml()
            .unwrap(),
        source.document.descriptor.to_xml().unwrap()
    );
    for index in 0..legacy.lines().count() {
        assert_eq!(
            upgraded.matches(&format!("# field-{index}\n")).count(),
            1,
            "field-{index}"
        );
    }
    assert!(upgraded.starts_with("# Header\n"));
    assert!(upgraded.ends_with("# Footer\n"));
}

/// Removed scalar wrappers attach to their own property; literal/quoted data retain exact meanings.
#[test]
fn migration_annotations_follow_property_identity_and_preserve_styles() {
    let source = owner();
    let legacy = source
        .to_yaml()
        .unwrap()
        .replacen(
            "\"uuid\": \"20000000-0000-4000-8000-000000000001\"",
            "\"uuid\": \"20000000-0000-4000-8000-000000000001\" # Identity note",
            1,
        )
        .replacen(
            "\"name\": \"Comment\"",
            "\"name\": \"Comment\" # Comment binding",
            1,
        )
        .replace(
            "\"value\": \"Editable part\"",
            "\"value\": 'Editable part' # Attribute note",
        )
        .replace(
            "\"text\": \"Образец\"",
            "\"text\": 'Образец' # Localized note",
        );
    let mut annotated = String::from("# Same\n");
    for line in legacy.lines() {
        if line.contains("\"xml\":") && line.contains("FutureProperty") {
            let (key, encoded) = line.split_once(':').unwrap();
            let value: String = serde_json::from_str(encoded.trim()).unwrap();
            let indent = " ".repeat(line.len() - line.trim_start().len() + 2);
            writeln!(annotated, "{key}: |- # Opaque note\n{indent}{value}").unwrap();
        } else {
            annotated.push_str(line);
            annotated.push('\n');
        }
    }
    annotated.push_str("# Same\n");
    for text in [annotated.clone(), annotated.replace('\n', "\r\n")] {
        let upgraded = upgrade_text(&text).unwrap();
        let normalized = upgraded.replace("\r\n", "\n");
        assert!(normalized.contains("# Comment binding\n        \"Comment\":"));
        assert!(
            normalized.contains(
                "# Identity note\n      \"uuid\": \"20000000-0000-4000-8000-000000000001\""
            )
        );
        assert!(
            normalized.contains("# Attribute note\n              \"Comment\": 'Editable part'")
        );
        assert!(normalized.contains("'Образец' # Localized note"));
        assert!(normalized.contains("|- # Opaque note"));
        assert_eq!(normalized.matches("# Same\n").count(), 2);
        assert_eq!(
            ObjectSource::from_yaml(&upgraded)
                .unwrap()
                .document
                .descriptor
                .to_xml()
                .unwrap(),
            source.document.descriptor.to_xml().unwrap()
        );
        if text.contains("\r\n") {
            assert!(!upgraded.replace("\r\n", "").contains('\n'));
        }
    }
    let mixed = annotated.replacen('\n', "\r\n", 1);
    assert_eq!(
        upgrade_text(&mixed).unwrap_err().code,
        "source_layout_trivia_unsupported"
    );
}

/// Mixed envelopes retain family wrappers and already readable descriptors without promoting XML.
#[test]
fn migration_annotations_preserve_nested_and_mixed_descriptor_envelopes() {
    let mut source = owner();
    source.nested.push(NativeDocument {
        path: SourcePath::parse("Catalogs/Sample/Forms/Main.xml").unwrap(),
        descriptor: Descriptor::from_xml(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/eska-format/designer/Catalogs/Sample/Forms/Main.xml"
        )))
        .unwrap(),
    });
    for readable_nested in [false, true] {
        if readable_nested {
            source.nested[0].descriptor.schema_version = super::super::READABLE_SCHEMA_VERSION;
        }
        let text = format!("# Family\n{}", source.to_yaml().unwrap())
            .replace("\"value\": \"Main\"", "\"value\": 'Main' # Nested note")
            .replace("\"Name\": \"Main\"", "\"Name\": 'Main' # Nested note");
        let mut expected = source.clone();
        upgrade(&mut expected);
        let upgraded = upgrade_text(&text).unwrap();
        assert_eq!(ObjectSource::from_yaml(&upgraded).unwrap(), expected);
        assert_eq!(upgraded.matches("# Nested note").count(), 1);
        assert_eq!(upgrade_text(&upgraded).unwrap(), upgraded);
    }
}
