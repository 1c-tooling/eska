use super::super::tests::{catalog, source};
use super::*;
use crate::project_core::source_format::SourceReader;
use std::{fs, path::Path};

/// Compare every semantic field while accepting the deliberate change in source byte addresses.
fn without_coordinates(mut descriptor: ParsedDescriptor) -> ParsedDescriptor {
    for object in &mut descriptor.objects {
        object.range = 0..0;
        if let Some(properties) = &mut object.properties {
            for property in properties {
                property.range = 0..0;
            }
        }
    }
    for reference in &mut descriptor.references {
        reference.range = 0..0;
    }
    for diagnostic in &mut descriptor.diagnostics {
        diagnostic.range = 0..0;
    }
    descriptor
}

#[test]
fn every_fixture_descriptor_has_the_same_semantics_for_yaml_and_original_xml() {
    let (_test, root) = source();
    let reader = SourceReader::open(&root).unwrap();
    for mode in [PropertiesMode::Summary, PropertiesMode::All] {
        for path in reader.owner_paths() {
            let projection = reader.read_metadata(path, mode).unwrap();
            for part in projection.documents {
                let xml = fs::read_to_string(
                    Path::new(env!("CARGO_MANIFEST_DIR"))
                        .join("tests/fixtures/eska-format/designer")
                        .join(part.path.as_path()),
                )
                .unwrap();
                let parent = if part.path.as_str() == "Catalogs/Sample/Forms/Main.xml" {
                    Some(
                        MetadataObject::new(
                            MetadataKind::Catalog,
                            "Sample".into(),
                            String::new(),
                            None,
                        )
                        .unwrap()
                        .id()
                        .clone(),
                    )
                } else {
                    None
                };
                let native = metadata_parser::parse(&xml, parent, mode).unwrap();
                assert_eq!(
                    without_coordinates(part.descriptor),
                    without_coordinates(native),
                    "{} {mode:?}",
                    part.path.as_str()
                );
            }
        }
    }
}

#[test]
fn yaml_locations_and_unsaved_values_belong_to_the_shared_model_snapshot() {
    let (_test, root) = source();
    let reader = SourceReader::open(&root).unwrap();
    let text = fs::read_to_string(root.join(catalog().as_path())).unwrap();
    let changed = format!(
        "# Unicode «в начале»\r\n{}",
        text.replace("Editable part", "Новое значение")
    );
    let snapshot = reader
        .parse_metadata(&catalog(), &changed, PropertiesMode::All)
        .unwrap();
    assert_ne!(
        snapshot.source_sha256,
        reader
            .read_metadata(&catalog(), PropertiesMode::All)
            .unwrap()
            .source_sha256
    );
    let catalog = &snapshot.documents[0].descriptor;
    assert_eq!(
        catalog.objects[3].metadata.id().as_str(),
        "catalog:Sample/tabular-section:Items/attribute:Count"
    );
    assert_eq!(
        catalog.objects[0].children[2].as_str(),
        "catalog:Sample/form:Main"
    );
    assert!(changed[catalog.references[0].range.clone()].contains("Main"));
    let properties = catalog.objects[1].properties.as_ref().unwrap();
    assert_eq!(
        properties[1].property.value,
        MetadataValue::Text("Новое значение".into())
    );
    assert!(changed[properties[1].range.clone()].contains("Новое значение"));
    for part in &snapshot.documents {
        for object in &part.descriptor.objects {
            assert!(changed[object.range.clone()].contains(object.metadata.uuid()));
            for property in object.properties.as_ref().unwrap() {
                assert!(!changed[property.range.clone()].is_empty());
            }
        }
    }
    assert!(!root.join("Configuration.xml").exists());
}

#[test]
fn summary_omits_properties_and_full_mode_retains_empty_nil_and_opaque_values() {
    let (_test, root) = source();
    let reader = SourceReader::open(&root).unwrap();
    let summary = reader
        .read_metadata(&catalog(), PropertiesMode::Summary)
        .unwrap();
    assert!(
        summary
            .documents
            .iter()
            .flat_map(|part| &part.descriptor.objects)
            .all(|object| object.properties.is_none())
    );
    assert_eq!(
        summary.documents[0].descriptor.objects[0].synonyms[0].content,
        "Образец"
    );
    let all = reader
        .read_metadata(&catalog(), PropertiesMode::All)
        .unwrap();
    let properties = all.documents[0].descriptor.objects[0]
        .properties
        .as_ref()
        .unwrap();
    let empty = properties
        .iter()
        .find(|property| property.property.key.name == "Comment")
        .unwrap();
    assert_eq!(empty.property.value, MetadataValue::Text(String::new()));
    assert!(empty.property.qualifiers.is_empty());
    let nil = properties
        .iter()
        .find(|property| property.property.key.name == "DefaultPresentation")
        .unwrap();
    assert_eq!(nil.property.value, MetadataValue::Text(String::new()));
    assert_eq!(
        nil.property.qualifiers[0].0.namespace.as_deref(),
        Some(XSI_NAMESPACE)
    );
    assert_eq!(nil.property.qualifiers[0].1, "true");
    let future = properties
        .iter()
        .find(|property| property.property.key.name == "FutureProperty")
        .unwrap();
    let MetadataValue::Record(values) = &future.property.value else {
        panic!("opaque record")
    };
    assert_eq!(
        values[0].value,
        MetadataValue::Text("123456789012345678901234567890.0001".into())
    );
}

