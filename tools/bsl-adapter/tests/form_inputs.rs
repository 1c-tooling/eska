//! Current YAML and explicit dialog input share one adapter without exporting metadata XML.

mod support;

use std::fs;

use eska::{
    analysis_core::{InputRequest, InputState, MetadataInputs},
    project_core::source_format::{SourcePath, import_designer},
};
use eska_bsl_adapter::form;
use support::TestDirectory;

const DECLARATION: &str = r#"<MetaDataObject xmlns="http://v8.1c.ru/8.3/MDClasses" version="2.20"><CommonForm uuid="20000000-0000-4000-8000-000000000001"><Properties><Name>Dialog</Name><Comment>Saved</Comment><FormType>Managed</FormType></Properties></CommonForm></MetaDataObject>"#;
const DIALOG: &str = r#"<Form xmlns="http://v8.1c.ru/8.3/xcf/logform"><Events><Event name="OnOpen">SavedOpen</Event></Events><ChildItems><InputField name="Value" id="1"/></ChildItems><Commands><Command name="Run"><Action>SavedAction</Action></Command></Commands></Form>"#;

/// The host supplies a parsed resource only after obtaining a current ready declaration.
fn current_form(inputs: &MetadataInputs, path: &SourcePath, dialog: &str) -> bsl_metadata::Form {
    let InputState::Ready(snapshot) = &inputs.owner(path).unwrap().state else {
        panic!("expected current form declaration");
    };
    let data = bsl_metadata::xml_parser::parse_form_xml(dialog).unwrap();
    form(&snapshot.documents[0].descriptor, data).unwrap()
}

/// Overlay edits cannot rewrite the saved resource or recover a model from invalid YAML.
#[test]
fn imported_form_yaml_matches_native_and_preserves_resource_bytes() {
    let directory = TestDirectory::new();
    let native = directory.0.join("native");
    fs::create_dir_all(native.join("CommonForms/Dialog/Ext/Form")).unwrap();
    fs::write(native.join("Configuration.xml"), r#"<MetaDataObject xmlns="http://v8.1c.ru/8.3/MDClasses" version="2.20"><Configuration uuid="10000000-0000-4000-8000-000000000001"><Properties><Name>FormProbe</Name></Properties><ChildObjects><CommonForm>Dialog</CommonForm></ChildObjects></Configuration></MetaDataObject>"#).unwrap();
    fs::write(native.join("CommonForms/Dialog.xml"), DECLARATION).unwrap();
    let dialog = format!("\u{feff}{}\r\n", DIALOG.replace('>', ">\r\n"));
    fs::write(native.join("CommonForms/Dialog/Ext/Form.xml"), &dialog).unwrap();
    let module_path = "CommonForms/Dialog/Ext/Form/Module.bsl";
    let module = "\u{feff}Процедура SavedOpen()\r\nКонецПроцедуры\r\n";
    fs::write(native.join(module_path), module).unwrap();
    let source = directory.0.join("source");
    import_designer(&native, &source).unwrap();
    let path = SourcePath::parse("CommonForms/Dialog/object.yaml").unwrap();
    let mut inputs = MetadataInputs::open(&source).unwrap();
    let request = InputRequest::default();
    inputs
        .load_saved(&path, inputs.revision(), &request)
        .unwrap();
    let saved = current_form(&inputs, &path, &dialog);
    assert_eq!(
        saved,
        bsl_metadata::xml_parser::parse_form_from_bsl_path(&native.join(module_path)).unwrap()
    );
    let text = fs::read_to_string(source.join(path.as_path())).unwrap();
    let draft = text.replace("\"value\": \"Saved\"", "\"value\": \"Current\"");
    assert_ne!(draft, text);
    inputs
        .apply_overlay(&path, inputs.revision(), 1, &draft, &request)
        .unwrap();
    assert_eq!(current_form(&inputs, &path, &dialog), saved);
    let changed_dialog = dialog.replace("SavedAction", "CurrentAction");
    let current = current_form(&inputs, &path, &changed_dialog);
    assert_eq!(saved.uuid(), current.uuid());
    assert_eq!(saved.command_handlers, ["SavedAction"]);
    assert_eq!(current.command_handlers, ["CurrentAction"]);
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
    assert_eq!(current_form(&inputs, &path, &dialog), saved);
    assert_eq!(
        fs::read_to_string(source.join(path.as_path())).unwrap(),
        text
    );
    assert_eq!(
        fs::read_to_string(source.join("CommonForms/Dialog/Ext/Form.xml")).unwrap(),
        dialog
    );
    assert_eq!(
        fs::read_to_string(source.join(module_path)).unwrap(),
        module
    );
    assert!(!source.join("CommonForms/Dialog.xml").exists());
    assert!(!source.join("Configuration.xml").exists());
}
