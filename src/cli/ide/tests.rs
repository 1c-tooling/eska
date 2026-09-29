//! Boundary tests complement the independent process client with exhaustive DTO variants.

use super::{dto, params};
use crate::project::{
    metadata_model::{
        CollectionKind, LocalizedText, MetadataKind, MetadataObject, MetadataProperty,
        MetadataValue, ModuleRole, NodeId, PropertyKey, ValueIssue,
    },
    metadata_parser::{LocatedProperty, PropertiesMode, parse},
};
use serde_json::json;

/// Every kind, collection and module role round-trips through the public wire vocabulary.
#[test]
fn node_dtos_round_trip_all_kinds_and_roles() {
    let owner = MetadataObject::new(
        MetadataKind::Catalog,
        "Проценты%/Раздел".into(),
        String::new(),
        None,
    )
    .unwrap()
    .id()
    .clone();
    let mut nodes = vec![NodeId::Object(owner.clone())];
    for kind in MetadataKind::ALL {
        nodes.push(NodeId::Collection {
            owner: owner.clone(),
            kind: CollectionKind::Metadata(*kind),
        });
    }
    for kind in [
        CollectionKind::Common,
        CollectionKind::Modules,
        CollectionKind::Unsupported,
    ] {
        nodes.push(NodeId::Collection {
            owner: owner.clone(),
            kind,
        });
    }
    for role in [
        ModuleRole::Module,
        ModuleRole::Object,
        ModuleRole::Manager,
        ModuleRole::RecordSet,
        ModuleRole::ValueManager,
        ModuleRole::ManagedApplication,
        ModuleRole::OrdinaryApplication,
        ModuleRole::Session,
        ModuleRole::ExternalConnection,
        ModuleRole::Command,
    ] {
        nodes.push(NodeId::Module {
            owner: owner.clone(),
            role,
        });
    }
    for node in nodes {
        assert_eq!(params::node(&dto::node_id(&node)).unwrap(), node);
    }
    assert!(params::node(&json!({"kind":"module","owner":owner,"role":"Object"})).is_err());
}

/// Ordered records preserve repeated names, namespaced qualifiers and unsupported values.
#[test]
fn property_dtos_preserve_all_value_variants() {
    let key = PropertyKey {
        namespace: Some("urn:test".into()),
        name: "Value".into(),
    };
    let field = MetadataProperty {
        key: key.clone(),
        qualifiers: vec![(key.clone(), "x".into())],
        value: MetadataValue::Text("\r\nПривет".into()),
    };
    let variants = [
        MetadataValue::Text("text".into()),
        MetadataValue::Localized(vec![LocalizedText {
            language: "ru".into(),
            content: "синоним".into(),
        }]),
        MetadataValue::Record(vec![field.clone(), field]),
        MetadataValue::Unsupported(ValueIssue::MixedContent),
        MetadataValue::Unsupported(ValueIssue::InvalidLocalizedText),
    ];
    for (value, kind) in
        variants
            .into_iter()
            .zip(["text", "localized", "record", "unsupported", "unsupported"])
    {
        let value = dto::property(
            &dto::Labels::new().unwrap(),
            MetadataKind::Catalog,
            &LocatedProperty {
                property: MetadataProperty {
                    key: key.clone(),
                    qualifiers: vec![],
                    value,
                },
                range: 3..25,
            },
        );
        assert_eq!(value["value"]["kind"], kind);
        assert_eq!(value["range"], json!({"start":3,"end":25}));
        if kind == "record" {
            assert_eq!(value["value"]["fields"].as_array().unwrap().len(), 2);
            assert_eq!(value["value"]["fields"][0]["qualifiers"][0]["value"], "x");
        }
    }
}

