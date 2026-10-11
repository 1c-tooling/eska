//! The canonical role identity comes from current YAML; rights remain a separate explicit input.

mod support;

use std::fs;

use eska::{
    analysis_core::{InputRequest, InputState, MetadataInputs},
    project_core::source_format::{SourcePath, import_designer},
};
use eska_bsl_adapter::role;
use support::TestDirectory;

const ROLE: &str = r#"<MetaDataObject xmlns="http://v8.1c.ru/8.3/MDClasses" version="2.20"><Role uuid="20000000-0000-4000-8000-000000000001"><Properties><Name>Reader</Name><Comment>Saved</Comment></Properties></Role></MetaDataObject>"#;
const RIGHTS: &str = r#"<Rights xmlns="http://v8.1c.ru/8.2/roles" version="2.20"><setForNewObjects>true</setForNewObjects><object><name>Catalog.Items</name><right><name>Read</name><value>true</value></right></object></Rights>"#;

/// Only current ready descriptors and explicitly parsed rights can enter the adapter.
fn current_role(inputs: &MetadataInputs, path: &SourcePath, rights: &str) -> bsl_metadata::Role {
    let InputState::Ready(snapshot) = &inputs.owner(path).unwrap().state else {
        panic!("expected current role descriptor");
    };
    let data = bsl_metadata::xml_parser::parse_rights_xml(rights).unwrap();
    role(&snapshot.documents[0].descriptor, data).unwrap()
}

/// Imported rights bytes stay preserved while saved/editor YAML and changed rights share one adapter.
#[test]
fn imported_role_yaml_combines_current_identity_and_supplied_rights_without_xml_export() {
    let directory = TestDirectory::new();
    let native = directory.0.join("native");
    fs::create_dir_all(native.join("Roles/Reader/Ext")).unwrap();
    fs::write(native.join("Configuration.xml"), r#"<MetaDataObject xmlns="http://v8.1c.ru/8.3/MDClasses" version="2.20"><Configuration uuid="10000000-0000-4000-8000-000000000001"><Properties><Name>RoleProbe</Name></Properties><ChildObjects><Role>Reader</Role></ChildObjects></Configuration></MetaDataObject>"#).unwrap();
    fs::write(native.join("Roles/Reader.xml"), ROLE).unwrap();
    fs::write(native.join("Roles/Reader/Ext/Rights.xml"), RIGHTS).unwrap();
    let source = directory.0.join("source");
    import_designer(&native, &source).unwrap();
    let path = SourcePath::parse("Roles/Reader/object.yaml").unwrap();
    let mut inputs = MetadataInputs::open(&source).unwrap();
    let request = InputRequest::default();
    inputs
        .load_saved(&path, inputs.revision(), &request)
        .unwrap();
    let saved = current_role(&inputs, &path, RIGHTS);
    assert_eq!(
        saved,
        bsl_metadata::parse_role_from_texts(ROLE, Some(RIGHTS)).unwrap()
    );
    let text = fs::read_to_string(source.join(path.as_path())).unwrap();
    let draft = text.replace("\"value\": \"Saved\"", "\"value\": \"Current\"");
    assert_ne!(draft, text);
    inputs
        .apply_overlay(&path, inputs.revision(), 1, &draft, &request)
        .unwrap();
    assert_eq!(current_role(&inputs, &path, RIGHTS), saved);
    let changed_rights = RIGHTS.replace("Catalog.Items", "Document.Current");
    let current = current_role(&inputs, &path, &changed_rights);
    assert_eq!(saved.uuid(), current.uuid());
    assert_eq!(saved.objects()[0].name, "Items");
    assert_eq!(current.objects()[0].name, "Current");
    inputs
        .apply_overlay(&path, inputs.revision(), 2, "document: [", &request)
        .unwrap();
    assert!(matches!(
        inputs.owner(&path).unwrap().state,
        InputState::Unavailable(_)
    ));
    inputs
        .close_overlay(&path, inputs.revision(), &request)
        .unwrap();
    assert_eq!(current_role(&inputs, &path, RIGHTS), saved);
    assert_eq!(
        fs::read_to_string(source.join(path.as_path())).unwrap(),
        text
    );
    assert_eq!(
        fs::read_to_string(source.join("Roles/Reader/Ext/Rights.xml")).unwrap(),
        RIGHTS
    );
    assert!(!source.join("Roles/Reader.xml").exists());
    assert!(!source.join("Configuration.xml").exists());
}
