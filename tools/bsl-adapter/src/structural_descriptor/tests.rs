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

/// The shared admission predicate is bounded to five reviewed leaves, not every unsupported kind.
#[test]
fn structural_kind_admission_excludes_semantic_and_unreviewed_presentation_families() {
    let reviewed = [
        MetadataKind::Language,
        MetadataKind::Template,
        MetadataKind::CommonTemplate,
        MetadataKind::CommonPicture,
        MetadataKind::CommandGroup,
    ];
    for kind in MetadataKind::ALL {
        assert_eq!(
            is_structural_descriptor_kind(*kind),
            reviewed.contains(kind),
            "{kind:?}"
        );
    }
    for kind in [
        MetadataKind::CommonAttribute,
        MetadataKind::SessionParameter,
        MetadataKind::CommonCommand,
        MetadataKind::Style,
        MetadataKind::StyleItem,
        MetadataKind::Configuration,
    ] {
        assert!(!is_structural_descriptor_kind(kind));
        assert_eq!(
            check(&xml(kind.designer_tag(), "", "")).unwrap_err().issue,
            AdapterIssue::WrongKind { actual: kind }
        );
    }
}

/// Current native flags, localized maps and enums retain absence and extension identity explicitly.
#[test]
fn presentation_leaves_validate_native_fields_without_loading_image_payloads() {
    for kind in ["CommonPicture", "CommandGroup"] {
        check(&xml(kind, "", "")).unwrap();
        check(&xml(kind, "<ObjectBelonging>Adopted</ObjectBelonging><ExtendedConfigurationObject>20000000-0000-4000-8000-000000000002</ExtendedConfigurationObject>", "")).unwrap();
    }
    for value in ["true", "false"] {
        check(&xml("CommonPicture", &format!("<AvailabilityForChoice>{value}</AvailabilityForChoice><AvailabilityForAppearance>{value}</AvailabilityForAppearance>"), "")).unwrap();
    }
    for representation in ["Auto", "Picture", "Text", "PictureAndText"] {
        for category in [
            "ActionsPanel",
            "FormCommandBar",
            "FormNavigationPanel",
            "NavigationPanel",
        ] {
            check(&xml("CommandGroup", &format!("<Representation>{representation}</Representation><Category>{category}</Category><Picture/><ToolTip/>"), "")).unwrap();
        }
    }
    check(&xml("CommandGroup", "<ToolTip xmlns:c='http://v8.1c.ru/8.1/data/core'><c:item><c:lang>ru</c:lang><c:content>Я 😀 &amp; действие</c:content></c:item><c:item><c:content/><c:lang>en</c:lang></c:item></ToolTip>", "")).unwrap();
    for reference in [
        "CommonPicture.Badge",
        "CommonPicture.Картинка",
        "StdPicture.Print",
    ] {
        for transparent in ["true", "false"] {
            check(&xml("CommandGroup", &format!("<Picture xmlns:r='http://v8.1c.ru/8.3/xcf/readable'><r:Ref>{reference}</r:Ref><r:LoadTransparent>{transparent}</r:LoadTransparent></Picture>"), "")).unwrap();
        }
    }
}

