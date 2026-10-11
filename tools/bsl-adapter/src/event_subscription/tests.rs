//! The reference parser supplies native fields; namespace and malformed-input checks are independent.

use super::*;
use eska::project_core::metadata_parser::{self, PropertiesMode};

/// Native descriptors keep `QName` bindings in the shared parser's captured scope.
fn xml(fields: &str) -> String {
    format!(
        r#"<MetaDataObject xmlns="http://v8.1c.ru/8.3/MDClasses" xmlns:v8="http://v8.1c.ru/8.1/data/core" xmlns:cfg="http://v8.1c.ru/8.1/data/enterprise/current-config" version="2.20"><EventSubscription uuid="10000000-0000-4000-8000-000000000001"><Properties><Name>BeforeSave</Name>{fields}</Properties></EventSubscription></MetaDataObject>"#
    )
}

/// Project logical metadata only; the adapter never receives a private XML DOM.
fn converted(text: &str) -> Result<EventSubscription, AdapterError> {
    let parsed = metadata_parser::parse(text, None, PropertiesMode::All).unwrap();
    event_subscription(&parsed.objects[0])
}

/// Complete native equality covers UUID, optional comment, source order, event and handler.
#[test]
fn subscription_models_match_native_reader_without_losing_source_alternatives() {
    for fields in [
        "",
        "<Comment/><Source/><Event/><Handler/>",
        "<Comment> A callback. </Comment><Source><v8:TypeSet>cfg:CatalogObject</v8:TypeSet><v8:Type>cfg:DocumentObject.Receipt</v8:Type><v8:TypeSet>cfg:CatalogObject</v8:TypeSet></Source><Event>BeforeWrite</Event><Handler>CommonModule.Tools.BeforeSave</Handler>",
    ] {
        let text = xml(fields);
        assert_eq!(
            converted(&text).unwrap(),
            bsl_metadata::xml_parser::parse_event_subscription_xml(&text).unwrap()
        );
    }
}

/// Source aliases and a default `QName` namespace denote the same ordered config identities.
#[test]
fn subscription_source_uses_resolved_namespaces_instead_of_prefix_spelling() {
    let original = xml(
        "<Source><v8:Type>cfg:CatalogObject.Items</v8:Type><v8:TypeSet>cfg:DocumentObject</v8:TypeSet></Source>",
    );
    let alias = original
        .replace("xmlns:v8=", "xmlns:core=")
        .replace("xmlns:cfg=", "xmlns:configuration=")
        .replace("v8:", "core:")
        .replace("cfg:", "configuration:");
    let expected = converted(&original).unwrap();
    assert_eq!(
        expected.source(),
        "cfg:CatalogObject.Items;cfg:DocumentObject"
    );
    assert_eq!(converted(&alias).unwrap(), expected);
    let default = xml(
        "<Source><v8:Type xmlns='http://v8.1c.ru/8.1/data/enterprise/current-config'>CatalogObject.Items</v8:Type><v8:TypeSet>cfg:DocumentObject</v8:TypeSet></Source>",
    );
    assert_eq!(converted(&default).unwrap(), expected);
    let foreign = original.replace(
        "http://v8.1c.ru/8.1/data/enterprise/current-config",
        "urn:foreign",
    );
    assert!(matches!(
        converted(&foreign).unwrap_err().issue,
        AdapterIssue::UnsupportedType { .. }
    ));
}

/// Unknown fields, unresolved `QNames` and record-as-scalar input fail at the current property.
#[test]
fn subscription_malformed_and_duplicate_fields_are_not_discarded() {
    for fields in [
        "<Source>cfg:CatalogObject.Items</Source>",
        "<Source><v8:Type>unbound:CatalogObject.Items</v8:Type></Source>",
        "<Source><v8:Future>cfg:CatalogObject.Items</v8:Future></Source>",
        "<Source><v8:Type><Item>cfg:CatalogObject.Items</Item></v8:Type></Source>",
        "<Source/><Source/>",
        "<Event><Value>BeforeWrite</Value></Event>",
        "<Event>BeforeWrite</Event><Event>OnWrite</Event>",
        "<Handler><Value>CommonModule.Tools.Method</Value></Handler>",
        "<Handler>A</Handler><Handler>B</Handler>",
    ] {
        let text = xml(fields);
        let error = converted(&text).unwrap_err();
        assert!(text[error.range].starts_with('<'));
    }
}

/// An uncaptured descriptor, wrong kind or invalid identity cannot supply a current subscription.
#[test]
fn subscription_requires_the_expected_kind_uuid_and_captured_properties() {
    let text = xml("");
    let parsed = metadata_parser::parse(&text, None, PropertiesMode::Summary).unwrap();
    assert_eq!(
        event_subscription(&parsed.objects[0]).unwrap_err().issue,
        AdapterIssue::PropertiesNotLoaded
    );
    assert!(matches!(
        converted(&text.replace("EventSubscription", "CommonModule"))
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
