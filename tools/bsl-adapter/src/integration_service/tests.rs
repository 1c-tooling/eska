//! Native equality preserves receive and send-only channels without inventing transport semantics.

use super::*;
use eska::project_core::metadata_parser::{self, PropertiesMode};

/// XML is an independent fixture for the common parser, never the adapter's input format.
fn xml(children: &str) -> String {
    format!(
        r#"<MetaDataObject xmlns="http://v8.1c.ru/8.3/MDClasses" version="2.20"><IntegrationService uuid="10000000-0000-4000-8000-000000000001"><Properties><Name>Bus</Name></Properties><ChildObjects>{children}</ChildObjects></IntegrationService></MetaDataObject>"#
    )
}

/// Stable channel identities permit assertions about order and actual property coordinates.
fn channel(name: &str, number: u8, fields: &str) -> String {
    format!(
        r#"<IntegrationServiceChannel uuid="20000000-0000-4000-8000-{number:012}"><Properties><Name>{name}</Name>{fields}</Properties></IntegrationServiceChannel>"#
    )
}

/// The adapter consumes one fully captured shared descriptor and its exact source ranges.
fn converted(text: &str) -> Result<IntegrationService, AdapterError> {
    integration_service(&metadata_parser::parse(text, None, PropertiesMode::All).unwrap())
}

/// Empty bindings are retained as channels and omitted only by the native handler iterator.
#[test]
fn integration_channels_match_native_reader_and_keep_send_only_bindings() {
    let children = format!(
        "{}{}{}",
        channel(
            "Incoming",
            1,
            "<ReceiveMessageProcessing>Receive</ReceiveMessageProcessing><MessageDirection>Receive</MessageDirection>"
        ),
        channel(
            "Outgoing",
            2,
            "<ReceiveMessageProcessing/><MessageDirection>Send</MessageDirection>"
        ),
        channel("Unbound", 3, "")
    );
    for children in ["", children.as_str()] {
        let text = xml(children);
        assert_eq!(
            converted(&text).unwrap(),
            bsl_metadata::xml_parser::parse_integration_service_xml(&text, "Bus").unwrap()
        );
    }
    let service = converted(&xml(&children)).unwrap();
    assert_eq!(service.channels().len(), 3);
    assert_eq!(service.receive_handlers().collect::<Vec<_>>(), ["Receive"]);
    assert_eq!(service.channels()[1].name(), "Outgoing");
    assert!(
        service.channels()[2]
            .receive_message_processing()
            .is_empty()
    );
}

/// Structured or repeated handler values fail at the actual channel property.
#[test]
fn integration_handler_rejects_records_and_duplicate_scalars() {
    for (field, issue) in [
        (
            "<ReceiveMessageProcessing><Future/></ReceiveMessageProcessing>",
            AdapterIssue::InvalidProperty {
                name: "ReceiveMessageProcessing".into(),
            },
        ),
        (
            "<ReceiveMessageProcessing/><ReceiveMessageProcessing>Duplicate</ReceiveMessageProcessing>",
            AdapterIssue::DuplicateProperty {
                name: "ReceiveMessageProcessing".into(),
            },
        ),
    ] {
        let text = xml(&channel("Incoming", 1, field));
        let error = converted(&text).unwrap_err();
        assert_eq!(error.issue, issue);
        assert!(text[error.range].contains("ReceiveMessageProcessing"));
    }
}

/// Foreign namespace fields cannot override a captured binding; summary-only children are unavailable.
#[test]
fn integration_service_requires_kind_identity_and_complete_channel_properties() {
    let text = xml(&channel(
        "Incoming",
        1,
        "<ReceiveMessageProcessing>Receive</ReceiveMessageProcessing><ReceiveMessageProcessing xmlns='urn:foreign'>Wrong</ReceiveMessageProcessing>",
    ));
    assert_eq!(
        converted(&text)
            .unwrap()
            .receive_handlers()
            .collect::<Vec<_>>(),
        ["Receive"]
    );
    let summary = metadata_parser::parse(&text, None, PropertiesMode::Summary).unwrap();
    assert_eq!(
        integration_service(&summary).unwrap_err().issue,
        AdapterIssue::PropertiesNotLoaded
    );
    let mut descriptor = metadata_parser::parse(&text, None, PropertiesMode::All).unwrap();
    descriptor.objects.last_mut().unwrap().properties = None;
    assert_eq!(
        integration_service(&descriptor).unwrap_err().issue,
        AdapterIssue::PropertiesNotLoaded
    );
    assert!(matches!(
        converted(&xml("").replace("IntegrationService", "HTTPService"))
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

/// Recoverable fragments, duplicate identities and invalid child UUIDs cannot become a ready binding table.
#[test]
fn integration_service_rejects_incomplete_channels_and_invalid_child_identity() {
    assert_eq!(
        converted(&xml("<Future>Unknown</Future>"))
            .unwrap_err()
            .issue,
        AdapterIssue::IncompleteDescriptor
    );
    let child = channel("Incoming", 1, "");
    assert_eq!(
        converted(&xml(
            &child.replace("20000000-0000-4000-8000-000000000001", "bad-uuid")
        ))
        .unwrap_err()
        .issue,
        AdapterIssue::InvalidUuid
    );
    assert_eq!(
        converted(&xml(&format!("{child}{child}")))
            .unwrap_err()
            .issue,
        AdapterIssue::IncompleteDescriptor
    );
}
