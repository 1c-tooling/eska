//! Boundary tests complement the independent process client with exhaustive DTO variants.

use super::{dto, params};
use crate::project::{
    metadata_model::{
        CollectionKind, LocalizedText, MetadataKind, MetadataObject, MetadataProperty,
        MetadataValue, ModuleRole, NodeId, PropertyKey, ValueIssue,
    },
    metadata_parser::LocatedProperty,
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
        let value = dto::property(&LocatedProperty {
            property: MetadataProperty {
                key: key.clone(),
                qualifiers: vec![],
                value,
            },
            range: 3..25,
        });
        assert_eq!(value["value"]["kind"], kind);
        assert_eq!(value["range"], json!({"start":3,"end":25}));
        if kind == "record" {
            assert_eq!(value["value"]["fields"].as_array().unwrap().len(), 2);
            assert_eq!(value["value"]["fields"][0]["qualifiers"][0]["value"], "x");
        }
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
