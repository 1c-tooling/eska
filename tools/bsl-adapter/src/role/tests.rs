//! Role projection preserves native rights scope without manufacturing a successful default.

use super::*;
use eska::project_core::metadata_parser::{self, PropertiesMode};

const ROLE: &str = r#"<MetaDataObject xmlns="http://v8.1c.ru/8.3/MDClasses" version="2.20"><Role uuid="10000000-0000-4000-8000-000000000001"><Properties><Name>Reader</Name><Synonym/><Comment/></Properties></Role></MetaDataObject>"#;
const RIGHTS: &str = r#"<Rights xmlns="http://v8.1c.ru/8.2/roles" version="2.20">
<setForNewObjects>true</setForNewObjects><setForAttributesByDefault>false</setForAttributesByDefault><independentRightsOfChildObjects>true</independentRightsOfChildObjects>
<object><name>Configuration.Probe</name><right><name>Administration</name><value>true</value></right></object>
<object><name>Catalog.Items</name><right><name>Read</name><value>true</value><restrictionByCondition><condition> Items.Owner IN (SELECT Ref FROM Catalog.Owners) </condition></restrictionByCondition></right></object>
<object><name>Catalog.Items.Attribute.Code</name><right><name>View</name><value>false</value></right></object>
</Rights>"#;

/// Source descriptors use the shared logical parser rather than the native role XML reader.
fn descriptor(text: &str, mode: PropertiesMode) -> ParsedDescriptor {
    metadata_parser::parse(text, None, mode).unwrap()
}

/// Source UUIDs, flags, object-grain references, order, duplicates and trimmed RLS match native input.
#[test]
fn role_identity_and_supplied_rights_match_the_native_reader() {
    let parsed = descriptor(ROLE, PropertiesMode::All);
    for rights in ["<Rights/>", RIGHTS] {
        let data = bsl_metadata::xml_parser::parse_rights_xml(rights).unwrap();
        let current = role(&parsed, data).unwrap();
        assert_eq!(
            current,
            bsl_metadata::parse_role_from_texts(ROLE, Some(rights)).unwrap()
        );
    }
    let data = bsl_metadata::xml_parser::parse_rights_xml(RIGHTS).unwrap();
    let current = role(&parsed, data).unwrap();
    assert_eq!(
        current.uuid().to_string(),
        "10000000-0000-4000-8000-000000000001"
    );
    assert!(current.data().set_for_new_objects());
    assert!(!current.data().set_for_attributes_by_default());
    assert!(current.data().independent_rights_of_child_objects());
    assert_eq!(current.objects().len(), 2);
    assert_eq!(current.objects()[0].name, "Items");
    assert_eq!(current.objects()[1].name, "Items");
    assert_eq!(
        current.objects()[0].restrictions,
        ["Items.Owner IN (SELECT Ref FROM Catalog.Owners)"]
    );
    assert!(current.objects()[1].restrictions.is_empty());
}

/// Changing the rights argument replaces the model; a previous holder keeps its original value.
#[test]
fn current_rights_replace_the_payload_without_replacing_role_identity() {
    let parsed = descriptor(ROLE, PropertiesMode::All);
    let saved = role(
        &parsed,
        bsl_metadata::xml_parser::parse_rights_xml(RIGHTS).unwrap(),
    )
    .unwrap();
    let changed = RIGHTS.replace("Catalog.Items", "Document.Current");
    let current = role(
        &parsed,
        bsl_metadata::xml_parser::parse_rights_xml(&changed).unwrap(),
    )
    .unwrap();
    assert_eq!(saved.uuid(), current.uuid());
    assert_eq!(saved.name(), current.name());
    assert_eq!(saved.objects()[0].mdo_type, bsl_metadata::MdoType::Catalog);
    assert_eq!(
        current.objects()[0].mdo_type,
        bsl_metadata::MdoType::Document
    );
    assert_eq!(current.objects()[0].name, "Current");
    assert_ne!(saved, current);
    assert!(bsl_metadata::xml_parser::parse_rights_xml("<Rights>").is_err());
    assert!(bsl_metadata::xml_parser::parse_rights_xml("<Future/>").is_err());
}

/// Default data is accepted only as an explicit caller decision for an absent resource.
#[test]
fn explicit_absent_rights_preserve_native_identity() {
    let current = role(&descriptor(ROLE, PropertiesMode::All), RoleData::default()).unwrap();
    assert_eq!(
        current,
        bsl_metadata::parse_role_from_texts(ROLE, None).unwrap()
    );
    assert!(current.objects().is_empty());
}

/// Missing captures, malformed identities and incomplete descriptors cannot become ready roles.
#[test]
fn role_projection_rejects_invalid_descriptor_inputs_at_source_ranges() {
    assert_eq!(
        role(
            &descriptor(ROLE, PropertiesMode::Summary),
            RoleData::default()
        )
        .unwrap_err()
        .issue,
        AdapterIssue::PropertiesNotLoaded
    );
    let wrong = ROLE.replace("Role", "HTTPService");
    assert!(matches!(
        role(
            &descriptor(&wrong, PropertiesMode::All),
            RoleData::default()
        )
        .unwrap_err()
        .issue,
        AdapterIssue::WrongKind {
            actual: MetadataKind::HTTPService
        }
    ));
    let invalid = ROLE.replace("10000000-0000-4000-8000-000000000001", "invalid");
    let error = role(
        &descriptor(&invalid, PropertiesMode::All),
        RoleData::default(),
    )
    .unwrap_err();
    assert_eq!(error.issue, AdapterIssue::InvalidUuid);
    assert!(invalid[error.range].contains("invalid"));
    let mut incomplete = descriptor(ROLE, PropertiesMode::All);
    incomplete.objects.clear();
    assert_eq!(
        role(&incomplete, RoleData::default()).unwrap_err().issue,
        AdapterIssue::MissingRoot
    );
    let incomplete = ROLE.replace(
        "</Role>",
        "<ChildObjects><Future>Unknown</Future></ChildObjects></Role>",
    );
    assert_eq!(
        role(
            &descriptor(&incomplete, PropertiesMode::All),
            RoleData::default()
        )
        .unwrap_err()
        .issue,
        AdapterIssue::IncompleteDescriptor
    );
}
