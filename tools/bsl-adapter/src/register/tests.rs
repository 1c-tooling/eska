//! Full register models, required field identities, modes and standard-field presence.

use super::*;
use eska::project_core::metadata_parser::{self, PropertiesMode};

/// Native XML is only a fixture for the common project parser and the selected reference reader.
fn xml(kind: &str, properties: &str, children: &str) -> String {
    format!(
        r#"<MetaDataObject xmlns="http://v8.1c.ru/8.3/MDClasses" xmlns:v8="http://v8.1c.ru/8.1/data/core" xmlns:cfg="http://v8.1c.ru/8.1/data/enterprise/current-config" xmlns:xs="http://www.w3.org/2001/XMLSchema" xmlns:other="urn:other" version="2.20"><{kind} uuid="10000000-0000-4000-8000-000000000001"><Properties><Name>Rates</Name>{properties}</Properties><ChildObjects>{children}</ChildObjects></{kind}></MetaDataObject>"#
    )
}

/// Compare field UUIDs, semantic types, optional values and implicit fields for all four families.
#[test]
fn register_families_match_native_kernel_models() {
    let children = r#"<Dimension uuid="20000000-0000-4000-8000-000000000001"><Properties><Name>Item</Name><DenyIncompleteValues>true</DenyIncompleteValues><Master>true</Master><Indexing>IndexWithAdditionalOrder</Indexing><Type><v8:Type>cfg:CatalogRef.Items</v8:Type></Type></Properties></Dimension><Dimension uuid="20000000-0000-4000-8000-000000000002"><Properties><Name>Optional</Name></Properties></Dimension><Resource uuid="30000000-0000-4000-8000-000000000001"><Properties><Name>Amount</Name><Type><v8:Type>xs:decimal</v8:Type><v8:NumberQualifiers><v8:Digits>15</v8:Digits><v8:FractionDigits>2</v8:FractionDigits></v8:NumberQualifiers></Type></Properties></Resource><Attribute uuid="40000000-0000-4000-8000-000000000001"><Properties><Name>When</Name><Type><v8:Type>xs:dateTime</v8:Type><v8:DateQualifiers><v8:DateFractions>DateTime</v8:DateFractions></v8:DateQualifiers></Type></Properties></Attribute>"#;
    for (kind, properties, native) in [
        (
            "InformationRegister",
            "<InformationRegisterPeriodicity>Second</InformationRegisterPeriodicity><WriteMode>RecorderSubordinate</WriteMode><EnableTotalsSliceFirst>true</EnableTotalsSliceFirst>",
            bsl_metadata::xml_parser::parse_information_register_xml
                as fn(&str) -> bsl_metadata::error::Result<Register>,
        ),
        (
            "AccumulationRegister",
            "<RegisterType>Balance</RegisterType>",
            bsl_metadata::xml_parser::parse_accumulation_register_xml,
        ),
        (
            "AccountingRegister",
            "",
            bsl_metadata::xml_parser::parse_accounting_register_xml,
        ),
        (
            "CalculationRegister",
            "",
            bsl_metadata::xml_parser::parse_calculation_register_xml,
        ),
    ] {
        let text = xml(kind, properties, children);
        let descriptor = metadata_parser::parse(&text, None, PropertiesMode::All).unwrap();
        assert_eq!(
            register(&descriptor).unwrap(),
            native(&text).unwrap(),
            "{kind}"
        );
    }
}

/// The platform catalog supplies implicit fields and periodicity controls Period independently.
#[test]
fn information_register_modes_preserve_optional_periodicity_and_standard_fields() {
    for (properties, periodicity, subordinate, attributes) in [
        ("", None, false, 3),
        (
            "<InformationRegisterPeriodicity>Nonperiodical</InformationRegisterPeriodicity><WriteMode>Independent</WriteMode>",
            Some(RegisterPeriodicity::Nonperiodical),
            false,
            3,
        ),
        (
            "<InformationRegisterPeriodicity>Month</InformationRegisterPeriodicity><WriteMode>RecorderSubordinate</WriteMode>",
            Some(RegisterPeriodicity::Month),
            true,
            4,
        ),
    ] {
        let text = xml("InformationRegister", properties, "");
        let descriptor = metadata_parser::parse(&text, None, PropertiesMode::All).unwrap();
        let model = register(&descriptor).unwrap();
        assert_eq!(model.periodicity(), periodicity);
        assert_eq!(model.is_recorder_subordinate(), subordinate);
        assert_eq!(model.attributes().len(), attributes);
        assert!(model.attributes().iter().all(|field| field.uuid().is_nil()));
    }
}

