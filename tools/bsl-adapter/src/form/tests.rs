//! Declaration and dialog changes are independent, without stale identity or inferred payloads.

use super::*;
use eska::project_core::metadata_parser::{self, PropertiesMode};

const DECLARATION: &str = r#"<MetaDataObject xmlns="http://v8.1c.ru/8.3/MDClasses" version="2.20"><CommonForm uuid="10000000-0000-4000-8000-000000000001"><Properties><Name>Dialog</Name><FormType>Managed</FormType></Properties></CommonForm></MetaDataObject>"#;
const DIALOG: &str = r#"<Form xmlns="http://v8.1c.ru/8.3/xcf/logform" xmlns:xs="http://www.w3.org/2001/XMLSchema" uuid="20000000-0000-4000-8000-000000000002">
<Properties><Name>ResourceIdentity</Name></Properties><FormType>Ordinary</FormType>
<Events><Event name="OnOpen"> CurrentOpen </Event></Events>
<ChildItems><UsualGroup name="Group" id="1"><ChildItems><InputField name="Value" id="2"><DataPath>Object.Code</DataPath><Events><Event name="OnChange">CurrentChange</Event></Events></InputField><Table name="Rows" id="3"/></ChildItems></UsualGroup></ChildItems>
<Commands><Command name="First"><Action> CurrentAction </Action></Command><Command name="Second"><Action>CurrentAction</Action></Command></Commands>
<Attributes><Attribute name="Object"><Type><Type>xs:string</Type><StringQualifiers><Length>20</Length></StringQualifiers></Type><MainAttribute>true</MainAttribute><Columns><Column name="Amount"><Type><Type>xs:decimal</Type></Type></Column></Columns></Attribute></Attributes>
</Form>"#;

/// Parse the shared declaration independently of the native dialog parser.
fn descriptor(text: &str, mode: PropertiesMode) -> ParsedDescriptor {
    metadata_parser::parse(text, None, mode).unwrap()
}

/// The resource cannot override the current declaration's name, UUID or type.
#[test]
fn current_declaration_replaces_only_dialog_identity() {
    let data = bsl_metadata::xml_parser::parse_form_xml(DIALOG).unwrap();
    assert_eq!(data.name(), "ResourceIdentity");
    assert_eq!(data.form_type(), FormType::Ordinary);
    let current = form(&descriptor(DECLARATION, PropertiesMode::All), data.clone()).unwrap();
    assert_eq!(current.name(), "Dialog");
    assert_eq!(
        current.uuid().to_string(),
        "10000000-0000-4000-8000-000000000001"
    );
    assert_eq!(current.form_type(), FormType::Managed);
    assert_eq!(current.elements, data.elements);
    assert_eq!(current.attributes, data.attributes);
    assert_eq!(current.event_handlers, data.event_handlers);
    assert_eq!(current.command_handlers, data.command_handlers);
    assert_eq!(current.elements.len(), 3);
    assert_eq!(current.elements[1].parent_id, Some(1));
    assert_eq!(
        current.elements[1].data_path.as_deref(),
        Some("Object.Code")
    );
    assert_eq!(current.attributes.len(), 1);
    assert!(current.attributes[0].is_main);
    assert_eq!(current.attributes[0].columns.len(), 1);
    assert_eq!(current.event_handlers.len(), 2);
    assert_eq!(current.command_handlers, ["CurrentAction", "CurrentAction"]);
}

/// Both declaration kinds use the same projection; defaults match the pinned native reader.
#[test]
fn nested_forms_and_explicit_ordinary_data_keep_the_supplied_payload() {
    for spelling in ["Ordinary", "ordinary", "Обычная"] {
        let text = DECLARATION
            .replace("CommonForm", "Form")
            .replace("Managed", spelling);
        let supplied = Form::ordinary_without_dialog("ResourceIdentity");
        let current = form(&descriptor(&text, PropertiesMode::All), supplied.clone()).unwrap();
        assert_eq!(current.form_type(), FormType::Ordinary);
        assert_eq!(current.event_handlers, supplied.event_handlers);
        assert!(current.elements.is_empty());
    }
    for text in [
        DECLARATION.replace("<FormType>Managed</FormType>", ""),
        DECLARATION.replace("Managed", ""),
    ] {
        let current = form(
            &descriptor(&text, PropertiesMode::All),
            Form::ordinary_without_dialog("ResourceIdentity"),
        )
        .unwrap();
        assert_eq!(current.form_type(), FormType::Managed);
    }
}

