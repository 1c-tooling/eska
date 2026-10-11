//! Native equality covers ordered operations and their parent-scoped parameter names.

use super::*;
use eska::project_core::metadata_parser::{self, PropertiesMode};

/// XML is only an independent fixture; the adapter consumes the common parsed descriptor.
fn xml(fields: &str, children: &str) -> String {
    format!(
        r#"<MetaDataObject xmlns="http://v8.1c.ru/8.3/MDClasses" version="2.20"><WebService uuid="10000000-0000-4000-8000-000000000001"><Properties><Name>API</Name>{fields}</Properties><ChildObjects>{children}</ChildObjects></WebService></MetaDataObject>"#
    )
}

/// Use distinct UUIDs while allowing the same parameter name under different operations.
fn operation(name: &str, number: u8, fields: &str) -> String {
    format!(
        r#"<Operation uuid="20000000-0000-4000-8000-{number:012}"><Properties><Name>{name}</Name>{fields}</Properties><ChildObjects><Parameter uuid="30000000-0000-4000-8000-{number:012}"><Properties><Name>Request</Name><XDTOType><Namespace>urn:example</Namespace><Name>Envelope</Name></XDTOType></Properties></Parameter></ChildObjects></Operation>"#
    )
}

/// Captured ranges belong to the exact supplied descriptor bytes.
fn converted(text: &str) -> Result<WebService, AdapterError> {
    web_service(&metadata_parser::parse(text, None, PropertiesMode::All).unwrap())
}

/// Native defaults and opaque XDTO fields cannot invent extra parameter types or reorder names.
#[test]
fn web_service_operations_and_parameters_match_native_reader() {
    let operations = format!(
        "{}{}",
        operation("First", 1, "<ProcedureName>FirstCall</ProcedureName>"),
        operation("Second", 2, "<ProcedureName>SecondCall</ProcedureName>")
    );
    for (fields, children) in [
        ("", ""),
        ("<Namespace/>", ""),
        ("<Namespace>urn:api</Namespace>", operations.as_str()),
    ] {
        let text = xml(fields, children);
        assert_eq!(
            converted(&text).unwrap(),
            bsl_metadata::xml_parser::parse_web_service_xml(&text, "API").unwrap()
        );
    }
    let default = converted(&xml("", &operation("Empty", 1, ""))).unwrap();
    assert_eq!(default.uri(), Some("WebServices/API/Ext/Module.bsl"));
    assert!(default.operations()[0].is_handler_empty());
    assert_eq!(default.operations()[0].parameters()[0].name(), "Request");
}

/// Duplicate or structured scalars fail at the correct root or operation property.
#[test]
fn web_service_scalar_fields_reject_records_and_duplicates() {
    for name in ["Namespace", "ProcedureName"] {
        for (field, issue) in [
            (
                format!("<{name}><Future>value</Future></{name}>"),
                AdapterIssue::InvalidProperty { name: name.into() },
            ),
            (
                format!("<{name}/><{name}>duplicate</{name}>"),
                AdapterIssue::DuplicateProperty { name: name.into() },
            ),
        ] {
            let text = if name == "Namespace" {
                xml(&field, "")
            } else {
                xml("", &operation("Call", 1, &field))
            };
            let error = converted(&text).unwrap_err();
            assert_eq!(error.issue, issue);
            assert!(text[error.range].contains(name));
        }
    }
}

/// Source kind, UUID and captured parameter properties are required despite the native model's small field set.
#[test]
fn web_service_requires_identity_and_all_parameter_property_capture() {
    let text = xml(
        "<Namespace>current</Namespace><Namespace xmlns='urn:foreign'>wrong</Namespace>",
        &operation("Call", 1, ""),
    );
    assert_eq!(converted(&text).unwrap().namespace(), "current");
    let summary = metadata_parser::parse(&text, None, PropertiesMode::Summary).unwrap();
    assert_eq!(
        web_service(&summary).unwrap_err().issue,
        AdapterIssue::PropertiesNotLoaded
    );
    let mut descriptor = metadata_parser::parse(&text, None, PropertiesMode::All).unwrap();
    descriptor.objects.last_mut().unwrap().properties = None;
    assert_eq!(
        web_service(&descriptor).unwrap_err().issue,
        AdapterIssue::PropertiesNotLoaded
    );
    assert!(matches!(
        converted(&text.replace("WebService", "HTTPService"))
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

/// Invalid child UUIDs and recoverable child fragments cannot become a ready operation table.
#[test]
fn web_service_rejects_incomplete_children_and_invalid_parameter_uuid() {
    assert_eq!(
        converted(&xml("", "<Future>Unknown</Future>"))
            .unwrap_err()
            .issue,
        AdapterIssue::IncompleteDescriptor
    );
    let child = operation("Call", 1, "");
    assert_eq!(
        converted(&xml(
            "",
            &child.replace("30000000-0000-4000-8000-000000000001", "bad-uuid")
        ))
        .unwrap_err()
        .issue,
        AdapterIssue::InvalidUuid
    );
    assert_eq!(
        converted(&xml("", &format!("{child}{child}")))
            .unwrap_err()
            .issue,
        AdapterIssue::IncompleteDescriptor
    );
}
