//! Saved and unsaved YAML enter the same converter through the real project input controller.

mod support;

use std::{fs, sync::Arc};

use eska::{
    analysis_core::{InputRequest, InputState, MetadataInputs},
    project_core::source_format::{MetadataSnapshot, SourcePath, import_designer},
};
use eska_bsl_adapter::{AdapterIssue, common_module, defined_type, register};
use support::TestDirectory;

/// Only the current ready snapshot may feed the analyzer; a prior value is never substituted.
fn ready(inputs: &MetadataInputs, path: &SourcePath) -> Arc<MetadataSnapshot> {
    let InputState::Ready(value) = &inputs.owner(path).unwrap().state else {
        panic!("expected ready snapshot");
    };
    Arc::clone(value)
}

/// Exercise equality, context changes, exact error locations and restoration without XML export.
#[test]
fn module_drafts_share_saved_semantics_and_invalid_inputs_have_no_stale_model() {
    let directory = TestDirectory::new();
    let source = directory.0.join("source");
    let corpus = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/eska-format/designer");
    import_designer(&corpus, &source).unwrap();
    let mut inputs = MetadataInputs::open(&source).unwrap();
    let path = inputs
        .owner_paths()
        .find(|path| path.as_str().starts_with("CommonModules/Tools/"))
        .unwrap()
        .clone();
    let request = InputRequest::default();
    inputs
        .load_saved(&path, inputs.revision(), &request)
        .unwrap();
    let saved = ready(&inputs, &path);
    let saved_model = common_module(&saved.documents[0].descriptor.objects[0]).unwrap();
    let native = fs::read_to_string(corpus.join("CommonModules/Tools.xml")).unwrap();
    assert_eq!(
        saved_model,
        bsl_metadata::xml_parser::parse_common_module_xml(&native).unwrap()
    );
    let text = fs::read_to_string(source.join(path.as_path())).unwrap();
    inputs
        .apply_overlay(&path, inputs.revision(), 1, &text, &request)
        .unwrap();
    let same = ready(&inputs, &path);
    assert_eq!(same, saved);
    assert_eq!(
        common_module(&same.documents[0].descriptor.objects[0]).unwrap(),
        saved_model
    );

    assert_eq!(text.matches("<Server>true</Server>").count(), 1);
    let draft = text.replace("<Server>true</Server>", "<Server>false</Server>");
    inputs
        .apply_overlay(&path, inputs.revision(), 2, &draft, &request)
        .unwrap();
    let changed = ready(&inputs, &path);
    let model = common_module(&changed.documents[0].descriptor.objects[0]).unwrap();
    assert!(!model.is_server());
    assert!(saved_model.is_server());
    assert_ne!(changed.source_sha256, saved.source_sha256);

    let invalid = text.replace("<Server>true</Server>", "<Server>TRUE</Server>");
    inputs
        .apply_overlay(&path, inputs.revision(), 3, &invalid, &request)
        .unwrap();
    let parsed = ready(&inputs, &path);
    let error = common_module(&parsed.documents[0].descriptor.objects[0]).unwrap_err();
    assert_eq!(
        error.issue,
        AdapterIssue::InvalidProperty {
            name: "Server".into()
        }
    );
    assert!(invalid[error.range].contains("<Server>TRUE</Server>"));

    inputs
        .apply_overlay(&path, inputs.revision(), 4, "document: [", &request)
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
    assert!(!source.join("Configuration.xml").exists());
    assert!(!source.join("CommonModules/Tools.xml").exists());
}

