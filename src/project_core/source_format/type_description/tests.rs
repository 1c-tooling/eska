use super::*;
use crate::project_core::metadata_parser::{self, PropertiesMode};
use crate::project_core::source_format::{
    Descriptor, MD_NAMESPACE, PropertyName, PropertyValue, READABLE_SCHEMA_VERSION, Section,
};

/// Native fixtures declare the real namespace context instead of guessing meaning from prefixes.
fn descriptor(value: &str) -> Descriptor {
    Descriptor::from_xml(&format!("\u{feff}<?xml version=\"1.0\" encoding=\"UTF-8\"?>\r\n<MetaDataObject xmlns=\"{MD_NAMESPACE}\" xmlns:v8=\"{CORE_NAMESPACE}\" xmlns:xs=\"http://www.w3.org/2001/XMLSchema\" xmlns:cfg=\"http://v8.1c.ru/8.1/data/enterprise/current-config\" version=\"2.20\"><Attribute uuid=\"10000000-0000-4000-8000-000000000001\"><Properties><Name>Amount</Name>{value}</Properties></Attribute></MetaDataObject>\r\n")).unwrap()
}

/// One value authority preserves repeated alternatives, type sets, order and exact qualifier digits.
#[test]
fn composite_types_round_trip_both_envelopes_and_native_semantics() {
    let source = descriptor(
        "<Type><v8:Type>xs:decimal</v8:Type><v8:NumberQualifiers><v8:AllowedSign>Nonnegative</v8:AllowedSign><v8:FractionDigits>0002</v8:FractionDigits><v8:Digits>000000000000000000000000000000009</v8:Digits></v8:NumberQualifiers><v8:TypeSet>cfg:DefinedType.Сумма</v8:TypeSet><v8:Type>xs:string</v8:Type><v8:StringQualifiers><v8:AllowedLength>Fixed</v8:AllowedLength><v8:Length>0</v8:Length></v8:StringQualifiers><v8:Type>xs:dateTime</v8:Type><v8:DateQualifiers><v8:DateFractions>DateTime</v8:DateFractions></v8:DateQualifiers></Type>",
    );
    let Some(PropertyValue::TypeDescription(value)) = source.object.property(PropertyName::Type)
    else {
        panic!("typed description")
    };
    assert_eq!(value.data.alternatives.len(), 4);
    assert_eq!(
        value
            .data
            .number
            .as_ref()
            .unwrap()
            .fraction_digits
            .as_ref()
            .unwrap()
            .as_str(),
        "0002"
    );
    let native = source.to_xml().unwrap();
    assert_eq!(source, Descriptor::from_xml(&native).unwrap());
    for schema in [1, READABLE_SCHEMA_VERSION] {
        let mut source = source.clone();
        source.schema_version = schema;
        let yaml = source.to_yaml().unwrap();
        assert_eq!(source, Descriptor::from_yaml(&yaml).unwrap());
        assert_eq!(
            native,
            Descriptor::from_yaml(&yaml).unwrap().to_xml().unwrap()
        );
        assert_eq!(yaml.matches("cfg:DefinedType.Сумма").count(), 1);
        assert_eq!(yaml.matches("0002").count(), 1);
    }
    let parsed = metadata_parser::parse(&native, None, PropertiesMode::All).unwrap();
    let property = parsed.objects[0]
        .properties
        .as_ref()
        .unwrap()
        .iter()
        .find(|property| property.property.key.name == "Type")
        .unwrap();
    assert_eq!(
        value.to_metadata(&source.xml.namespaces).unwrap(),
        property.property.value
    );
}

/// Unbound layouts, null defaults and imprecise digits cannot become editable native values.
#[test]
fn readable_type_bindings_and_qualifier_domains_refuse_ambiguous_values() {
    let mut source = descriptor(
        "<Type><v8:Type>xs:decimal</v8:Type><v8:NumberQualifiers><v8:Digits>9</v8:Digits><v8:FractionDigits>2</v8:FractionDigits><v8:AllowedSign>Any</v8:AllowedSign></v8:NumberQualifiers></Type>",
    );
    source.schema_version = READABLE_SCHEMA_VERSION;
    let wire = serde_json::to_value(source).unwrap();
    let mut invalid = Vec::new();
    for (key, value) in [
        ("digits", serde_json::Value::Null),
        ("digits", serde_json::json!(9)),
        ("fraction_digits", serde_json::json!("-1")),
        ("allowed_sign", serde_json::json!("Future")),
    ] {
        let mut next = wire.clone();
        next["object"]["properties"]["Type"]["number"][key] = value;
        invalid.push(next);
    }
    let mut next = wire.clone();
    next["object"]["properties"]["Type"]["alternatives"][0]["name"] =
        serde_json::json!("unknown:Type");
    invalid.push(next);
    for (key, value) in [
        ("prefix", serde_json::json!("xs")),
        (
            "parts",
            serde_json::json!([{"kind":"alternative","index":0},{"kind":"alternative","index":0},{"kind":"number"}]),
        ),
        (
            "parts",
            serde_json::json!([{"kind":"alternative","index":1},{"kind":"number"}]),
        ),
        (
            "number_fields",
            serde_json::json!(["digits", "digits", "fraction_digits", "allowed_sign"]),
        ),
        (
            "number_fields",
            serde_json::json!(["digits", "allowed_sign"]),
        ),
    ] {
        let mut next = wire.clone();
        next["object"]["native"]["sections"][0]["items"][1]["layout"][key] = value;
        invalid.push(next);
    }
    let mut next = wire.clone();
    next["object"]["native"]["sections"][0]["items"][1] = serde_json::json!("Type");
    invalid.push(next);
    let mut next = wire.clone();
    next["object"]["native"]["sections"][0]["items"][1]["name"] = serde_json::json!("Comment");
    invalid.push(next);
    let mut next = wire;
    next["object"]["properties"]["Type"]["number"]
        .as_object_mut()
        .unwrap()
        .remove("digits");
    invalid.push(next);
    for value in invalid {
        assert!(Descriptor::from_yaml(&serde_json::to_string(&value).unwrap()).is_err());
    }
}

