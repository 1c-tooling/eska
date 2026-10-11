//! Native equality includes parent-scoped method names, routes, handlers and logical module URI.

use super::*;
use eska::project_core::metadata_parser::{self, PropertiesMode};

/// Keep all identities unique while allowing the same method name under different templates.
fn xml(fields: &str, children: &str) -> String {
    format!(
        r#"<MetaDataObject xmlns="http://v8.1c.ru/8.3/MDClasses" version="2.20"><HTTPService uuid="10000000-0000-4000-8000-000000000001"><Properties><Name>API</Name>{fields}</Properties><ChildObjects>{children}</ChildObjects></HTTPService></MetaDataObject>"#
    )
}

/// The adapter receives only the common descriptor, including exact source ranges.
fn converted(text: &str) -> Result<HTTPService, AdapterError> {
    http_service(&metadata_parser::parse(text, None, PropertiesMode::All).unwrap())
}

/// Construct a direct template with one method instead of flattening its handler into the root.
fn template(name: &str, number: u8, fields: &str, method: &str) -> String {
    format!(
        r#"<URLTemplate uuid="20000000-0000-4000-8000-{number:012}"><Properties><Name>{name}</Name>{fields}</Properties><ChildObjects><Method uuid="30000000-0000-4000-8000-{number:012}"><Properties><Name>Get</Name>{method}</Properties></Method></ChildObjects></URLTemplate>"#
    )
}

/// Complete equality verifies native defaults and repeated method names under distinct parents.
#[test]
fn http_service_routes_and_handlers_match_native_reader() {
    let children = format!(
        "{}{}",
        template(
            "First",
            1,
            "<Template>/v1/first</Template>",
            "<HTTPMethod>GET</HTTPMethod><Handler>FirstGet</Handler>",
        ),
        template(
            "Second",
            2,
            "<Template>/v1/second/{id}</Template>",
            "<HTTPMethod>POST</HTTPMethod><Handler>SecondPost</Handler>",
        ),
    );
    for (fields, children) in [
        ("", ""),
        ("<RootURL/>", ""),
        ("<RootURL>api</RootURL>", children.as_str()),
    ] {
        let text = xml(fields, children);
        assert_eq!(
            converted(&text).unwrap(),
            bsl_metadata::xml_parser::parse_http_service_xml(&text, "API").unwrap()
        );
    }
    let defaults = converted(&xml("", &template("Empty", 1, "", ""))).unwrap();
    assert_eq!(defaults.uri(), Some("HTTPServices/API/Ext/Module.bsl"));
    let route = &defaults.url_templates()[0];
    assert_eq!(route.template(), "");
    assert_eq!(route.methods()[0].http_method(), "");
    assert!(route.methods()[0].is_handler_empty());
}

/// Structured or duplicate scalars fail at their actual level with the offending source range.
#[test]
fn http_service_scalar_fields_reject_records_and_duplicates() {
    for name in ["RootURL", "Template", "HTTPMethod", "Handler"] {
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
            let text = match name {
                "RootURL" => xml(&field, ""),
                "Template" => xml("", &template("Route", 1, &field, "")),
                _ => xml("", &template("Route", 1, "", &field)),
            };
            let error = converted(&text).unwrap_err();
            assert_eq!(error.issue, issue);
            assert!(text[error.range].contains(name));
        }
    }
}

/// Qualified source properties, kind and identity cannot be substituted by unrelated same-name fields.
#[test]
fn http_service_requires_captured_properties_and_correct_identity() {
    let text = xml(
        "<RootURL>current</RootURL><RootURL xmlns='urn:foreign'>wrong</RootURL>",
        "",
    );
    assert_eq!(converted(&text).unwrap().root_url(), "current");
    let summary = metadata_parser::parse(&text, None, PropertiesMode::Summary).unwrap();
    assert_eq!(
        http_service(&summary).unwrap_err().issue,
        AdapterIssue::PropertiesNotLoaded
    );
    assert!(matches!(
        converted(&text.replace("HTTPService", "WebService"))
            .unwrap_err()
            .issue,
        AdapterIssue::WrongKind { .. }
    ));
    let invalid = text.replace("10000000-0000-4000-8000-000000000001", "bad-uuid");
    assert_eq!(
        converted(&invalid).unwrap_err().issue,
        AdapterIssue::InvalidUuid
    );
}

/// Child identity failures and recoverable parser issues cannot yield a ready but incomplete route table.
#[test]
fn http_service_rejects_incomplete_children_and_invalid_child_uuid() {
    let text = xml("", "<Future>Unknown</Future>");
    assert_eq!(
        converted(&text).unwrap_err().issue,
        AdapterIssue::IncompleteDescriptor
    );
    let child = template("Route", 1, "", "");
    let text = xml(
        "",
        &child.replace("30000000-0000-4000-8000-000000000001", "bad-uuid"),
    );
    assert_eq!(
        converted(&text).unwrap_err().issue,
        AdapterIssue::InvalidUuid
    );
    let text = xml("", &format!("{child}{child}"));
    assert_eq!(
        converted(&text).unwrap_err().issue,
        AdapterIssue::IncompleteDescriptor
    );
}
