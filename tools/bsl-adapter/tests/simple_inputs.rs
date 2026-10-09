//! Primitive metadata drafts use the same YAML lifecycle and never modify the saved source.

mod support;

use std::{fs, path::PathBuf, sync::Arc};

use eska::{
    analysis_core::{InputRequest, InputState, MetadataInputs},
    project_core::source_format::{MetadataSnapshot, SourcePath, import_designer},
};
use eska_bsl_adapter::{AdapterIssue, constant_object, enum_object};
use support::TestDirectory;

/// Create an isolated pair of native descriptors without touching an existing playground project.
fn primitive_source(directory: &TestDirectory) -> PathBuf {
    let native = directory.0.join("native");
    fs::create_dir(&native).unwrap();
    fs::create_dir(native.join("Constants")).unwrap();
    fs::create_dir(native.join("Enums")).unwrap();
    fs::write(native.join("Configuration.xml"), r#"<MetaDataObject xmlns="http://v8.1c.ru/8.3/MDClasses" version="2.20"><Configuration uuid="10000000-0000-4000-8000-000000000001"><Properties><Name>PrimitiveProbe</Name></Properties><ChildObjects><Constant>Amount</Constant><Enum>State</Enum></ChildObjects></Configuration></MetaDataObject>"#).unwrap();
    fs::write(native.join("Constants/Amount.xml"), r#"<MetaDataObject xmlns="http://v8.1c.ru/8.3/MDClasses" xmlns:v8="http://v8.1c.ru/8.1/data/core" xmlns:xs="http://www.w3.org/2001/XMLSchema" version="2.20"><Constant uuid="20000000-0000-4000-8000-000000000001"><Properties><Name>Amount</Name><Type><v8:Type>xs:string</v8:Type></Type></Properties></Constant></MetaDataObject>"#).unwrap();
    fs::write(native.join("Enums/State.xml"), r#"<MetaDataObject xmlns="http://v8.1c.ru/8.3/MDClasses" version="2.20"><Enum uuid="30000000-0000-4000-8000-000000000001"><Properties><Name>State</Name></Properties><ChildObjects><EnumValue uuid="40000000-0000-4000-8000-000000000001"><Properties><Name>Ready</Name></Properties></EnumValue><EnumValue uuid="50000000-0000-4000-8000-000000000001"><Properties><Name>Pending</Name></Properties></EnumValue></ChildObjects></Enum></MetaDataObject>"#).unwrap();
    let source = directory.0.join("source");
    import_designer(&native, &source).unwrap();
    source
}

/// Require the current revision rather than a previously retained successful model.
fn ready(inputs: &MetadataInputs, path: &SourcePath) -> Arc<MetadataSnapshot> {
    let InputState::Ready(value) = &inputs.owner(path).unwrap().state else {
        panic!("expected ready snapshot");
    };
    Arc::clone(value)
}

/// A constant draft changes its actual type; unsupported `QName` reports the exact YAML span.
#[test]
fn constant_drafts_use_current_types_and_locate_unsupported_values() {
    let directory = TestDirectory::new();
    let source = primitive_source(&directory);
    let mut inputs = MetadataInputs::open(&source).unwrap();
    let path = inputs
        .owner_paths()
        .find(|path| path.as_str().starts_with("Constants/Amount/"))
        .unwrap()
        .clone();
    let request = InputRequest::default();
    inputs
        .load_saved(&path, inputs.revision(), &request)
        .unwrap();
    let saved = ready(&inputs, &path);
    let saved_model = constant_object(&saved.documents[0].descriptor).unwrap();
    let text = fs::read_to_string(source.join(path.as_path())).unwrap();
    let draft = text.replace("xs:string", "xs:boolean");
    assert_ne!(draft, text);
    inputs
        .apply_overlay(&path, inputs.revision(), 1, &draft, &request)
        .unwrap();
    let changed = ready(&inputs, &path);
    let model = constant_object(&changed.documents[0].descriptor).unwrap();
    assert_eq!(
        model.constant_type,
        Some(bsl_metadata::AttributeType::Boolean)
    );
    assert_eq!(model.uuid, saved_model.uuid);
    assert_ne!(model.constant_type, saved_model.constant_type);
    let invalid = text.replace("xs:string", "xs:future");
    inputs
        .apply_overlay(&path, inputs.revision(), 2, &invalid, &request)
        .unwrap();
    let snapshot = ready(&inputs, &path);
    let error = constant_object(&snapshot.documents[0].descriptor).unwrap_err();
    assert!(matches!(error.issue, AdapterIssue::UnsupportedType { .. }));
    assert!(invalid[error.range].contains("xs:future"));
    inputs
        .close_overlay(&path, inputs.revision(), &request)
        .unwrap();
    assert_eq!(ready(&inputs, &path), saved);
    assert_eq!(
        fs::read_to_string(source.join(path.as_path())).unwrap(),
        text
    );
    assert!(!source.join("Constants/Amount.xml").exists());
}

/// Enum value drafts preserve declaration identity/order and do not mutate the retained snapshot.
#[test]
fn enum_drafts_preserve_identity_and_invalid_yaml_has_no_current_model() {
    let directory = TestDirectory::new();
    let source = primitive_source(&directory);
    let mut inputs = MetadataInputs::open(&source).unwrap();
    let path = inputs
        .owner_paths()
        .find(|path| path.as_str().starts_with("Enums/State/"))
        .unwrap()
        .clone();
    let request = InputRequest::default();
    inputs
        .load_saved(&path, inputs.revision(), &request)
        .unwrap();
    let saved = ready(&inputs, &path);
    let saved_model = enum_object(&saved.documents[0].descriptor).unwrap();
    let text = fs::read_to_string(source.join(path.as_path())).unwrap();
    let draft = text.replace("\"value\": \"Ready\"", "\"value\": \"Complete\"");
    assert_ne!(draft, text);
    inputs
        .apply_overlay(&path, inputs.revision(), 1, &draft, &request)
        .unwrap();
    let changed = ready(&inputs, &path);
    let model = enum_object(&changed.documents[0].descriptor).unwrap();
    assert_eq!(model.enum_values[0].name, "Complete");
    assert_eq!(model.enum_values[1].name, "Pending");
    assert_eq!(model.enum_values[0].uuid, saved_model.enum_values[0].uuid);
    assert_eq!(saved_model.enum_values[0].name, "Ready");
    inputs
        .apply_overlay(&path, inputs.revision(), 2, "schema_version: [", &request)
        .unwrap();
    assert!(matches!(
        inputs.owner(&path).unwrap().state,
        InputState::Unavailable(_)
    ));
    inputs
        .close_overlay(&path, inputs.revision(), &request)
        .unwrap();
    assert_eq!(ready(&inputs, &path), saved);
    assert_eq!(
        fs::read_to_string(source.join(path.as_path())).unwrap(),
        text
    );
    assert!(!source.join("Enums/State.xml").exists());
}
