//! Named Type and `TypeSet` values resolve before edits or protected generations are published.

use std::{
    fs,
    path::{Path, PathBuf},
};

use super::{
    baseline::{tree, uuid},
    types::description,
};
use crate::support::TestDir;
use eska::project::source_format::{
    AcceptedBase, ObjectSource, PropertyName, PropertyValue, Section, TypeAlternativeKind,
    accept_source, import_designer, import_designer_readable, prepare_property_edit,
    prepare_protected,
};

/// Both envelopes share native type declarations; only the pinned owner can grant edit permission.
fn prepare(root: &Path, readable: bool, frozen: bool) -> (PathBuf, AcceptedBase) {
    let input = super::fixture(root);
    fs::remove_file(input.join("Ext/ParentConfigurations.bin")).unwrap();
    let path = input.join("Configuration.xml");
    let xml = fs::read_to_string(&path).unwrap();
    fs::write(&path, xml.replace("<Catalog>Sample</Catalog>", "<Catalog>Sample</Catalog><Catalog>Other</Catalog><DefinedType>Narrow</DefinedType><DefinedType>Wide</DefinedType>")).unwrap();
    let path = input.join("Catalogs/Sample.xml");
    let xml = fs::read_to_string(&path).unwrap()
        .replace("xmlns:xsi=", "xmlns:cfg=\"http://v8.1c.ru/8.1/data/enterprise/current-config\" xmlns:alias=\"http://v8.1c.ru/8.1/data/enterprise/current-config\" xmlns:r=\"http://v8.1c.ru/8.3/xcf/readable\" xmlns:xsi=")
        .replace("<InternalInfo>", "<InternalInfo><r:GeneratedType name=\"CatalogRef.Sample\" category=\"Ref\"/>")
        .replace("<Comment>Editable part</Comment>", "<Comment>Editable part</Comment><Type><v8:Type>cfg:CatalogRef.Sample</v8:Type><v8:TypeSet>alias:DefinedType.Narrow</v8:TypeSet></Type>");
    fs::write(path, xml).unwrap();
    fs::create_dir(input.join("DefinedTypes")).unwrap();
    for (kind, name, prefix, type_name, category) in [
        ("Catalog", "Other", '8', "CatalogRef.Other", "Ref"),
        (
            "DefinedType",
            "Narrow",
            'a',
            "DefinedType.Narrow",
            "DefinedType",
        ),
        (
            "DefinedType",
            "Wide",
            'b',
            "DefinedType.Wide",
            "DefinedType",
        ),
    ] {
        let collection = if kind == "Catalog" {
            "Catalogs"
        } else {
            "DefinedTypes"
        };
        fs::write(input.join(format!("{collection}/{name}.xml")), format!(
            r#"<MetaDataObject xmlns="http://v8.1c.ru/8.3/MDClasses" xmlns:r="http://v8.1c.ru/8.3/xcf/readable" xmlns:v8="http://v8.1c.ru/8.1/data/core" xmlns:xs="http://www.w3.org/2001/XMLSchema" xmlns:cfg="http://v8.1c.ru/8.1/data/enterprise/current-config" version="2.20"><{kind} uuid="{}"><InternalInfo><r:GeneratedType name="{type_name}" category="{category}"/></InternalInfo><Properties><Name>{name}</Name>{}</Properties></{kind}></MetaDataObject>"#, uuid(prefix).as_str(), if kind == "DefinedType" { "<Type><v8:Type>xs:decimal</v8:Type></Type>" } else { "" })).unwrap();
    }
    let source = root.join("source");
    if readable {
        import_designer_readable(&input, &source).unwrap();
    } else {
        import_designer(&input, &source).unwrap();
    }
    let storage = root.join("accepted");
    let frozen = if frozen { vec![uuid('2')] } else { Vec::new() };
    let reference = accept_source(&source, &storage, &frozen).unwrap();
    (source, AcceptedBase::open(&storage, &reference).unwrap())
}

/// Alternative element spelling remains editable even when the platform later normalizes it.
#[test]
fn named_targets_preserve_both_native_alternative_spellings() {
    for readable in [false, true] {
        let test = TestDir::new();
        let (source, base) = prepare(&test.0, readable, false);
        let accepted_before = tree(&test.0.join("accepted"));
        for (index, kind) in [
            (0, TypeAlternativeKind::TypeSet),
            (1, TypeAlternativeKind::Type),
        ] {
            let mut next = description(&source);
            next.data.alternatives[index].kind = kind;
            let edit = prepare_property_edit(
                &source,
                &base,
                &uuid('3'),
                PropertyName::Type,
                PropertyValue::TypeDescription(next),
            )
            .unwrap();
            edit.publish(&base).unwrap();
        }
        let working = tree(&source);
        let output = test.0.join("generations");
        let prepared = prepare_protected(&source, &base, &output, true).unwrap();
        assert!(prepared.diagnostics().is_empty());
        let xml = fs::read_to_string(prepared.xml().join("Catalogs/Sample.xml")).unwrap();
        assert!(xml.contains("<v8:TypeSet>cfg:CatalogRef.Sample</v8:TypeSet>"));
        assert!(xml.contains("<v8:Type>alias:DefinedType.Narrow</v8:Type>"));
        assert_eq!(tree(&source), working);
        assert_eq!(tree(&test.0.join("accepted")), accepted_before);
    }
}

/// Nested defined sets are invalid even without a cycle; neither editor nor raw export writes them.
#[test]
fn defined_type_owners_refuse_self_and_acyclic_named_sets() {
    for readable in [false, true] {
        let test = TestDir::new();
        let (source, base) = prepare(&test.0, readable, false);
        let before = tree(&source);
        let accepted_before = tree(&test.0.join("accepted"));
        let file = source.join("DefinedTypes/Narrow/object.yaml");
        let original = fs::read_to_string(&file).unwrap();
        let mut owner = ObjectSource::from_yaml(&original).unwrap();
        let Some(PropertyValue::TypeDescription(value)) = owner
            .document
            .descriptor
            .object
            .property(PropertyName::Type)
        else {
            panic!("defined type description");
        };
        for name in ["cfg:DefinedType.Narrow", "cfg:DefinedType.Wide"] {
            for kind in [TypeAlternativeKind::Type, TypeAlternativeKind::TypeSet] {
                let mut next = value.clone();
                next.data.alternatives[0].kind = kind;
                next.data.alternatives[0].name = name.into();
                let error = prepare_property_edit(
                    &source,
                    &base,
                    &uuid('a'),
                    PropertyName::Type,
                    PropertyValue::TypeDescription(next),
                )
                .unwrap_err();
                assert_eq!(error.code, "source_type_set_owner_invalid");
                assert_eq!(tree(&source), before);
            }
        }
        for section in &mut owner.document.descriptor.object.sections {
            if let Section::Properties(properties) = section {
                for property in properties {
                    if let eska::project::source_format::Property::Typed {
                        value: PropertyValue::TypeDescription(value),
                        ..
                    } = property
                    {
                        value.data.alternatives[0].kind = TypeAlternativeKind::TypeSet;
                        value.data.alternatives[0].name = "cfg:DefinedType.Wide".into();
                    }
                }
            }
            if let Section::Preserved(value) = section {
                value.xml = value
                    .xml
                    .replace("category=\"DefinedType\"", "category=\"Ref\"");
            }
        }
        fs::write(&file, owner.to_yaml().unwrap()).unwrap();
        let working = tree(&source);
        let output = test.0.join("generations");
        let error = prepare_protected(&source, &base, &output, false).unwrap_err();
        assert_eq!(error.error.code, "source_type_set_owner_invalid");
        assert!(!output.exists());
        assert_eq!(tree(&source), working);
        assert_eq!(tree(&test.0.join("accepted")), accepted_before);
    }
}

/// Import preserves native input for inspection, but invalid kinds cannot become an accepted base.
#[test]
fn nested_native_type_sets_refuse_acceptance_in_both_envelopes() {
    for readable in [false, true] {
        let test = TestDir::new();
        let (source, _) = prepare(&test.0, readable, false);
        let source_before = tree(&source);
        let native = test.0.join("xml/DefinedTypes/Narrow.xml");
        fs::write(
            &native,
            fs::read_to_string(&native).unwrap().replace(
                "<v8:Type>xs:decimal</v8:Type>",
                "<v8:TypeSet>cfg:DefinedType.Wide</v8:TypeSet>",
            ),
        )
        .unwrap();
        let before = tree(&test.0.join("xml"));
        let output = test.0.join("invalid");
        if readable {
            import_designer_readable(&test.0.join("xml"), &output)
        } else {
            import_designer(&test.0.join("xml"), &output)
        }
        .unwrap();
        let working = tree(&output);
        let refused = test.0.join("refused");
        let error = accept_source(&output, &refused, &[]).unwrap_err();
        assert_eq!(error.code, "source_type_set_owner_invalid");
        assert!(!refused.exists());
        assert_eq!(tree(&output), working);
        assert_eq!(tree(&test.0.join("xml")), before);
        assert_eq!(tree(&source), source_before);
    }
}

/// Frozen values are restored before named-set validation; strict mode still reports the edit.
#[test]
fn protected_defined_type_sets_restore_frozen_values_before_validation() {
    for readable in [false, true] {
        let test = TestDir::new();
        let (source, _) = prepare(&test.0, readable, false);
        let storage = test.0.join("frozen-accepted");
        let reference = accept_source(&source, &storage, &[uuid('a')]).unwrap();
        let base = AcceptedBase::open(&storage, &reference).unwrap();
        let accepted_before = tree(&storage);
        let file = source.join("DefinedTypes/Narrow/object.yaml");
        fs::write(
            &file,
            fs::read_to_string(&file)
                .unwrap()
                .replace("xs:decimal", "cfg:DefinedType.Wide"),
        )
        .unwrap();
        let before = tree(&source);
        let prepared = prepare_protected(&source, &base, &test.0.join("allowed"), false).unwrap();
        assert_eq!(prepared.diagnostics().len(), 1);
        let xml = fs::read_to_string(prepared.xml().join("DefinedTypes/Narrow.xml")).unwrap();
        assert!(xml.contains("<v8:Type>xs:decimal</v8:Type>"));
        assert!(!xml.contains("cfg:DefinedType.Wide"));
        assert!(prepare_protected(&source, &base, &test.0.join("strict"), true).is_err());
        assert_eq!(tree(&source), before);
        assert_eq!(tree(&storage), accepted_before);
    }
}

/// Existing alternative leaves retain quoting and comments; missing targets cause no file writes.
#[test]
fn named_type_and_type_set_edits_validate_targets_in_both_envelopes() {
    for readable in [false, true] {
        let test = TestDir::new();
        let (source, base) = prepare(&test.0, readable, false);
        let accepted_before = tree(&test.0.join("accepted"));
        let file = source.join("Catalogs/Sample/object.yaml");
        let original = fs::read_to_string(&file)
            .unwrap()
            .replace(
                "\"cfg:CatalogRef.Sample\"",
                "'cfg:CatalogRef.Sample' # target",
            )
            .replace(
                "\"alias:DefinedType.Narrow\"",
                "'alias:DefinedType.Narrow' # set",
            )
            .replace('\n', "\r\n");
        fs::write(&file, &original).unwrap();
        let before = tree(&source);
        let mut value = description(&source);
        value.data.alternatives[0].name = "cfg:CatalogRef.Missing".into();
        let error = prepare_property_edit(
            &source,
            &base,
            &uuid('3'),
            PropertyName::Type,
            PropertyValue::TypeDescription(value.clone()),
        )
        .unwrap_err();
        assert_eq!(error.code, "source_type_target_missing");
        assert_eq!(tree(&source), before);
        value.data.alternatives[0].name = "cfg:CatalogRef.Other".into();
        value.data.alternatives[1].name = "alias:DefinedType.Wide".into();
        let next = PropertyValue::TypeDescription(value);
        let edit =
            prepare_property_edit(&source, &base, &uuid('3'), PropertyName::Type, next.clone())
                .unwrap();
        assert_eq!(
            edit.updated(),
            original
                .replace(
                    "'cfg:CatalogRef.Sample' # target",
                    "'cfg:CatalogRef.Other' # target"
                )
                .replace(
                    "'alias:DefinedType.Narrow' # set",
                    "'alias:DefinedType.Wide' # set"
                )
        );
        edit.publish(&base).unwrap();
        assert!(
            prepare_property_edit(&source, &base, &uuid('3'), PropertyName::Type, next)
                .unwrap()
                .is_empty()
        );
        let prepared =
            prepare_protected(&source, &base, &test.0.join("generations"), true).unwrap();
        assert!(prepared.diagnostics().is_empty());
        let xml = fs::read_to_string(prepared.xml().join("Catalogs/Sample.xml")).unwrap();
        assert!(xml.contains("<v8:Type>cfg:CatalogRef.Other</v8:Type>"));
        assert!(xml.contains("<v8:TypeSet>alias:DefinedType.Wide</v8:TypeSet>"));
        assert!(xml.contains("<r:GeneratedType name=\"CatalogRef.Sample\""));
        assert_eq!(tree(&test.0.join("accepted")), accepted_before);
    }
}

/// Forbidden dangling edits are restored before target validation; allowed ones stop publication.
#[test]
fn protected_named_type_targets_follow_selected_accepted_definitions() {
    for readable in [false, true] {
        for frozen in [false, true] {
            let test = TestDir::new();
            let (source, base) = prepare(&test.0, readable, frozen);
            let accepted_before = tree(&test.0.join("accepted"));
            let file = source.join("Catalogs/Sample/object.yaml");
            let saved = fs::read_to_string(&file).unwrap();
            fs::write(
                &file,
                saved.replace(
                    "\"name\": \"cfg:CatalogRef.Sample\"",
                    "\"name\": \"cfg:CatalogRef.Missing\"",
                ),
            )
            .unwrap();
            let before = tree(&source);
            let generations = test.0.join("generations");
            if frozen {
                let prepared = prepare_protected(&source, &base, &generations, false).unwrap();
                assert!(
                    fs::read_to_string(prepared.xml().join("Catalogs/Sample.xml"))
                        .unwrap()
                        .contains("<v8:Type>cfg:CatalogRef.Sample</v8:Type>")
                );
                assert_eq!(prepared.diagnostics().len(), 1);
                assert!(prepare_protected(&source, &base, &test.0.join("strict"), true).is_err());
            } else {
                let error = prepare_protected(&source, &base, &generations, true).unwrap_err();
                assert_eq!(error.error.code, "source_type_target_missing");
                assert!(!generations.exists());
                let mut object =
                    ObjectSource::from_yaml(&fs::read_to_string(&file).unwrap()).unwrap();
                for section in &mut object.document.descriptor.object.sections {
                    if let Section::Preserved(value) = section {
                        value.xml = value.xml.replace("CatalogRef.Sample", "CatalogRef.Missing");
                    }
                }
                fs::write(&file, object.to_yaml().unwrap()).unwrap();
                let error = prepare_protected(&source, &base, &generations, false).unwrap_err();
                assert_eq!(error.error.code, "source_type_target_missing");
                assert!(!generations.exists());
                fs::write(
                    &file,
                    before[&PathBuf::from("Catalogs/Sample/object.yaml")]
                        .as_ref()
                        .unwrap(),
                )
                .unwrap();
            }
            assert_eq!(tree(&source), before);
            assert_eq!(tree(&test.0.join("accepted")), accepted_before);
        }
    }
}
