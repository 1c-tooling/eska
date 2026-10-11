//! Native model equality and invalid-current-input checks do not rely on a second adapter oracle.

use super::*;
use eska::project_core::metadata_parser::{self, PropertiesMode};

/// Logical job properties keep namespace bindings and source coordinates from the shared parser.
fn xml(fields: &str) -> String {
    format!(
        r#"<MetaDataObject xmlns="http://v8.1c.ru/8.3/MDClasses" version="2.20"><ScheduledJob uuid="10000000-0000-4000-8000-000000000001"><Properties><Name>BeforeRun</Name>{fields}</Properties></ScheduledJob></MetaDataObject>"#
    )
}

/// The adapter receives a logical descriptor with captured properties, never a private XML DOM.
fn converted(text: &str) -> Result<ScheduledJob, AdapterError> {
    let descriptor = metadata_parser::parse(text, None, PropertiesMode::All).unwrap();
    scheduled_job(&descriptor.objects[0])
}

/// Missing flags have the native reader's false defaults, while all explicit boolean states persist.
#[test]
fn scheduled_jobs_match_native_uuid_name_callback_and_flags() {
    for fields in [
        "",
        "<MethodName/>",
        "<MethodName>CommonModule.Tools.Saved</MethodName><Predefined>true</Predefined><Use>false</Use>",
        "<MethodName>CommonModule.Tools.Current</MethodName><Predefined>false</Predefined><Use>true</Use>",
    ] {
        let text = xml(fields);
        assert_eq!(
            converted(&text).unwrap(),
            bsl_metadata::xml_parser::parse_scheduled_job_xml(&text).unwrap()
        );
    }
}

/// Explicit malformed values cannot quietly inherit the false defaults used for absent fields.
#[test]
fn scheduled_job_invalid_flags_and_structured_callback_fail_at_the_current_field() {
    for (name, fields) in [
        ("Predefined", "<Predefined/>"),
        ("Predefined", "<Predefined>Future</Predefined>"),
        ("Use", "<Use>TRUE</Use>"),
        ("Use", "<Use><Value>true</Value></Use>"),
        (
            "MethodName",
            "<MethodName><Value>CommonModule.Tools.Saved</Value></MethodName>",
        ),
    ] {
        let text = xml(fields);
        let error = converted(&text).unwrap_err();
        assert_eq!(
            error.issue,
            AdapterIssue::InvalidProperty { name: name.into() }
        );
        assert_eq!(&text[error.range], fields);
    }
}

/// Duplicates are rejected even when equal; foreign namespace flags cannot shadow schema fields.
#[test]
fn scheduled_job_checks_property_identity_and_duplicate_fields() {
    for name in ["MethodName", "Predefined", "Use"] {
        let value = if name == "MethodName" {
            "CommonModule.Tools.Saved"
        } else {
            "false"
        };
        let text = xml(&format!("<{name}>{value}</{name}><{name}>{value}</{name}>"));
        let error = converted(&text).unwrap_err();
        assert_eq!(
            error.issue,
            AdapterIssue::DuplicateProperty { name: name.into() }
        );
        assert_eq!(&text[error.range], format!("<{name}>{value}</{name}>"));
    }
    let model = converted(&xml("<Use xmlns='urn:foreign'>true</Use><Use>false</Use>")).unwrap();
    assert!(!model.is_enabled());
}

/// Summary capture and wrong identities cannot become a ready model with silently absent fields.
#[test]
fn scheduled_job_requires_the_expected_kind_uuid_and_property_capture() {
    let text = xml("");
    let summary = metadata_parser::parse(&text, None, PropertiesMode::Summary).unwrap();
    assert_eq!(
        scheduled_job(&summary.objects[0]).unwrap_err().issue,
        AdapterIssue::PropertiesNotLoaded
    );
    assert!(matches!(
        converted(&text.replace("ScheduledJob", "EventSubscription"))
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