/// Invalid explicit values are errors at source fields instead of native-parser silent defaults.
#[test]
fn register_enums_and_flags_are_strict_and_located() {
    for (kind, fields, name) in [
        (
            "InformationRegister",
            "<InformationRegisterPeriodicity>Future</InformationRegisterPeriodicity>",
            "InformationRegisterPeriodicity",
        ),
        (
            "InformationRegister",
            "<WriteMode>Future</WriteMode>",
            "WriteMode",
        ),
        (
            "AccumulationRegister",
            "<RegisterType>Future</RegisterType>",
            "RegisterType",
        ),
        (
            "InformationRegister",
            "<EnableTotalsSliceLast>1</EnableTotalsSliceLast>",
            "EnableTotalsSliceLast",
        ),
    ] {
        let text = xml(kind, fields, "");
        let descriptor = metadata_parser::parse(&text, None, PropertiesMode::All).unwrap();
        let error = register(&descriptor).unwrap_err();
        assert_eq!(
            error.issue,
            AdapterIssue::InvalidProperty { name: name.into() }
        );
        assert!(text[error.range].starts_with(&format!("<{name}>")));
    }
}

/// Mandatory field identity and Type cannot be replaced with a random UUID or Unknown.
#[test]
fn invalid_field_identity_and_missing_types_prevent_register_model() {
    for (child, issue) in [
        (
            r#"<Resource uuid="20000000-0000-4000-8000-000000000001"><Properties><Name>Amount</Name></Properties></Resource>"#,
            AdapterIssue::MissingProperty {
                name: "Type".into(),
            },
        ),
        (
            r#"<Attribute uuid="20000000-0000-4000-8000-000000000001"><Properties><Name>Details</Name></Properties></Attribute>"#,
            AdapterIssue::MissingProperty {
                name: "Type".into(),
            },
        ),
        (
            r#"<Dimension uuid="invalid"><Properties><Name>Item</Name></Properties></Dimension>"#,
            AdapterIssue::InvalidUuid,
        ),
    ] {
        let text = xml("InformationRegister", "", child);
        let descriptor = metadata_parser::parse(&text, None, PropertiesMode::All).unwrap();
        let error = register(&descriptor).unwrap_err();
        assert_eq!(error.issue, issue);
        assert!(text[error.range].contains(child));
    }
}

/// Namespace collisions cannot switch the register's flags or its dimension's type.
#[test]
fn register_projection_respects_qualified_property_identity() {
    let text = xml(
        "InformationRegister",
        "<other:EnableTotalsSliceLast>false</other:EnableTotalsSliceLast><EnableTotalsSliceLast>true</EnableTotalsSliceLast>",
        "",
    );
    let descriptor = metadata_parser::parse(&text, None, PropertiesMode::All).unwrap();
    assert!(register(&descriptor).unwrap().enable_totals_slice_last());
}

/// A partial descriptor or a summary must not publish a complete register model.
#[test]
fn incomplete_and_wrong_descriptors_fail_explicitly() {
    let text = xml("InformationRegister", "", "<Future/>");
    let descriptor = metadata_parser::parse(&text, None, PropertiesMode::All).unwrap();
    assert!(!descriptor.diagnostics.is_empty());
    assert_eq!(
        register(&descriptor).unwrap_err().issue,
        AdapterIssue::IncompleteDescriptor
    );
    let descriptor = metadata_parser::parse(
        &xml("InformationRegister", "", ""),
        None,
        PropertiesMode::Summary,
    )
    .unwrap();
    assert_eq!(
        register(&descriptor).unwrap_err().issue,
        AdapterIssue::PropertiesNotLoaded
    );
    let descriptor =
        metadata_parser::parse(&xml("Catalog", "", ""), None, PropertiesMode::All).unwrap();
    assert_eq!(
        register(&descriptor).unwrap_err().issue,
        AdapterIssue::WrongKind {
            actual: MetadataKind::Catalog
        }
    );
}

/// Quarter/Year are valid platform values but cannot be represented by the pinned kernel enum.
#[test]
fn valid_unmodeled_periodicity_never_becomes_nonperiodical_or_missing() {
    for value in ["Quarter", "Year"] {
        let text = xml(
            "InformationRegister",
            &format!("<InformationRegisterPeriodicity>{value}</InformationRegisterPeriodicity>"),
            "",
        );
        let descriptor = metadata_parser::parse(&text, None, PropertiesMode::All).unwrap();
        let error = register(&descriptor).unwrap_err();
        assert_eq!(
            error.issue,
            AdapterIssue::UnsupportedPropertyValue {
                name: "InformationRegisterPeriodicity".into(),
                value: value.into()
            }
        );
        assert!(text[error.range].contains(value));
        let native = bsl_metadata::xml_parser::parse_information_register_xml(&text).unwrap();
        assert!(native.periodicity().is_none());
        assert!(native.virtual_tables().is_empty());
        assert!(
            native
                .attributes()
                .iter()
                .any(|field| field.name_en() == Some("Period"))
        );
    }
}