/// An explicit new payload changes handlers without mutating an earlier model or its identity.
#[test]
fn changed_dialog_data_replaces_the_current_payload() {
    let parsed = descriptor(DECLARATION, PropertiesMode::All);
    let saved = form(
        &parsed,
        bsl_metadata::xml_parser::parse_form_xml(DIALOG).unwrap(),
    )
    .unwrap();
    let changed = DIALOG
        .replace("CurrentAction", "EditedAction")
        .replace("CurrentChange", "EditedChange");
    let current = form(
        &parsed,
        bsl_metadata::xml_parser::parse_form_xml(&changed).unwrap(),
    )
    .unwrap();
    assert_eq!(saved.uuid(), current.uuid());
    assert_eq!(saved.name(), current.name());
    assert_eq!(saved.command_handlers, ["CurrentAction", "CurrentAction"]);
    assert_eq!(current.command_handlers, ["EditedAction", "EditedAction"]);
    assert_ne!(saved, current);
    assert!(bsl_metadata::xml_parser::parse_form_xml("<Form>").is_err());
    assert!(bsl_metadata::xml_parser::parse_form_xml("<Future/>").is_err());
}

/// Unknown, structured and duplicate types have declaration coordinates instead of defaults.
#[test]
fn invalid_form_types_fail_at_the_current_property() {
    for replacement in [
        "<FormType>Future</FormType>",
        "<FormType><Future/></FormType>",
        "<FormType>Managed</FormType><FormType>Ordinary</FormType>",
    ] {
        let text = DECLARATION.replace("<FormType>Managed</FormType>", replacement);
        let error = form(
            &descriptor(&text, PropertiesMode::All),
            Form::ordinary_without_dialog("Input"),
        )
        .unwrap_err();
        assert!(
            matches!(error.issue, AdapterIssue::InvalidProperty { ref name } | AdapterIssue::DuplicateProperty { ref name } if name == "FormType")
        );
        assert!(text[error.range].contains("FormType"));
    }
    let foreign = DECLARATION.replace(
        "<FormType>Managed</FormType>",
        "<f:FormType xmlns:f=\"urn:foreign\">Future</f:FormType>",
    );
    assert_eq!(
        form(
            &descriptor(&foreign, PropertiesMode::All),
            Form::ordinary_without_dialog("Input")
        )
        .unwrap()
        .form_type(),
        FormType::Managed
    );
}

/// Missing captures, identities and incomplete parses cannot be treated as empty successful forms.
#[test]
fn incomplete_or_wrong_descriptors_fail_before_projection() {
    let supplied = Form::ordinary_without_dialog("Input");
    assert_eq!(
        form(
            &descriptor(DECLARATION, PropertiesMode::Summary),
            supplied.clone()
        )
        .unwrap_err()
        .issue,
        AdapterIssue::PropertiesNotLoaded
    );
    let wrong = DECLARATION.replace("CommonForm", "Role");
    assert_eq!(
        form(&descriptor(&wrong, PropertiesMode::All), supplied.clone())
            .unwrap_err()
            .issue,
        AdapterIssue::WrongKind {
            actual: MetadataKind::Role
        }
    );
    let invalid = DECLARATION.replace("10000000-0000-4000-8000-000000000001", "invalid");
    let error = form(&descriptor(&invalid, PropertiesMode::All), supplied.clone()).unwrap_err();
    assert_eq!(error.issue, AdapterIssue::InvalidUuid);
    assert!(invalid[error.range].contains("invalid"));
    let mut empty = descriptor(DECLARATION, PropertiesMode::All);
    empty.objects.clear();
    assert_eq!(
        form(&empty, supplied.clone()).unwrap_err().issue,
        AdapterIssue::MissingRoot
    );
    let incomplete = DECLARATION.replace(
        "</CommonForm>",
        "<ChildObjects><Future>Unknown</Future></ChildObjects></CommonForm>",
    );
    assert_eq!(
        form(&descriptor(&incomplete, PropertiesMode::All), supplied)
            .unwrap_err()
            .issue,
        AdapterIssue::IncompleteDescriptor
    );
}
