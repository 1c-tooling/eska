//! Human reasons use Fluent in both locales; source tokens remain separate machine data.

use eska::{
    project_core::{metadata_model::PropertyKey, metadata_parser::ParseIssue},
    tool_cli::localization::{LocalizationValue, Localizer},
};
use eska_bsl_adapter::AdapterIssue;

use crate::MetadataDiagnosticIssue;

/// Unsupported engine coverage is a warning; malformed required metadata remains an error.
pub(super) const fn adapter_severity(issue: &AdapterIssue) -> u8 {
    match issue {
        AdapterIssue::UnsupportedOwnership { .. }
        | AdapterIssue::UnsupportedType { .. }
        | AdapterIssue::UnsupportedTypeField { .. }
        | AdapterIssue::UnsupportedStandardAttribute { .. }
        | AdapterIssue::UnsupportedPropertyValue { .. } => 2,
        _ => 1,
    }
}

/// Unknown preserved structures describe coverage; invalid object/identity is a current input error.
pub(super) const fn parse_severity(issue: &ParseIssue) -> u8 {
    match issue {
        ParseIssue::UnsupportedRoot
        | ParseIssue::UnknownKind { .. }
        | ParseIssue::UnsupportedValue => 2,
        ParseIssue::InvalidObject | ParseIssue::DuplicateIdentity(_) => 1,
    }
}

/// Domain causes select explicit localized messages rather than exposing Rust Debug.
pub fn message(issue: &MetadataDiagnosticIssue, localizer: &Localizer) -> String {
    match issue {
        MetadataDiagnosticIssue::Adapter(issue) => adapter_message(issue, localizer),
        MetadataDiagnosticIssue::Parse(issue) => parse_message(issue, localizer),
        MetadataDiagnosticIssue::UnsupportedKind(kind) => one(
            localizer,
            "tool-metadata-unsupported-kind",
            "kind",
            kind.as_str(),
        ),
    }
}

/// Each adapter cause retains a distinct explanation; properties and explicit source values are substituted verbatim.
fn adapter_message(issue: &AdapterIssue, localizer: &Localizer) -> String {
    match issue {
        AdapterIssue::PropertiesNotLoaded => localizer.text("tool-metadata-properties-not-loaded"),
        AdapterIssue::WrongKind { actual } => one(
            localizer,
            "tool-metadata-wrong-kind",
            "kind",
            actual.as_str(),
        ),
        AdapterIssue::InvalidUuid => localizer.text("tool-metadata-invalid-uuid"),
        AdapterIssue::DuplicateProperty { name } => {
            one(localizer, "tool-metadata-duplicate-property", "name", name)
        }
        AdapterIssue::InvalidProperty { name } => {
            one(localizer, "tool-metadata-invalid-property", "name", name)
        }
        AdapterIssue::UnsupportedOwnership { value } => one(
            localizer,
            "tool-metadata-unsupported-ownership",
            "value",
            value,
        ),
        AdapterIssue::MissingProperty { name } => {
            one(localizer, "tool-metadata-missing-property", "name", name)
        }
        AdapterIssue::UnsupportedType { key } => one(
            localizer,
            "tool-metadata-unsupported-type",
            "name",
            &qualified(key),
        ),
        AdapterIssue::UnsupportedTypeField { key } => one(
            localizer,
            "tool-metadata-unsupported-type-field",
            "name",
            &qualified(key),
        ),
        AdapterIssue::UnresolvedTypeName { text } => one(
            localizer,
            "tool-metadata-unresolved-type-name",
            "value",
            text,
        ),
        AdapterIssue::MissingRoot => localizer.text("tool-metadata-missing-root"),
        AdapterIssue::IncompleteDescriptor => localizer.text("tool-metadata-incomplete-descriptor"),
        AdapterIssue::UnsupportedStandardAttribute { name } => one(
            localizer,
            "tool-metadata-unsupported-standard-attribute",
            "name",
            name,
        ),
        AdapterIssue::UnsupportedPropertyValue { name, value } => localizer.format(
            "tool-metadata-unsupported-property-value",
            &[
                ("name", LocalizationValue::Text(name)),
                ("value", LocalizationValue::Text(value)),
            ],
        ),
    }
}

/// Recoverable parse issues keep their native meaning without claiming all preserved resources are analyzed.
fn parse_message(issue: &ParseIssue, localizer: &Localizer) -> String {
    match issue {
        ParseIssue::UnsupportedRoot => localizer.text("tool-metadata-unsupported-root"),
        ParseIssue::UnknownKind { namespace, name } => one(
            localizer,
            "tool-metadata-unknown-kind",
            "name",
            &qualified(&PropertyKey {
                namespace: namespace.clone(),
                name: name.clone(),
            }),
        ),
        ParseIssue::InvalidObject => localizer.text("tool-metadata-invalid-object"),
        ParseIssue::DuplicateIdentity(_) => localizer.text("tool-metadata-duplicate-identity"),
        ParseIssue::UnsupportedValue => localizer.text("tool-metadata-unsupported-value"),
    }
}

/// Present a `QName` explicitly; a missing namespace is different from a guessed native prefix.
fn qualified(key: &PropertyKey) -> String {
    key.namespace.as_ref().map_or_else(
        || key.name.clone(),
        |namespace| format!("{{{namespace}}}{}", key.name),
    )
}

/// One-field reasons share the same Fluent argument handling, including Unicode and escaping.
fn one(localizer: &Localizer, key: &str, name: &str, value: &str) -> String {
    localizer.format(key, &[(name, LocalizationValue::Text(value))])
}
