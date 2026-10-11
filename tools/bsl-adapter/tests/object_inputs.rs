//! Object drafts share the project controller and preserve the identities of nested fields.

mod support;

use std::fs;

use bsl_metadata::AttributeType;
use eska::{
    analysis_core::{InputRequest, InputState, MetadataInputs},
    project_core::source_format::import_designer,
};
use eska_bsl_adapter::{AdapterIssue, metadata_object};
use support::TestDirectory;

/// A draft changes catalogue settings and a tabular type without affecting disk or retained models.
#[test]
fn object_and_tabular_drafts_have_current_types_and_exact_yaml_failures() {
    let directory = TestDirectory::new();
    let native = directory.0.join("native");
    fs::create_dir(&native).unwrap();
    fs::create_dir(native.join("Catalogs")).unwrap();
    fs::write(native.join("Configuration.xml"), r#"<MetaDataObject xmlns="http://v8.1c.ru/8.3/MDClasses" version="2.20"><Configuration uuid="10000000-0000-4000-8000-000000000001"><Properties><Name>ObjectProbe</Name></Properties><ChildObjects><Catalog>Items</Catalog></ChildObjects></Configuration></MetaDataObject>"#).unwrap();
    let xml = r#"<MetaDataObject xmlns="http://v8.1c.ru/8.3/MDClasses" xmlns:v8="http://v8.1c.ru/8.1/data/core" xmlns:xs="http://www.w3.org/2001/XMLSchema" version="2.20"><Catalog uuid="20000000-0000-4000-8000-000000000001"><Properties><Name>Items</Name><CodeLength>10</CodeLength><CodeType>String</CodeType></Properties><ChildObjects><TabularSection uuid="30000000-0000-4000-8000-000000000001"><Properties><Name>Rows</Name><Synonym><v8:item><v8:lang>ru</v8:lang><v8:content>Строки</v8:content></v8:item></Synonym></Properties><ChildObjects><Attribute uuid="40000000-0000-4000-8000-000000000001"><Properties><Name>Value</Name><Type><v8:Type>xs:string</v8:Type></Type></Properties></Attribute></ChildObjects></TabularSection></ChildObjects></Catalog></MetaDataObject>"#;
    fs::write(native.join("Catalogs/Items.xml"), xml).unwrap();
    let source = directory.0.join("source");
    import_designer(&native, &source).unwrap();
    let mut inputs = MetadataInputs::open(&source).unwrap();
    let path = inputs
        .owner_paths()
        .find(|path| path.as_str().starts_with("Catalogs/Items/"))
        .unwrap()
        .clone();
    let request = InputRequest::default();
    inputs
        .load_saved(&path, inputs.revision(), &request)
        .unwrap();
    let InputState::Ready(saved) = &inputs.owner(&path).unwrap().state else {
        panic!("expected saved model");
    };
    let saved = std::sync::Arc::clone(saved);
    let saved_model = metadata_object(&saved.documents[0].descriptor, Vec::new()).unwrap();
    let text = fs::read_to_string(source.join(path.as_path())).unwrap();
    let draft = text
        .replace("<CodeType>String</CodeType>", "<CodeType>Number</CodeType>")
        .replace("xs:string", "xs:boolean");
    assert_ne!(draft, text);
    inputs
        .apply_overlay(&path, inputs.revision(), 1, &draft, &request)
        .unwrap();
    let InputState::Ready(changed) = &inputs.owner(&path).unwrap().state else {
        panic!("expected current draft model");
    };
    let model = metadata_object(&changed.documents[0].descriptor, Vec::new()).unwrap();
    assert!(model.attributes.iter().any(|field| {
        field.name == "Код"
            && field.attr_type
                == AttributeType::Number {
                    precision: 10,
                    scale: 0,
                }
    }));
    assert_eq!(
        model.tabular_sections[0].attributes()[0].attr_type(),
        &AttributeType::Boolean
    );
    assert_eq!(
        model.tabular_sections[0].uuid(),
        saved_model.tabular_sections[0].uuid()
    );
    assert_eq!(
        model.tabular_sections[0].attributes()[0].uuid(),
        saved_model.tabular_sections[0].attributes()[0].uuid()
    );
    assert_eq!(
        saved_model.tabular_sections[0].attributes()[0].attr_type(),
        &AttributeType::String { length: None }
    );
    assert_eq!(model.tabular_sections[0].synonym(), Some("Строки"));
    let invalid = text.replace(
        "<CodeLength>10</CodeLength>",
        "<CodeLength>-10</CodeLength>",
    );
    inputs
        .apply_overlay(&path, inputs.revision(), 2, &invalid, &request)
        .unwrap();
    let InputState::Ready(parsed) = &inputs.owner(&path).unwrap().state else {
        panic!("expected a parsed source whose conversion fails");
    };
    let error = metadata_object(&parsed.documents[0].descriptor, Vec::new()).unwrap_err();
    assert_eq!(
        error.issue,
        AdapterIssue::InvalidProperty {
            name: "CodeLength".into()
        }
    );
    assert!(invalid[error.range].contains("<CodeLength>-10</CodeLength>"));
    inputs
        .close_overlay(&path, inputs.revision(), &request)
        .unwrap();
    let InputState::Ready(restored) = &inputs.owner(&path).unwrap().state else {
        panic!("expected restored saved source");
    };
    assert_eq!(restored, &saved);
    assert_eq!(
        fs::read_to_string(source.join(path.as_path())).unwrap(),
        text
    );
    assert!(!source.join("Catalogs/Items.xml").exists());
}