#[test]
fn unknown_children_and_unsupported_properties_remain_visible_in_yaml_coordinates() {
    let (_test, root) = source();
    let reader = SourceReader::open(&root).unwrap();
    let mut family = reader.read_owner(&catalog()).unwrap().source;
    let node = &mut family.document.descriptor.object;
    let Section::Properties(properties) = &mut node.sections[1] else {
        panic!("properties")
    };
    properties.push(Property::Preserved {
        fragment: XmlFragment {
            xml: "<Mixed>before<Value/>after</Mixed>".into(),
            namespaces: vec![],
        },
    });
    let Section::Children(children) = &mut node.sections[2] else {
        panic!("children")
    };
    children.push(Child::Preserved {
        fragment: XmlFragment {
            xml: "<FutureChild xmlns='urn:future'/>".into(),
            namespaces: vec![],
        },
    });
    let text = family.to_yaml().unwrap();
    let snapshot = reader
        .parse_metadata(&catalog(), &text, PropertiesMode::All)
        .unwrap();
    let diagnostics = &snapshot.documents[0].descriptor.diagnostics;
    assert_eq!(diagnostics.len(), 2);
    assert_eq!(diagnostics[0].issue, ParseIssue::UnsupportedValue);
    assert!(text[diagnostics[0].range.clone()].contains("Mixed"));
    assert!(
        matches!(&diagnostics[1].issue, ParseIssue::UnknownKind { namespace: Some(namespace), name } if namespace == "urn:future" && name == "FutureChild")
    );
    assert!(text[diagnostics[1].range.clone()].contains("FutureChild"));
}

#[test]
fn logical_identity_collisions_use_the_same_recoverable_rule_as_xml() {
    let (_test, root) = source();
    let reader = SourceReader::open(&root).unwrap();
    let mut family = reader.read_owner(&catalog()).unwrap().source;
    let Section::Children(children) = &mut family.document.descriptor.object.sections[2] else {
        panic!("children")
    };
    let Child::Inline { object } = &children[0] else {
        panic!("attribute")
    };
    let mut duplicate = object.clone();
    duplicate.uuid = NativeUuid::parse("90000000-0000-4000-8000-000000000001").unwrap();
    children.push(Child::Inline { object: duplicate });
    let yaml = family.to_yaml().unwrap();
    let from_yaml = reader
        .parse_metadata(&catalog(), &yaml, PropertiesMode::Summary)
        .unwrap();
    let native = metadata_parser::parse(
        &family.document.descriptor.to_xml().unwrap(),
        None,
        PropertiesMode::Summary,
    )
    .unwrap();
    assert_eq!(
        without_coordinates(from_yaml.documents[0].descriptor.clone()),
        without_coordinates(native)
    );
    assert!(matches!(
        from_yaml.documents[0].descriptor.diagnostics[0].issue,
        ParseIssue::DuplicateIdentity(_)
    ));
}

#[test]
fn opaque_qnames_and_multiple_nested_issues_keep_the_native_projection_rules() {
    let (_test, root) = source();
    let reader = SourceReader::open(&root).unwrap();
    let mut family = reader.read_owner(&catalog()).unwrap().source;
    let Section::Properties(properties) = &mut family.document.descriptor.object.sections[1] else {
        panic!("properties")
    };
    properties.push(Property::Preserved { fragment: XmlFragment {
        xml: "<Type><v8:Type>s:string</v8:Type><v8:Type xmlns:s='urn:custom'>s:string</v8:Type></Type>".into(),
        namespaces: vec![Namespace { prefix: Some("s".into()), uri: "http://www.w3.org/2001/XMLSchema".into() }],
    }});
    properties.push(Property::Preserved { fragment: XmlFragment {
        xml: "<NestedIssues><First>before<Value/>after</First><Second>before<Value/>after</Second></NestedIssues>".into(),
        namespaces: vec![],
    }});
    let yaml = family.to_yaml().unwrap();
    let projected = reader
        .parse_metadata(&catalog(), &yaml, PropertiesMode::All)
        .unwrap();
    let native = metadata_parser::parse(
        &family.document.descriptor.to_xml().unwrap(),
        None,
        PropertiesMode::All,
    )
    .unwrap();
    assert_eq!(
        without_coordinates(projected.documents[0].descriptor.clone()),
        without_coordinates(native)
    );
    assert_eq!(projected.documents[0].descriptor.diagnostics.len(), 2);
    assert!(
        projected.documents[0]
            .descriptor
            .diagnostics
            .iter()
            .all(|issue| yaml[issue.range.clone()].contains("NestedIssues"))
    );
}

