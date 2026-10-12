use super::*;
use crate::project_core::source_format::{NativeDocument, ObjectSource, SourcePath};

/// The public fixture includes inline identities, exact scalars, namespaces and unknown XML.
fn owner() -> ObjectSource {
    ObjectSource {
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

/// Unhandled migration annotations fail before publication; readable comments survive exact no-op.
#[test]
fn explicit_envelope_migration_keeps_transport_or_refuses_unhandled_trivia() {
    let legacy = owner().to_yaml().unwrap();
    assert_eq!(
        upgrade_text(&format!("# Authored note\n{legacy}"))
            .unwrap_err()
            .code,
        "source_layout_trivia_unsupported"
    );
    let readable = upgrade_text(&legacy.replace('\n', "\r\n")).unwrap();
    assert!(readable.contains("\r\n"));
    assert!(!readable.replace("\r\n", "").contains('\n'));
    let annotated = format!("# Authored note\r\n{readable}");
    assert_eq!(upgrade_text(&annotated).unwrap(), annotated);
}
