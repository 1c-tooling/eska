//! Every domain cause must format in both locales without changing source tokens or severity.

use eska::{project_core::MetadataKind, tool_cli::localization::Locale};

use super::*;

/// Formatting must resolve every Fluent argument while preserving Unicode and qualified source names.
fn assert_translations(issue: &MetadataDiagnosticIssue, source_tokens: &[&str]) {
    let english = Localizer::try_new(Locale::EnUs).unwrap();
    let russian = Localizer::try_new(Locale::RuRu).unwrap();
    let messages = [message(issue, &english), message(issue, &russian)];
    assert_ne!(messages[0], messages[1], "{issue:?}");
    for text in messages {
        assert!(!text.is_empty(), "{issue:?}");
        assert!(!text.contains("tool-metadata-"), "{text}");
        assert!(!text.contains("{ $"), "{text}");
        for token in source_tokens {
            assert!(text.contains(token), "{issue:?}: {text} lacks {token}");
        }
    }
}

/// A human translation must not turn an unsupported feature into an invalid-input error.
fn assert_adapter(issue: AdapterIssue, severity: u8, source_tokens: &[&str]) {
    assert_eq!(adapter_severity(&issue), severity, "{issue:?}");
    assert_translations(&MetadataDiagnosticIssue::Adapter(issue), source_tokens);
}

/// Required model inputs remain errors; each parameterless cause has its own translated explanation.
#[test]
fn required_adapter_inputs_have_distinct_localized_errors() {
    let issues = [
        AdapterIssue::PropertiesNotLoaded,
        AdapterIssue::InvalidUuid,
        AdapterIssue::MissingRoot,
        AdapterIssue::IncompleteDescriptor,
    ];
    for issue in &issues {
        assert_adapter(issue.clone(), 1, &[]);
    }
    for locale in [Locale::EnUs, Locale::RuRu] {
        let localizer = Localizer::try_new(locale).unwrap();
        let messages: std::collections::BTreeSet<_> = issues
            .iter()
            .map(|issue| message(&MetadataDiagnosticIssue::Adapter(issue.clone()), &localizer))
            .collect();
        assert_eq!(messages.len(), issues.len());
    }
    assert_adapter(
        AdapterIssue::WrongKind {
            actual: MetadataKind::ChartOfAccounts,
        },
        1,
        &["chart-of-accounts"],
    );
}

/// Invalid fields are errors while unsupported fields/values are warnings, retaining verbatim details.
#[test]
fn adapter_property_reasons_preserve_source_tokens_in_both_locales() {
    const NAME: &str = "Свойство😀";
    const VALUE: &str = "Будущий<'&>😀";
    for issue in [
        AdapterIssue::DuplicateProperty { name: NAME.into() },
        AdapterIssue::InvalidProperty { name: NAME.into() },
        AdapterIssue::MissingProperty { name: NAME.into() },
    ] {
        assert_adapter(issue, 1, &[NAME]);
    }
    assert_adapter(
        AdapterIssue::UnsupportedOwnership {
            value: VALUE.into(),
        },
        2,
        &[VALUE],
    );
    assert_adapter(
        AdapterIssue::UnresolvedTypeName { text: VALUE.into() },
        1,
        &[VALUE],
    );
    assert_adapter(
        AdapterIssue::UnsupportedStandardAttribute { name: NAME.into() },
        2,
        &[NAME],
    );
    assert_adapter(
        AdapterIssue::UnsupportedPropertyValue {
            name: NAME.into(),
            value: VALUE.into(),
        },
        2,
        &[NAME, VALUE],
    );
}

/// A `QName` keeps its explicit namespace; an absent namespace must not be replaced with a guessed prefix.
#[test]
fn qualified_type_and_parser_reasons_keep_namespace_identity() {
    for (namespace, expected) in [
        (None, "Будущий😀"),
        (Some("urn:test"), "{urn:test}Будущий😀"),
    ] {
        let key = PropertyKey {
            namespace: namespace.map(str::to_owned),
            name: "Будущий😀".into(),
        };
        assert_adapter(
            AdapterIssue::UnsupportedType { key: key.clone() },
            2,
            &[expected],
        );
        assert_adapter(
            AdapterIssue::UnsupportedTypeField { key: key.clone() },
            2,
            &[expected],
        );
        let issue = ParseIssue::UnknownKind {
            namespace: key.namespace,
            name: key.name,
        };
        assert_eq!(parse_severity(&issue), 2);
        assert_translations(&MetadataDiagnosticIssue::Parse(issue), &[expected]);
    }
}

/// Preserved unsupported structures are warnings; invalid/duplicate objects are errors in either locale.
#[test]
fn parser_and_unsupported_kind_reasons_have_explicit_severity() {
    let identity = serde_json::from_str("\"configuration:Demo/catalog:Products\"").unwrap();
    for (issue, severity) in [
        (ParseIssue::UnsupportedRoot, 2),
        (ParseIssue::InvalidObject, 1),
        (ParseIssue::DuplicateIdentity(identity), 1),
        (ParseIssue::UnsupportedValue, 2),
    ] {
        assert_eq!(parse_severity(&issue), severity, "{issue:?}");
        assert_translations(&MetadataDiagnosticIssue::Parse(issue), &[]);
    }
    assert_translations(
        &MetadataDiagnosticIssue::UnsupportedKind(MetadataKind::ChartOfAccounts),
        &["chart-of-accounts"],
    );
}
