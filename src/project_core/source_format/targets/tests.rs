//! Type targets are declared native names with accepted namespace scopes and composition.

use super::*;
use crate::project_core::source_format::{Descriptor, TypeAlternativeKind};

/// Small complete compositions give generated types their real declaring objects and namespaces.
fn documents(alternative: &str) -> BTreeMap<SourcePath, NativeDocument> {
    let mut result = BTreeMap::new();
    for (path, kind, uuid, content) in [
        (
            "Configuration.xml",
            "Configuration",
            '1',
            "<Properties><Name>Root</Name></Properties><ChildObjects><Catalog>Sample</Catalog><Catalog>Other</Catalog><DefinedType>Alias</DefinedType><Constant>Value</Constant><ChartOfCharacteristicTypes>Properties</ChartOfCharacteristicTypes></ChildObjects>".to_owned(),
        ),
        (
            "Catalogs/Sample.xml",
            "Catalog",
            '2',
            generated("Sample", "CatalogRef.Sample", "Ref"),
        ),
        (
            "Catalogs/Other.xml",
            "Catalog",
            '3',
            generated("Other", "CatalogRef.Other", "Ref"),
        ),
        (
            "DefinedTypes/Alias.xml",
            "DefinedType",
            '4',
            generated("Alias", "DefinedType.Alias", "DefinedType").replace(
                "</Properties>",
                "<Type><v8:Type>q:CatalogRef.Sample</v8:Type></Type></Properties>",
            ),
        ),
        (
            "Constants/Value.xml",
            "Constant",
            '5',
            format!("<Properties><Name>Value</Name><Type><v8:Type>{alternative}</v8:Type></Type></Properties>"),
        ),
        (
            "ChartsOfCharacteristicTypes/Properties.xml",
            "ChartOfCharacteristicTypes",
            '6',
            generated("Properties", "Characteristic.Properties", "Characteristic"),
        ),
    ] {
        let path = SourcePath::parse(path).unwrap();
        let xml = format!(
            r#"<MetaDataObject xmlns="http://v8.1c.ru/8.3/MDClasses" xmlns:v8="http://v8.1c.ru/8.1/data/core" xmlns:q="http://v8.1c.ru/8.1/data/enterprise/current-config" xmlns:alias="http://v8.1c.ru/8.1/data/enterprise/current-config" xmlns:foreign="urn:other" xmlns:r="http://v8.1c.ru/8.3/xcf/readable" version="2.20"><{kind} uuid="{uuid}0000000-0000-4000-8000-000000000001">{content}</{kind}></MetaDataObject>"#
        );
        result.insert(
            path.clone(),
            NativeDocument {
                path,
                descriptor: Descriptor::from_xml(&xml).unwrap(),
            },
        );
    }
    result
}

/// Generated type spellings belong to native `InternalInfo`, rather than property or path guesses.
fn generated(name: &str, value: &str, category: &str) -> String {
    format!(
        r#"<InternalInfo><r:GeneratedType name="{value}" category="{category}"><r:TypeId>90000000-0000-4000-8000-000000000001</r:TypeId></r:GeneratedType></InternalInfo><Properties><Name>{name}</Name></Properties>"#
    )
}

/// `QName` URI identity admits aliases, while bare sets and other namespaces need no named target.
#[test]
fn type_targets_use_generated_declarations_and_effective_namespace_uris() {
    for name in [
        "q:CatalogRef.Sample",
        "alias:CatalogRef.Other",
        "q:AnyRef",
        "q:CatalogRef",
        "q:ConstantsSet",
        "foreign:CatalogRef.Missing",
    ] {
        validate_documents(&documents(name)).unwrap();
    }
    let error = validate_documents(&documents("q:CatalogRef.Missing")).unwrap_err();
    assert_eq!(error.code, "source_type_target_missing");
    assert_eq!(
        error.location,
        [
            "50000000-0000-4000-8000-000000000001",
            "Type",
            "alternatives",
            "0"
        ]
    );
}

/// The platform accepts both element spellings and normalizes them when exporting native XML.
#[test]
fn type_targets_preserve_both_alternative_spellings_for_named_targets() {
    for name in [
        "q:CatalogRef.Sample",
        "alias:DefinedType.Alias",
        "q:Characteristic.Properties",
    ] {
        for kind in [TypeAlternativeKind::Type, TypeAlternativeKind::TypeSet] {
            let mut values = documents(name);
            let value = values
                .get_mut(&SourcePath::parse("Constants/Value.xml").unwrap())
                .unwrap();
            set_alternative(&mut value.descriptor.object, kind, name);
            validate_documents(&values).unwrap();
        }
    }
}

/// The current native profile refuses even acyclic named sets inside a defined type.
#[test]
fn defined_types_refuse_nested_named_sets_before_export_and_edit() {
    let accepted = documents("q:CatalogRef.Sample");
    let path = SourcePath::parse("DefinedTypes/Alias.xml").unwrap();
    for name in ["q:DefinedType.Alias", "q:Characteristic.Properties"] {
        for kind in [TypeAlternativeKind::Type, TypeAlternativeKind::TypeSet] {
            let mut updated = accepted[&path].descriptor.object.clone();
            set_alternative(&mut updated, kind, name);
            let error = validate_edit(&accepted, &updated).unwrap_err();
            assert_eq!(error.code, "source_type_set_owner_invalid");
            assert_eq!(error.location.last().unwrap(), "0");
            let mut values = accepted.clone();
            values.get_mut(&path).unwrap().descriptor.object = updated;
            assert_eq!(
                validate_documents(&values).unwrap_err().code,
                "source_type_set_owner_invalid"
            );
        }
    }
}