/// Both captions are additive; nested XML names, unsupported namespaces and raw values survive.
#[test]
fn property_dtos_include_contextual_and_nested_captions() {
    let labels = dto::Labels::new().unwrap();
    let nested = MetadataProperty {
        key: PropertyKey {
            namespace: Some("http://v8.1c.ru/8.1/data/core".into()),
            name: "NumberQualifiers".into(),
        },
        qualifiers: vec![],
        value: MetadataValue::Text("raw".into()),
    };
    let mut value = LocatedProperty {
        property: MetadataProperty {
            key: PropertyKey {
                namespace: Some("http://v8.1c.ru/8.3/MDClasses".into()),
                name: "Type".into(),
            },
            qualifiers: vec![],
            value: MetadataValue::Record(vec![nested]),
        },
        range: 5..30,
    };
    let result = dto::property(&labels, MetadataKind::StyleItem, &value);
    assert_eq!(result["caption"], json!({"ru-RU":"Вид", "en-US":"Type"}));
    assert_eq!(result["key"]["name"], "Type");
    assert_eq!(result["range"], json!({"start":5,"end":30}));
    assert_eq!(
        result["value"]["fields"][0]["caption"]["en-US"],
        "Number qualifiers"
    );
    assert_eq!(result["value"]["fields"][0]["value"]["text"], "raw");
    value.property.key.name = "ClientApplicationTheme".into();
    let result = dto::property(&labels, MetadataKind::Configuration, &value);
    assert_eq!(result["caption"]["ru-RU"], "Тема клиентского приложения");
    value.property.key.namespace = Some("urn:future".into());
    let result = dto::property(&labels, MetadataKind::Configuration, &value);
    assert!(result.get("caption").is_none());
}

/// Captions are additive for enums, standard identities and namespace-resolved type names.
#[test]
fn property_value_captions_preserve_source_and_wire_shape() {
    let xml = r#"<MetaDataObject xmlns="http://v8.1c.ru/8.3/MDClasses"
        xmlns:r="http://v8.1c.ru/8.3/xcf/readable" xmlns:c="http://v8.1c.ru/8.1/data/core"
        xmlns:s="http://www.w3.org/2001/XMLSchema"><Configuration uuid="test"><Properties>
        <Name>Demo</Name><CompatibilityMode>Version8_3_27</CompatibilityMode>
        <Comment>Version8_3_27</Comment><StandardAttributes><r:StandardAttribute name="Code">
        <r:Type><c:Type>s:string</c:Type></r:Type></r:StandardAttribute></StandardAttributes>
        </Properties></Configuration></MetaDataObject>"#;
    let parsed = parse(xml, None, PropertiesMode::All).unwrap();
    let labels = dto::Labels::new().unwrap();
    let properties: Vec<_> = parsed.objects[0]
        .properties
        .as_ref()
        .unwrap()
        .iter()
        .map(|value| dto::property(&labels, MetadataKind::Configuration, value))
        .collect();
    assert_eq!(
        properties[1]["value"],
        json!({"kind":"text","text":"Version8_3_27",
        "caption":{"ru-RU":"Версия 8.3.27","en-US":"Version 8.3.27"}})
    );
    assert_eq!(
        properties[2]["value"],
        json!({"kind":"text","text":"Version8_3_27"})
    );
    let attribute = &properties[3]["value"]["fields"][0];
    assert_eq!(attribute["qualifiers"][0]["value"], "Code");
    assert_eq!(
        attribute["qualifiers"][0]["caption"],
        json!({"ru-RU":"Код","en-US":"Code"})
    );
    assert_eq!(
        attribute["value"]["fields"][0]["value"]["fields"][0]["value"],
        json!({"kind":"text","text":"s:string","caption":{"ru-RU":"Строка","en-US":"String"}})
    );
}

/// Only known boolean contexts get a checkbox hint; identical user strings stay raw.
#[test]
fn boolean_hints_preserve_text_and_respect_xml_context() {
    let xml = r#"<MetaDataObject xmlns="http://v8.1c.ru/8.3/MDClasses"
        xmlns:r="http://v8.1c.ru/8.3/xcf/readable"
        xmlns:a="http://v8.1c.ru/8.2/managed-application/core" xmlns:f="urn:foreign">
        <Catalog uuid="test"><Properties><Name>true</Name><Comment>false</Comment>
        <Hierarchical>true</Hierarchical><CheckUnique> 0 </CheckUnique><QuickChoice>false</QuickChoice>
        <f:Hierarchical>true</f:Hierarchical><Unknown>true</Unknown><ReadOnly>invalid</ReadOnly>
        <StandardAttributes><r:StandardAttribute name="Code"><r:MultiLine>false</r:MultiLine>
        <r:Name>true</r:Name></r:StandardAttribute></StandardAttributes>
        <UsedMobileApplicationFunctionalities><a:functionality><a:functionality>Biometrics</a:functionality>
        <a:use>1</a:use></a:functionality></UsedMobileApplicationFunctionalities>
        </Properties></Catalog></MetaDataObject>"#;
    let parsed = parse(xml, None, PropertiesMode::All).unwrap();
    let labels = dto::Labels::new().unwrap();
    let properties = parsed.objects[0].properties.as_ref().unwrap();
    let values: Vec<_> = properties
        .iter()
        .map(|value| dto::property(&labels, MetadataKind::Catalog, value)["value"].clone())
        .collect();
    for index in [0, 1, 5, 6, 7] {
        assert!(
            values[index].get("scalarType").is_none(),
            "{}",
            values[index]
        );
    }
    for index in [2, 3, 4] {
        assert_eq!(values[index]["scalarType"], "boolean");
    }
    assert_eq!(values[3]["text"], " 0 ");
    let nested = &values[8]["fields"][0]["value"]["fields"];
    assert_eq!(nested[0]["value"]["scalarType"], "boolean");
    assert!(nested[1]["value"].get("scalarType").is_none());
    assert_eq!(
        values[9]["fields"][0]["value"]["fields"][1]["value"]["scalarType"],
        "boolean"
    );
    // QuickChoice is a boolean for a catalog but an enum for an attribute.
    assert!(
        dto::property(&labels, MetadataKind::Attribute, &properties[4])["value"]
            .get("scalarType")
            .is_none()
    );
}