#[test]
#[ignore = "Explicit read-only XML/ESKA playground roots, projection mode and fresh report are required"]
fn real_playground_semantic_parity() {
    use crate::project_core::source_format::{FileRole, composition::reference_path};
    use std::{io::Write, path::PathBuf, time::Instant};
    let playground = PathBuf::from(std::env::var_os("ESKA_TEST_ROOT").unwrap())
        .canonicalize()
        .unwrap();
    let source = PathBuf::from(std::env::var_os("ESKA_FORMAT_INPUT").unwrap())
        .canonicalize()
        .unwrap();
    let native = PathBuf::from(std::env::var_os("ESKA_FORMAT_XML_INPUT").unwrap())
        .canonicalize()
        .unwrap();
    let output = PathBuf::from(std::env::var_os("ESKA_FORMAT_PARITY_REPORT").unwrap());
    assert!(source.starts_with(&playground) && native.starts_with(&playground));
    assert!(
        output
            .parent()
            .unwrap()
            .canonicalize()
            .unwrap()
            .starts_with(&playground)
    );
    let mut report = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output)
        .unwrap();
    let started = Instant::now();
    let mode = match std::env::var("ESKA_FORMAT_PROPERTIES_MODE")
        .unwrap()
        .as_str()
    {
        "summary" => PropertiesMode::Summary,
        "all" => PropertiesMode::All,
        _ => panic!("explicit projection mode required"),
    };
    // Retain only independently derived logical parents, never a project-wide parsed DOM/model.
    let mut parents = BTreeMap::new();
    let mut pending = vec![(SourcePath::parse("Configuration.xml").unwrap(), None)];
    while let Some((path, parent)) = pending.pop() {
        assert!(parents.insert(path.clone(), parent.clone()).is_none());
        let parsed = metadata_parser::parse(
            &fs::read_to_string(native.join(path.as_path())).unwrap(),
            parent,
            PropertiesMode::Summary,
        )
        .unwrap();
        let base = if parsed.objects[0].metadata.kind() == MetadataKind::Configuration {
            ""
        } else {
            path.as_str().strip_suffix(".xml").unwrap()
        };
        for reference in parsed.references {
            pending.push((
                reference_path(base, reference.kind, &reference.name).unwrap(),
                reference.id.parent(),
            ));
        }
    }
    let reader = SourceReader::open(&source).unwrap();
    let declared = reader
        .manifest()
        .files
        .iter()
        .filter(|entry| matches!(entry.role, FileRole::Descriptor { .. }))
        .count();
    assert_eq!(parents.len(), declared);
    let (mut owners, mut documents, mut objects, mut references, mut diagnostics) = (0, 0, 0, 0, 0);
    for path in reader.owner_paths() {
        let projected = reader.read_metadata(path, mode).unwrap();
        owners += 1;
        for part in projected.documents {
            let parent = parents
                .remove(&part.path)
                .expect("native independently declares this path");
            let original = metadata_parser::parse(
                &fs::read_to_string(native.join(part.path.as_path())).unwrap(),
                parent,
                mode,
            )
            .unwrap();
            objects += original.objects.len();
            references += original.references.len();
            diagnostics += original.diagnostics.len();
            documents += 1;
            assert!(
                without_coordinates(part.descriptor) == without_coordinates(original),
                "semantic mismatch: {}",
                part.path.as_str()
            );
        }
    }
    assert!(parents.is_empty());
    let result = serde_json::json!({"schema_version":1,"optimized":!cfg!(debug_assertions),
        "mode":format!("{mode:?}"),"owners":owners,"documents":documents,"objects":objects,
        "references":references,"diagnostics":diagnostics,"seconds":started.elapsed().as_secs_f64()});
    report
        .write_all(&serde_json::to_vec_pretty(&result).unwrap())
        .unwrap();
    println!("{result}");
}
