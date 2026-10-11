//! Native equality covers ordered content and child names; incomplete descriptors stay explicit.

use super::*;
use eska::project_core::metadata_parser::{self, PropertiesMode};

/// References carry the source schema namespace independently of prefix spelling.
fn xml(fields: &str, children: &str) -> String {
    format!(
        r#"<MetaDataObject xmlns="http://v8.1c.ru/8.3/MDClasses" xmlns:xr="http://v8.1c.ru/8.3/xcf/readable" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance" version="2.20"><Subsystem uuid="10000000-0000-4000-8000-000000000001"><Properties><Name>Services</Name>{fields}</Properties><ChildObjects>{children}</ChildObjects></Subsystem></MetaDataObject>"#
    )
}

/// Only the shared parsed descriptor is available to the adapter.
fn converted(text: &str) -> Result<Subsystem, AdapterError> {
    subsystem(&metadata_parser::parse(text, None, PropertiesMode::All).unwrap())
}

/// Native content order, duplicates, member tails and unsupported-kind exclusions all persist.
#[test]
fn subsystem_content_and_children_match_native_reader() {
    for (fields, children) in [
        ("", ""),
        ("<Content/>", "<Subsystem>Child</Subsystem>"),
        (
            "<Content><xr:Item xsi:type='xr:MDObjectRef'>Catalog.Items</xr:Item><xr:Item xsi:type='xr:MDObjectRef'>Document.Receipt.Attribute.Amount</xr:Item><xr:Item xsi:type='xr:MDObjectRef'>Catalog.Items</xr:Item><xr:Item xsi:type='xr:MDObjectRef'>CommonForm.Menu</xr:Item><xr:Item xsi:type='xr:MDObjectRef'>Bogus.Unknown</xr:Item><xr:Item xsi:type='xr:MDObjectRef'>Catalog.</xr:Item></Content>",
            "<Subsystem> First </Subsystem><Subsystem>Second</Subsystem>",
        ),
    ] {
        let text = xml(fields, children);
        assert_eq!(
            converted(&text).unwrap(),
            bsl_metadata::xml_parser::parse_subsystem_xml(&text).unwrap()
        );
    }
}

/// Prefix aliases are interchangeable, while foreign record members cannot become valid content.
#[test]
fn subsystem_content_uses_property_and_item_namespace_identity() {
    let original = xml(
        "<Content><xr:Item xsi:type='xr:MDObjectRef'>Catalog.Items</xr:Item></Content>",
        "",
    );
    let alias = original
        .replace("xmlns:xr=", "xmlns:readable=")
        .replace("xr:", "readable:");
    assert_eq!(converted(&alias).unwrap(), converted(&original).unwrap());
    let foreign = original.replace("http://v8.1c.ru/8.3/xcf/readable", "urn:foreign");
    assert_eq!(
        converted(&foreign).unwrap_err().issue,
        AdapterIssue::InvalidProperty {
            name: "Content".into()
        }
    );
    let duplicate = xml("<Content/><Content/>", "");
    assert_eq!(
        converted(&duplicate).unwrap_err().issue,
        AdapterIssue::DuplicateProperty {
            name: "Content".into()
        }
    );
    let scalar = xml("<Content>Catalog.Items</Content>", "");
    assert_eq!(
        converted(&scalar).unwrap_err().issue,
        AdapterIssue::InvalidProperty {
            name: "Content".into()
        }
    );
}

/// Bad child fragments cannot disappear into an apparently complete organisational graph value.
#[test]
fn subsystem_recoverable_descriptor_issues_are_rejected() {
    let text = xml("", "<Future>Unknown</Future>");
    assert_eq!(
        converted(&text).unwrap_err().issue,
        AdapterIssue::IncompleteDescriptor
    );
    let duplicate = xml(
        "",
        "<Subsystem>Child</Subsystem><Subsystem>Child</Subsystem>",
    );
    assert_eq!(
        converted(&duplicate).unwrap_err().issue,
        AdapterIssue::IncompleteDescriptor
    );
}

/// Missing property capture and invalid identities cannot silently produce empty content.
#[test]
fn subsystem_requires_kind_uuid_and_captured_properties() {
    let text = xml("", "");
    let summary = metadata_parser::parse(&text, None, PropertiesMode::Summary).unwrap();
    assert_eq!(
        subsystem(&summary).unwrap_err().issue,
        AdapterIssue::PropertiesNotLoaded
    );
    assert!(matches!(
        converted(&text.replace("Subsystem", "ScheduledJob"))
            .unwrap_err()
            .issue,
        AdapterIssue::WrongKind { .. }
    ));
    assert_eq!(
        converted(&text.replace("10000000-0000-4000-8000-000000000001", "bad-uuid"))
            .unwrap_err()
            .issue,
        AdapterIssue::InvalidUuid
    );
}