/// Filling values can be boolean or text despite having identical XML content.
#[test]
fn explicit_boolean_annotations_are_namespace_aware() {
    let xml = r#"<MetaDataObject xmlns="http://v8.1c.ru/8.3/MDClasses"
        xmlns:i="http://www.w3.org/2001/XMLSchema-instance" xmlns:s="http://www.w3.org/2001/XMLSchema">
        <Catalog uuid="test"><Properties><Name>Values</Name><FillValue i:type="s:boolean">false</FillValue>
        <FillValue i:type="s:string">false</FillValue><FillValue i:type="s:boolean" xmlns:s="urn:custom">false</FillValue>
        <Hierarchical i:type="s:string">true</Hierarchical><Mask>false</Mask>
        </Properties></Catalog></MetaDataObject>"#;
    let parsed = parse(xml, None, PropertiesMode::All).unwrap();
    let labels = dto::Labels::new().unwrap();
    for (index, value) in parsed.objects[0]
        .properties
        .as_ref()
        .unwrap()
        .iter()
        .enumerate()
    {
        let result = dto::property(&labels, MetadataKind::Catalog, value);
        assert_eq!(result["value"]["kind"], "text");
        assert_eq!(result["value"].get("scalarType").is_some(), index == 1);
        assert_eq!(
            result["value"]["text"],
            ["Values", "false", "false", "false", "true", "false"][index]
        );
    }
}

/// Valid Unicode percent signs stay literal; fallback bytes require complete native encoding.
#[test]
fn path_dtos_decode_reversibly() {
    let path = std::path::Path::new("src/Имя%20.xml");
    let decoded: params::Path = params::decode(&dto::path(path)).unwrap();
    assert_eq!(decoded.native().unwrap(), path);
    for value in [
        json!({"value":"%FFplain","encoding":"percent"}),
        json!({"value":"x","encoding":"uri"}),
    ] {
        assert!(
            params::decode::<params::Path>(&value)
                .and_then(|path| path.native())
                .is_err()
        );
    }
    #[cfg(unix)]
    {
        use std::{ffi::OsString, os::unix::ffi::OsStringExt};
        let path = std::path::PathBuf::from(OsString::from_vec(b"%\xff".to_vec()));
        assert_eq!(
            params::decode::<params::Path>(&dto::path(&path))
                .unwrap()
                .native()
                .unwrap(),
            path
        );
        assert!(
            params::decode::<params::Path>(&json!({"value":"%0041","encoding":"utf-16-percent"}))
                .unwrap()
                .native()
                .is_err()
        );
    }
}

/// Object presentation kinds are independent of IDs, parents, names and locale.
#[test]
fn tree_node_dto_exposes_authoritative_kind() {
    use crate::project::configurator::{ChildrenState, TreeLabel, TreeNode};
    let labels = dto::Labels::new().unwrap();
    for kind in MetadataKind::ALL {
        let object =
            MetadataObject::new(*kind, "ПроизвольноеИмя".into(), String::new(), None).unwrap();
        let mut node = TreeNode {
            id: NodeId::Object(object.id().clone()),
            metadata_kind: Some(*kind),
            parent: None,
            label: TreeLabel::Name("ДругоеИмя".into()),
            children: Vec::new(),
            state: ChildrenState::Unloaded,
            expanded_by_default: false,
            root_section: false,
            diagnostics: Vec::new(),
        };
        assert_eq!(labels.node(&node)["metadataKind"], kind.as_str());
        node.metadata_kind = None;
        assert!(labels.node(&node)["metadataKind"].is_null());
    }
}
