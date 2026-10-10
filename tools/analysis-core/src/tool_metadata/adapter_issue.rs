//! Domain causes become stable machine fields; Rust Debug output and localized strings are not a protocol.

use eska_bsl_adapter::AdapterIssue;
use serde_json::{Value, json};

/// Exhaustive transport mapping keeps every current cause distinct without changing the core's error types.
pub fn value(issue: &AdapterIssue) -> Value {
    match issue {
        AdapterIssue::PropertiesNotLoaded => json!({"kind":"properties_not_loaded"}),
        AdapterIssue::WrongKind { actual } => json!({"kind":"wrong_kind","actual":actual.as_str()}),
        AdapterIssue::InvalidUuid => json!({"kind":"invalid_uuid"}),
        AdapterIssue::DuplicateProperty { name } => {
            json!({"kind":"duplicate_property","name":name})
        }
        AdapterIssue::InvalidProperty { name } => json!({"kind":"invalid_property","name":name}),
        AdapterIssue::UnsupportedOwnership { value } => {
            json!({"kind":"unsupported_ownership","value":value})
        }
        AdapterIssue::MissingProperty { name } => json!({"kind":"missing_property","name":name}),
        AdapterIssue::UnsupportedType { key } => json!({"kind":"unsupported_type","key":key}),
        AdapterIssue::UnsupportedTypeField { key } => {
            json!({"kind":"unsupported_type_field","key":key})
        }
        AdapterIssue::UnresolvedTypeName { text } => {
            json!({"kind":"unresolved_type_name","text":text})
        }
        AdapterIssue::MissingRoot => json!({"kind":"missing_root"}),
        AdapterIssue::IncompleteDescriptor => json!({"kind":"incomplete_descriptor"}),
        AdapterIssue::UnsupportedStandardAttribute { name } => {
            json!({"kind":"unsupported_standard_attribute","name":name})
        }
        AdapterIssue::UnsupportedPropertyValue { name, value } => {
            json!({"kind":"unsupported_property_value","name":name,"value":value})
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use eska::project_core::{MetadataKind, metadata_model::PropertyKey};

    /// Plain causes carry only their contracted tag, while kind identifiers retain canonical serialization.
    #[test]
    fn plain_causes_and_wrong_kind_have_stable_machine_shapes() {
        for (issue, expected) in [
            (
                AdapterIssue::PropertiesNotLoaded,
                json!({"kind":"properties_not_loaded"}),
            ),
            (AdapterIssue::InvalidUuid, json!({"kind":"invalid_uuid"})),
            (AdapterIssue::MissingRoot, json!({"kind":"missing_root"})),
            (
                AdapterIssue::IncompleteDescriptor,
                json!({"kind":"incomplete_descriptor"}),
            ),
            (
                AdapterIssue::WrongKind {
                    actual: MetadataKind::ChartOfAccounts,
                },
                json!({"kind":"wrong_kind","actual":"chart-of-accounts"}),
            ),
        ] {
            assert_eq!(value(&issue), expected);
        }
    }

    /// QName/setting fields remain structured, including a null namespace; source values are not localized or normalized.
    #[test]
    fn cause_details_preserve_qualified_keys_and_explicit_source_values() {
        for (issue, expected) in [
            (
                AdapterIssue::DuplicateProperty {
                    name: "Type".into(),
                },
                json!({"kind":"duplicate_property","name":"Type"}),
            ),
            (
                AdapterIssue::InvalidProperty {
                    name: "CodeLength".into(),
                },
                json!({"kind":"invalid_property","name":"CodeLength"}),
            ),
            (
                AdapterIssue::UnsupportedOwnership {
                    value: "Будущий😀".into(),
                },
                json!({"kind":"unsupported_ownership","value":"Будущий😀"}),
            ),
            (
                AdapterIssue::MissingProperty {
                    name: "Type".into(),
                },
                json!({"kind":"missing_property","name":"Type"}),
            ),
            (
                AdapterIssue::UnsupportedType {
                    key: PropertyKey {
                        namespace: Some("urn:config".into()),
                        name: "FutureRef.Target".into(),
                    },
                },
                json!({"kind":"unsupported_type","key":{"namespace":"urn:config","name":"FutureRef.Target"}}),
            ),
            (
                AdapterIssue::UnsupportedTypeField {
                    key: PropertyKey {
                        namespace: None,
                        name: "Future".into(),
                    },
                },
                json!({"kind":"unsupported_type_field","key":{"namespace":null,"name":"Future"}}),
            ),
            (
                AdapterIssue::UnresolvedTypeName {
                    text: "unknown:Type".into(),
                },
                json!({"kind":"unresolved_type_name","text":"unknown:Type"}),
            ),
            (
                AdapterIssue::UnsupportedStandardAttribute {
                    name: "FutureField".into(),
                },
                json!({"kind":"unsupported_standard_attribute","name":"FutureField"}),
            ),
            (
                AdapterIssue::UnsupportedPropertyValue {
                    name: "Mode".into(),
                    value: "Future".into(),
                },
                json!({"kind":"unsupported_property_value","name":"Mode","value":"Future"}),
            ),
        ] {
            assert_eq!(value(&issue), expected);
        }
    }
}
