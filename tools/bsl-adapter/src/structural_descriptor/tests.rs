//! Structural success cannot imply a payload model; malformed current descriptor fields stay explicit.

use super::*;
use eska::project_core::metadata_parser::{self, PropertiesMode};

const MD: &str = "http://v8.1c.ru/8.3/MDClasses";

/// These leaf declarations use fresh test identities and contain no template resource or query data.
fn xml(kind: &str, fields: &str, children: &str) -> String {
    format!(
        r#"<MetaDataObject xmlns="{MD}" version="2.20"><{kind} uuid="10000000-0000-4000-8000-000000000001"><Properties><Name>Sample</Name>{fields}</Properties><ChildObjects>{children}</ChildObjects></{kind}></MetaDataObject>"#
    )
}

/// The shared parser is the only source representation available to the structural adapter.
fn check(text: &str) -> Result<(), AdapterError> {
    structural_descriptor(&metadata_parser::parse(text, None, PropertiesMode::All).unwrap())
}

/// Absent properties remain absent; every current native template type is structurally recognized.
#[test]
fn language_and_template_descriptors_validate_without_invented_defaults_or_payloads() {
    for kind in ["Language", "Template", "CommonTemplate"] {
        check(&xml(kind, "", "")).unwrap();
    }
    for code in ["", "ru", "en", "custom-code"] {
        check(&xml(
            "Language",
            &format!("<LanguageCode>{code}</LanguageCode>"),
            "",
        ))
        .unwrap();
    }
    for kind in ["Template", "CommonTemplate"] {
        for value in [
            "ActiveDocument",
            "AddIn",
            "BinaryData",
            "DataCompositionAppearanceTemplate",
            "DataCompositionSchema",
            "GeographicalSchema",
            "GraphicalSchema",
            "HTMLDocument",
            "SpreadsheetDocument",
            "TextDocument",
        ] {
            check(&xml(
                kind,
                &format!("<TemplateType>{value}</TemplateType>"),
                "",
            ))
            .unwrap();
        }
    }
}

/// A duplicate or structured present scalar is invalid rather than an absent value or platform default.
#[test]
fn explicit_malformed_fields_keep_their_own_snapshot_ranges() {
    for (kind, field) in [
        ("Language", "LanguageCode"),
        ("Template", "TemplateType"),
        ("CommonTemplate", "TemplateType"),
        ("Language", "Comment"),
    ] {
        let duplicate = xml(kind, &format!("<{field}/><{field}/>"), "");
        let error = check(&duplicate).unwrap_err();
        assert_eq!(
            error.issue,
            AdapterIssue::DuplicateProperty { name: field.into() }
        );
        assert!(duplicate[error.range].starts_with(&format!("<{field}")));
        let structured = xml(
            kind,
            &format!("<{field}><Child>value</Child></{field}>"),
            "",
        );
        let error = check(&structured).unwrap_err();
        assert_eq!(
            error.issue,
            AdapterIssue::InvalidProperty { name: field.into() }
        );
        assert!(structured[error.range].starts_with(&format!("<{field}>")));
    }
    for value in ["", "FutureTemplateType", "datacompositionschema"] {
        let text = xml(
            "Template",
            &format!("<TemplateType>{value}</TemplateType>"),
            "",
        );
        let error = check(&text).unwrap_err();
        assert_eq!(
            error.issue,
            AdapterIssue::UnsupportedPropertyValue {
                name: "TemplateType".into(),
                value: value.into(),
            }
        );
        assert!(text[error.range].starts_with("<TemplateType>"));
    }
}

/// Semantic keys use namespace identity, and summary mode cannot become a complete descriptor.
#[test]
fn qualifiers_identity_and_loaded_property_guards_are_preserved() {
    let text = xml("Language", "<LanguageCode>ru</LanguageCode>", "");
    let alias = text
        .replace(&format!("xmlns=\"{MD}\""), &format!("xmlns:m=\"{MD}\""))
        .replace("<MetaDataObject", "<m:MetaDataObject")
        .replace("</MetaDataObject", "</m:MetaDataObject")
        .replace("<Language", "<m:Language")
        .replace("</Language", "</m:Language")
        .replace("<Properties", "<m:Properties")
        .replace("</Properties", "</m:Properties")
        .replace("<Name", "<m:Name")
        .replace("</Name", "</m:Name")
        .replace("<ChildObjects", "<m:ChildObjects")
        .replace("</ChildObjects", "</m:ChildObjects");
    check(&alias).unwrap();
    let foreign = xml(
        "Language",
        "<LanguageCode xmlns='urn:foreign'><Child/></LanguageCode>",
        "",
    );
    check(&foreign).unwrap();
    let summary = metadata_parser::parse(&text, None, PropertiesMode::Summary).unwrap();
    assert_eq!(
        structural_descriptor(&summary).unwrap_err().issue,
        AdapterIssue::PropertiesNotLoaded
    );
    let invalid = text.replace("10000000-0000-4000-8000-000000000001", "invalid-uuid");
    assert_eq!(
        check(&invalid).unwrap_err().issue,
        AdapterIssue::InvalidUuid
    );
    assert!(matches!(
        check(&xml("Catalog", "", "")).unwrap_err().issue,
        AdapterIssue::WrongKind {
            actual: MetadataKind::Catalog
        }
    ));
}

/// Extra children or unsupported ownership prevent structural success without creating a kernel model.
#[test]
fn leaf_and_ownership_failures_are_not_hidden_by_structure_only_admission() {
    let reference = xml("Template", "", "<Template>ForeignChild</Template>");
    assert_eq!(
        check(&reference).unwrap_err().issue,
        AdapterIssue::IncompleteDescriptor
    );
    let child = xml(
        "Language",
        "",
        r#"<Attribute uuid="20000000-0000-4000-8000-000000000001"><Properties><Name>Child</Name></Properties></Attribute>"#,
    );
    assert_eq!(
        check(&child).unwrap_err().issue,
        AdapterIssue::IncompleteDescriptor
    );
    let unknown = xml("Template", "<ObjectBelonging>Future</ObjectBelonging>", "");
    assert_eq!(
        check(&unknown).unwrap_err().issue,
        AdapterIssue::UnsupportedOwnership {
            value: "Future".into()
        }
    );
    let bad_reference = xml(
        "Language",
        "<ExtendedConfigurationObject>wrong-uuid</ExtendedConfigurationObject>",
        "",
    );
    assert_eq!(
        check(&bad_reference).unwrap_err().issue,
        AdapterIssue::InvalidProperty {
            name: "ExtendedConfigurationObject".into()
        }
    );
}