/// Unknown categories retain existence validation; known set categories must have a valid owner.
#[test]
fn type_targets_preserve_unknown_categories_and_refuse_invalid_known_owners() {
    for (category, expected) in [
        ("Future", None),
        ("DefinedType", Some("source_type_definition_invalid")),
        ("Characteristic", Some("source_type_definition_invalid")),
    ] {
        let mut values = documents("q:CatalogRef.Sample");
        let sample = values
            .get_mut(&SourcePath::parse("Catalogs/Sample.xml").unwrap())
            .unwrap();
        for section in &mut sample.descriptor.object.sections {
            if let Section::Preserved(value) = section {
                value.xml = value
                    .xml
                    .replace("category=\"Ref\"", &format!("category=\"{category}\""));
            }
        }
        match expected {
            Some(code) => assert_eq!(validate_documents(&values).unwrap_err().code, code),
            None => validate_documents(&values).unwrap(),
        }
    }
}

/// Test mutations use the model's actual property and leave the native slot index unchanged.
fn set_alternative(node: &mut MetadataNode, kind: TypeAlternativeKind, name: &str) {
    for section in &mut node.sections {
        if let Section::Properties(properties) = section {
            for property in properties {
                if let Property::Typed {
                    value: PropertyValue::TypeDescription(value),
                    ..
                } = property
                {
                    value.data.alternatives[0].kind = kind;
                    value.data.alternatives[0].name = name.into();
                }
            }
        }
    }
}

/// Undeclared descriptors and unrelated preserved sections cannot supply a missing type target.
#[test]
fn type_targets_ignore_undeclared_objects_and_non_internal_info() {
    let mut values = documents("q:CatalogRef.Missing");
    let mut extra = values[&SourcePath::parse("Catalogs/Other.xml").unwrap()].clone();
    for section in &mut extra.descriptor.object.sections {
        if let Section::Preserved(value) = section {
            value.xml = value.xml.replace("CatalogRef.Other", "CatalogRef.Missing");
        }
    }
    values.insert(SourcePath::parse("Catalogs/Undeclared.xml").unwrap(), extra);
    assert_eq!(
        validate_documents(&values).unwrap_err().code,
        "source_type_target_missing"
    );
    let sample = values
        .get_mut(&SourcePath::parse("Catalogs/Sample.xml").unwrap())
        .unwrap();
    for section in &mut sample.descriptor.object.sections {
        if let Section::Preserved(value) = section {
            value.xml = value
                .xml
                .replace("InternalInfo", "FutureSection")
                .replace("CatalogRef.Sample", "CatalogRef.Missing");
        }
    }
    assert_eq!(
        validate_documents(&values).unwrap_err().code,
        "source_type_target_missing"
    );
}

/// Duplicate native type names are ambiguous, and malformed known declarations are refused.
#[test]
fn type_targets_refuse_ambiguous_and_invalid_generated_names() {
    for (before, after, code) in [
        (
            "CatalogRef.Other",
            "CatalogRef.Sample",
            "source_type_target_ambiguous",
        ),
        (
            "CatalogRef.Other",
            "broken name",
            "source_type_definition_invalid",
        ),
    ] {
        let mut values = documents("q:CatalogRef.Sample");
        let other = values
            .get_mut(&SourcePath::parse("Catalogs/Other.xml").unwrap())
            .unwrap();
        for section in &mut other.descriptor.object.sections {
            if let Section::Preserved(value) = section {
                value.xml = value.xml.replace(before, after);
            }
        }
        assert_eq!(validate_documents(&values).unwrap_err().code, code);
    }
}

/// Working namespace rebinding cannot evade the independently verified target scope in a preview.
#[test]
fn type_edit_targets_use_accepted_scope() {
    let values = documents("q:CatalogRef.Sample");
    let mut updated = values[&SourcePath::parse("DefinedTypes/Alias.xml").unwrap()]
        .descriptor
        .object
        .clone();
    updated.namespaces.push(Namespace {
        prefix: Some("q".into()),
        uri: "urn:other".into(),
    });
    for section in &mut updated.sections {
        if let Section::Properties(properties) = section {
            for property in properties {
                if let Property::Typed {
                    value: PropertyValue::TypeDescription(value),
                    ..
                } = property
                {
                    value.data.alternatives[0].name = "q:CatalogRef.Missing".into();
                }
            }
        }
    }
    assert_eq!(
        validate_edit(&values, &updated).unwrap_err().code,
        "source_type_target_missing"
    );
}

/// Valid comment-only native sections carry no declarations and remain available to the writer.
#[test]
fn type_targets_preserve_comment_only_native_sections() {
    let mut values = documents("q:CatalogRef.Sample");
    let sample = values
        .get_mut(&SourcePath::parse("Catalogs/Sample.xml").unwrap())
        .unwrap();
    sample.descriptor.object.sections.push(Section::Preserved(
        crate::project_core::source_format::XmlFragment {
            xml: "<!-- native annotation -->".into(),
            namespaces: Vec::new(),
        },
    ));
    sample.descriptor.validate().unwrap();
    assert!(
        sample
            .descriptor
            .to_xml()
            .unwrap()
            .contains("<!-- native annotation -->")
    );
    validate_documents(&values).unwrap();
}