/// Namespace aliases are equivalent, while malformed present maps, references and flags clear coverage.
#[test]
fn presentation_field_failures_keep_exact_snapshot_locations() {
    let cases = [
        (
            "CommonPicture",
            "AvailabilityForChoice",
            "<AvailabilityForChoice>1</AvailabilityForChoice>",
        ),
        (
            "CommonPicture",
            "AvailabilityForAppearance",
            "<AvailabilityForAppearance><Child/></AvailabilityForAppearance>",
        ),
        (
            "CommonPicture",
            "AvailabilityForChoice",
            "<AvailabilityForChoice/><AvailabilityForChoice/>",
        ),
        (
            "CommonPicture",
            "Picture",
            "<Picture><Inline>unreviewed</Inline></Picture>",
        ),
        (
            "CommonPicture",
            "SuppressObject",
            "<SuppressObject>CommonPicture.Other</SuppressObject>",
        ),
        (
            "CommandGroup",
            "ToolTip",
            "<ToolTip>not localized</ToolTip>",
        ),
        (
            "CommandGroup",
            "ToolTip",
            "<ToolTip xmlns:c='http://v8.1c.ru/8.1/data/core'><c:item><c:lang>ru</c:lang><c:content>A</c:content></c:item><c:item><c:lang>ru</c:lang><c:content>B</c:content></c:item></ToolTip>",
        ),
        (
            "CommandGroup",
            "ToolTip",
            "<ToolTip xmlns:c='urn:foreign'><c:item><c:lang>ru</c:lang><c:content>A</c:content></c:item></ToolTip>",
        ),
        (
            "CommandGroup",
            "ToolTip",
            "<ToolTip xmlns:c='http://v8.1c.ru/8.1/data/core'><c:item><c:lang>ru</c:lang></c:item></ToolTip>",
        ),
        (
            "CommandGroup",
            "ToolTip",
            "<ToolTip xmlns:c='http://v8.1c.ru/8.1/data/core'><c:item><c:lang>ru</c:lang><c:lang>en</c:lang></c:item></ToolTip>",
        ),
        (
            "CommandGroup",
            "Picture",
            "<Picture>CommonPicture.Badge</Picture>",
        ),
        (
            "CommandGroup",
            "Picture",
            "<Picture xmlns:r='urn:foreign'><r:Ref>CommonPicture.Badge</r:Ref><r:LoadTransparent>true</r:LoadTransparent></Picture>",
        ),
        (
            "CommandGroup",
            "Picture",
            "<Picture xmlns:r='http://v8.1c.ru/8.3/xcf/readable'><r:Ref>CommonPicture.Badge</r:Ref><r:Ref>CommonPicture.Other</r:Ref></Picture>",
        ),
        (
            "CommandGroup",
            "Picture",
            "<Picture xmlns:r='http://v8.1c.ru/8.3/xcf/readable'><r:Ref>Catalog.Badge</r:Ref><r:LoadTransparent>true</r:LoadTransparent></Picture>",
        ),
        (
            "CommandGroup",
            "Picture",
            "<Picture xmlns:r='http://v8.1c.ru/8.3/xcf/readable'><r:Ref>StdPicture.Print.Other</r:Ref><r:LoadTransparent>true</r:LoadTransparent></Picture>",
        ),
        (
            "CommandGroup",
            "Picture",
            "<Picture xmlns:r='http://v8.1c.ru/8.3/xcf/readable'><r:Ref>CommonPicture.Badge</r:Ref><r:LoadTransparent>False</r:LoadTransparent></Picture>",
        ),
        ("CommandGroup", "FutureProperty", "<FutureProperty/>"),
    ];
    for (kind, name, fields) in cases {
        let text = xml(kind, fields, "");
        let error = check(&text).unwrap_err();
        assert!(
            matches!(&error.issue, AdapterIssue::InvalidProperty {name: actual} | AdapterIssue::DuplicateProperty {name: actual} if actual == name),
            "{name}: {error:?}"
        );
        assert!(text[error.range].starts_with(&format!("<{name}")));
    }
}

/// Known enums use exact native tokens; future values retain the explicit field and its source range.
#[test]
fn presentation_enum_failures_keep_exact_snapshot_locations() {
    for (name, value) in [
        ("Representation", "Future"),
        ("Category", ""),
        ("Category", "actionspanel"),
    ] {
        let text = xml("CommandGroup", &format!("<{name}>{value}</{name}>"), "");
        let error = check(&text).unwrap_err();
        assert_eq!(
            error.issue,
            AdapterIssue::UnsupportedPropertyValue {
                name: name.into(),
                value: value.into()
            }
        );
        assert!(text[error.range].starts_with(&format!("<{name}>")));
    }
}

/// The new leaves share the same loaded-property, leaf-child and identity/ownership guards.
#[test]
fn presentation_leaves_preserve_common_descriptor_guards() {
    for kind in ["CommonPicture", "CommandGroup"] {
        let text = xml(kind, "", "");
        let summary = metadata_parser::parse(&text, None, PropertiesMode::Summary).unwrap();
        assert_eq!(
            structural_descriptor(&summary).unwrap_err().issue,
            AdapterIssue::PropertiesNotLoaded
        );
        assert_eq!(
            check(&text.replace("10000000-0000-4000-8000-000000000001", "bad"))
                .unwrap_err()
                .issue,
            AdapterIssue::InvalidUuid
        );
        assert_eq!(
            check(&xml(kind, "<ObjectBelonging>Future</ObjectBelonging>", ""))
                .unwrap_err()
                .issue,
            AdapterIssue::UnsupportedOwnership {
                value: "Future".into()
            }
        );
        assert_eq!(
            check(&xml(
                kind,
                "<ExtendedConfigurationObject>bad</ExtendedConfigurationObject>",
                ""
            ))
            .unwrap_err()
            .issue,
            AdapterIssue::InvalidProperty {
                name: "ExtendedConfigurationObject".into()
            }
        );
        assert_eq!(
            check(&xml(kind, "", "<Template>Child</Template>"))
                .unwrap_err()
                .issue,
            AdapterIssue::IncompleteDescriptor
        );
        assert_eq!(
            check(&xml(kind, "<Comment annotation='unreviewed'/>", ""))
                .unwrap_err()
                .issue,
            AdapterIssue::InvalidProperty {
                name: "Comment".into()
            }
        );
    }
}