/// Defined-type edits use exact YAML coordinates and restore the same saved canonical value.
#[test]
fn defined_type_drafts_change_underlying_type_and_report_unsupported_values_in_yaml() {
    let directory = TestDirectory::new();
    let native = directory.0.join("native");
    fs::create_dir(&native).unwrap();
    fs::create_dir(native.join("DefinedTypes")).unwrap();
    fs::write(native.join("Configuration.xml"), r#"<MetaDataObject xmlns="http://v8.1c.ru/8.3/MDClasses" version="2.20"><Configuration uuid="10000000-0000-4000-8000-000000000001"><Properties><Name>TypeProbe</Name></Properties><ChildObjects><DefinedType>Value</DefinedType></ChildObjects></Configuration></MetaDataObject>"#).unwrap();
    let xml = r#"<MetaDataObject xmlns="http://v8.1c.ru/8.3/MDClasses" xmlns:v8="http://v8.1c.ru/8.1/data/core" xmlns:xs="http://www.w3.org/2001/XMLSchema" xmlns:cfg="http://v8.1c.ru/8.1/data/enterprise/current-config" version="2.20"><DefinedType uuid="20000000-0000-4000-8000-000000000001"><Properties><Name>Value</Name><Type><v8:Type>xs:decimal</v8:Type><v8:NumberQualifiers><v8:Digits>15</v8:Digits><v8:FractionDigits>2</v8:FractionDigits></v8:NumberQualifiers></Type></Properties></DefinedType></MetaDataObject>"#;
    fs::write(native.join("DefinedTypes/Value.xml"), xml).unwrap();
    let source = directory.0.join("source");
    import_designer(&native, &source).unwrap();
    let mut inputs = MetadataInputs::open(&source).unwrap();
    let path = inputs
        .owner_paths()
        .find(|path| path.as_str().starts_with("DefinedTypes/Value/"))
        .unwrap()
        .clone();
    let request = InputRequest::default();
    inputs
        .load_saved(&path, inputs.revision(), &request)
        .unwrap();
    let saved = ready(&inputs, &path);
    let saved_model = defined_type(&saved.documents[0].descriptor.objects[0]).unwrap();
    assert_eq!(
        saved_model,
        bsl_metadata::xml_parser::parse_defined_type_xml(xml).unwrap()
    );
    let text = fs::read_to_string(source.join(path.as_path())).unwrap();
    inputs
        .apply_overlay(&path, inputs.revision(), 1, &text, &request)
        .unwrap();
    assert_eq!(ready(&inputs, &path), saved);
    let draft = text.replace(
        "<v8:Type>xs:decimal</v8:Type>",
        "<v8:Type>xs:boolean</v8:Type>",
    );
    assert_ne!(draft, text);
    inputs
        .apply_overlay(&path, inputs.revision(), 2, &draft, &request)
        .unwrap();
    let changed = ready(&inputs, &path);
    assert_eq!(
        defined_type(&changed.documents[0].descriptor.objects[0])
            .unwrap()
            .underlying_type(),
        &bsl_metadata::AttributeType::Boolean
    );
    assert_eq!(
        saved_model.underlying_type(),
        &bsl_metadata::AttributeType::Number {
            precision: 15,
            scale: 2
        }
    );
    let unsupported = text.replace(
        "<v8:Type>xs:decimal</v8:Type>",
        "<v8:Type>cfg:Future.Value</v8:Type>",
    );
    inputs
        .apply_overlay(&path, inputs.revision(), 3, &unsupported, &request)
        .unwrap();
    let snapshot = ready(&inputs, &path);
    let error = defined_type(&snapshot.documents[0].descriptor.objects[0]).unwrap_err();
    assert!(matches!(error.issue, AdapterIssue::UnsupportedType { .. }));
    assert!(unsupported[error.range].contains("cfg:Future.Value"));
    inputs
        .close_overlay(&path, inputs.revision(), &request)
        .unwrap();
    assert_eq!(ready(&inputs, &path), saved);
    assert_eq!(
        fs::read_to_string(source.join(path.as_path())).unwrap(),
        text
    );
    assert!(!source.join("Configuration.xml").exists());
    assert!(!source.join("DefinedTypes/Value.xml").exists());
}

/// Register fields use the same YAML snapshot as modes, while prior model holders keep their value.
#[test]
fn register_yaml_and_drafts_preserve_types_and_address_invalid_modes() {
    let directory = TestDirectory::new();
    let native = directory.0.join("native");
    fs::create_dir(&native).unwrap();
    fs::create_dir(native.join("InformationRegisters")).unwrap();
    fs::write(native.join("Configuration.xml"), r#"<MetaDataObject xmlns="http://v8.1c.ru/8.3/MDClasses" version="2.20"><Configuration uuid="10000000-0000-4000-8000-000000000001"><Properties><Name>RegisterProbe</Name></Properties><ChildObjects><InformationRegister>Rates</InformationRegister></ChildObjects></Configuration></MetaDataObject>"#).unwrap();
    let xml = r#"<MetaDataObject xmlns="http://v8.1c.ru/8.3/MDClasses" xmlns:v8="http://v8.1c.ru/8.1/data/core" xmlns:xs="http://www.w3.org/2001/XMLSchema" xmlns:cfg="http://v8.1c.ru/8.1/data/enterprise/current-config" version="2.20"><InformationRegister uuid="20000000-0000-4000-8000-000000000001"><Properties><Name>Rates</Name><InformationRegisterPeriodicity>Month</InformationRegisterPeriodicity><WriteMode>Independent</WriteMode></Properties><ChildObjects><Dimension uuid="30000000-0000-4000-8000-000000000001"><Properties><Name>Item</Name><Type><v8:Type>cfg:CatalogRef.Items</v8:Type></Type></Properties></Dimension></ChildObjects></InformationRegister></MetaDataObject>"#;
    fs::write(native.join("InformationRegisters/Rates.xml"), xml).unwrap();
    let source = directory.0.join("source");
    import_designer(&native, &source).unwrap();
    let mut inputs = MetadataInputs::open(&source).unwrap();
    let path = inputs
        .owner_paths()
        .find(|path| path.as_str().starts_with("InformationRegisters/Rates/"))
        .unwrap()
        .clone();
    let request = InputRequest::default();
    inputs
        .load_saved(&path, inputs.revision(), &request)
        .unwrap();
    let saved = ready(&inputs, &path);
    let saved_model = register(&saved.documents[0].descriptor).unwrap();
    assert_eq!(
        saved_model,
        bsl_metadata::xml_parser::parse_information_register_xml(xml).unwrap()
    );
    let text = fs::read_to_string(source.join(path.as_path())).unwrap();
    let draft = text
        .replace(
            "<v8:Type>cfg:CatalogRef.Items</v8:Type>",
            "<v8:Type>xs:boolean</v8:Type>",
        )
        .replace(
            "<WriteMode>Independent</WriteMode>",
            "<WriteMode>RecorderSubordinate</WriteMode>",
        );
    assert_ne!(draft, text);
    inputs
        .apply_overlay(&path, inputs.revision(), 1, &draft, &request)
        .unwrap();
    let changed = ready(&inputs, &path);
    let model = register(&changed.documents[0].descriptor).unwrap();
    assert!(model.is_recorder_subordinate());
    assert_eq!(
        model.dimensions()[0].attr_type(),
        Some(&bsl_metadata::AttributeType::Boolean)
    );
    assert!(!saved_model.is_recorder_subordinate());
    assert_eq!(
        model.dimensions()[0].uuid(),
        saved_model.dimensions()[0].uuid()
    );
    let invalid = text.replace(
        "<WriteMode>Independent</WriteMode>",
        "<WriteMode>Future</WriteMode>",
    );
    inputs
        .apply_overlay(&path, inputs.revision(), 2, &invalid, &request)
        .unwrap();
    let snapshot = ready(&inputs, &path);
    let error = register(&snapshot.documents[0].descriptor).unwrap_err();
    assert_eq!(
        error.issue,
        AdapterIssue::InvalidProperty {
            name: "WriteMode".into()
        }
    );
    assert!(invalid[error.range].contains("<WriteMode>Future</WriteMode>"));
    inputs
        .close_overlay(&path, inputs.revision(), &request)
        .unwrap();
    assert_eq!(ready(&inputs, &path), saved);
    assert_eq!(
        fs::read_to_string(source.join(path.as_path())).unwrap(),
        text
    );
    assert!(!source.join("Configuration.xml").exists());
}