/// Old typed descriptions migrate annotations and values separately from their reconstruction slots.
#[test]
fn annotated_type_migration_retains_one_value_authority_and_native_layout() {
    use crate::project_core::source_format::{NativeDocument, ObjectSource, SourcePath};
    let source = ObjectSource {
        form: None,
        document: NativeDocument {
            path: SourcePath::parse("Constants/Amount.xml").unwrap(),
            descriptor: descriptor(
                "<Type><v8:Type>xs:string</v8:Type><v8:StringQualifiers><v8:Length>0009</v8:Length><v8:AllowedLength>Variable</v8:AllowedLength></v8:StringQualifiers></Type>",
            ),
        },
        nested: Vec::new(),
    };
    let original = source.to_yaml().unwrap();
    let mut input = String::from("# Header\r\n");
    for (index, line) in original.lines().enumerate() {
        use std::fmt::Write;
        write!(input, "{line} # field-{index}\r\n").unwrap();
    }
    input.push_str("# Footer\r\n");
    let output = crate::project_core::source_format::readable::upgrade_text(&input).unwrap();
    let updated = ObjectSource::from_yaml(&output).unwrap();
    assert_eq!(
        source.document.descriptor.to_xml().unwrap(),
        updated.document.descriptor.to_xml().unwrap()
    );
    assert!(!output.replace("\r\n", "").contains('\n'));
    assert_eq!(output.matches("0009").count(), 1);
    for index in 0..original.lines().count() {
        assert_eq!(output.matches(&format!("# field-{index}\r\n")).count(), 1);
    }
    assert_eq!(
        crate::project_core::source_format::readable::upgrade_text(&output).unwrap(),
        output
    );
}

/// A partially understood type cannot drop foreign qualifiers, annotations or namespace scope.
#[test]
fn unknown_type_shapes_remain_whole_preserved_fragments() {
    for xml in [
        "<Type><v8:Type>missing:Unknown</v8:Type></Type>",
        "<Type><v8:Type>xs:decimal</v8:Type><v8:NumberQualifiers><v8:Digits>-1</v8:Digits></v8:NumberQualifiers></Type>",
        "<Type><v8:Type>xs:string</v8:Type><v8:StringQualifiers><v8:AllowedLength>Future</v8:AllowedLength></v8:StringQualifiers></Type>",
        "<Type><v8:Type>xs:string</v8:Type><!--keep--></Type>",
        "<Type><v8:Type xmlns:v8=\"http://v8.1c.ru/8.1/data/core\">xs:string</v8:Type></Type>",
        "<Type><v8:Type>xs:string</v8:Type><v8:Future>0</v8:Future></Type>",
        "<Type><v8:NumberQualifiers/></Type>",
        "<Type><v8:NumberQualifiers><v8:Digits>9</v8:Digits><v8:Digits>10</v8:Digits></v8:NumberQualifiers></Type>",
        "<Type><v8:Type>xs:string</v8:Type><v8:StringQualifiers><v8:Length>4</v8:Length></v8:StringQualifiers><v8:StringQualifiers><v8:Length>5</v8:Length></v8:StringQualifiers></Type>",
    ] {
        let source = descriptor(xml);
        assert!(
            source.object.property(PropertyName::Type).is_none(),
            "{xml}"
        );
        let Section::Properties(properties) = &source.object.sections[0] else {
            panic!("properties")
        };
        assert!(
            matches!(&properties[1], crate::project_core::source_format::Property::Preserved {fragment} if fragment.xml==xml),
            "{xml}"
        );
        assert!(source.to_xml().unwrap().contains(xml));
    }
}

/// The same native tag in a style item has another domain and cannot acquire composite semantics.
#[test]
fn type_owner_and_absent_states_do_not_infer_an_editable_description() {
    for (xml, expected) in [("", None), ("<Type/>", Some(PropertyValue::Empty))] {
        let source = descriptor(xml);
        assert_eq!(
            source.object.property(PropertyName::Type),
            expected.as_ref()
        );
        let mut readable = source.clone();
        readable.schema_version = READABLE_SCHEMA_VERSION;
        assert_eq!(
            readable,
            Descriptor::from_yaml(&readable.to_yaml().unwrap()).unwrap()
        );
    }
    let source = descriptor("<Type><v8:Type>xs:string</v8:Type></Type>");
    let native = source
        .to_xml()
        .unwrap()
        .replace("<Attribute ", "<StyleItem ")
        .replace("</Attribute>", "</StyleItem>");
    let style = Descriptor::from_xml(&native).unwrap();
    assert!(style.object.property(PropertyName::Type).is_none());
    assert_eq!(native, style.to_xml().unwrap());
    let alias = source
        .to_xml()
        .unwrap()
        .replace("xmlns:v8=", "xmlns:core=")
        .replace("v8:Type", "core:Type");
    let aliased = Descriptor::from_xml(&alias).unwrap();
    let Some(PropertyValue::TypeDescription(value)) = aliased.object.property(PropertyName::Type)
    else {
        panic!("aliased description")
    };
    assert_eq!(value.native.prefix, "core");
    assert_eq!(alias, aliased.to_xml().unwrap());
}
