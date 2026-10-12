//! Explicit real-corpus preparation; actual platform builds and fresh dumps are separate gates.

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use eska::project_core::{
    MetadataKind, ProjectName, creation,
    platform_settings::PlatformVersion,
    source_format::{
        AcceptedBase, Child, ExactUnsigned, MD_NAMESPACE, MetadataNode, NativeUuid, Property,
        PropertyName, PropertyValue, Section, SourcePath, SourceReader, prepare_property_edit,
        prepare_protected,
    },
};

/// A target records both source authority and independently editable native coordinates.
struct Target {
    native: SourcePath,
    uuid: NativeUuid,
    property: PropertyName,
    before: String,
    after: String,
    value: PropertyValue,
}

/// Select three ordinary scalar edits, retain the full corpus, and prepare an independent XML control.
#[test]
#[ignore = "Real CF scalar preparation requires explicit read-only native input and fresh playground output"]
fn real_scalar_acceptance() {
    let (input, output) = paths();
    let imported = creation::import_project(
        &input,
        &output.join("project"),
        ProjectName::parse("scalar-acceptance".into()).unwrap(),
        Some(&PlatformVersion::parse("8.3.27.2325").unwrap()),
        &[],
    )
    .unwrap();
    let source = imported.project.source();
    let base =
        AcceptedBase::open(&output.join("project/.eska/accepted"), &imported.accepted).unwrap();
    let reader = SourceReader::open(source).unwrap();
    let targets = select(&input, &reader, &base);
    assert_eq!(
        targets.len(),
        3,
        "real editable bool/integer/enum targets are required"
    );
    let original = prepare_protected(source, &base, &output.join("generations"), true).unwrap();
    let control = output.join("control");
    super::super::copy_tree(&input, &control);
    let mut expected = BTreeMap::new();
    for target in &targets {
        let path = control.join(target.native.as_path());
        let updated = native_edit(&fs::read_to_string(&path).unwrap(), target);
        fs::write(&path, &updated).unwrap();
        expected.insert(target.native.clone(), updated);
        let plan = prepare_property_edit(
            source,
            &base,
            &target.uuid,
            target.property,
            target.value.clone(),
        )
        .unwrap();
        assert!(!plan.is_empty());
        plan.publish(&base).unwrap();
        let unchanged = prepare_property_edit(
            source,
            &base,
            &target.uuid,
            target.property,
            target.value.clone(),
        )
        .unwrap();
        assert!(unchanged.is_empty());
        unchanged.publish(&base).unwrap();
    }
    let converted = prepare_protected(source, &base, &output.join("generations"), true).unwrap();
    assert!(converted.diagnostics().is_empty());
    let previous: BTreeMap<_, _> = original
        .files()
        .map(|entry| (&entry.path, &entry.sha256))
        .collect();
    let changed: Vec<_> = converted
        .files()
        .filter(|entry| previous[&entry.path] != &entry.sha256)
        .collect();
    assert_eq!(changed.len(), expected.len());
    for entry in changed {
        let expected = &expected[&entry.path];
        let actual = fs::read_to_string(converted.xml().join(entry.path.as_path())).unwrap();
        let expected = eska::project_core::source_format::Descriptor::from_xml(expected).unwrap();
        assert_eq!(
            expected,
            eska::project_core::source_format::Descriptor::from_xml(&actual).unwrap()
        );
    }
    let report = serde_json::json!({
        "schema_version":1, "accepted_base_sha256":imported.accepted.sha256,
        "properties":targets.iter().map(|target| target.property).collect::<Vec<_>>(),
        "changed_native_files":expected.len(), "native_files":converted.files().count(),
        "source_owners":reader.owner_paths().count(), "no_op":true, "protected_exclusions":0,
        "prepared_generation":converted.generation(),
        "platform_build_and_dump":"pending",
    });
    fs::write(
        output.join("preparation.json"),
        serde_json::to_vec_pretty(&report).unwrap(),
    )
    .unwrap();
    println!("{report}");
}

/// Claim a new output inside the sibling playground without granting writes to the native input.
fn paths() -> (PathBuf, PathBuf) {
    let playground = PathBuf::from(std::env::var_os("ESKA_TEST_ROOT").unwrap())
        .canonicalize()
        .unwrap();
    let input = PathBuf::from(std::env::var_os("ESKA_FORMAT_XML_INPUT").unwrap())
        .canonicalize()
        .unwrap();
    let output = PathBuf::from(std::env::var_os("ESKA_FORMAT_OUTPUT").unwrap());
    assert!(input.starts_with(&playground));
    assert!(
        output
            .parent()
            .unwrap()
            .canonicalize()
            .unwrap()
            .starts_with(&playground)
    );
    assert!(!output.exists() && !output.starts_with(&input));
    fs::create_dir(&output).unwrap();
    (input, output)
}

/// Discover permitted values from declared owner snapshots, never from a filename or mutable UUID.
fn select(input: &Path, reader: &SourceReader, base: &AcceptedBase) -> Vec<Target> {
    let mut targets = BTreeMap::new();
    for path in reader.owner_paths() {
        let owner = reader.read_owner(path).unwrap();
        for part in std::iter::once(&owner.source.document).chain(&owner.source.nested) {
            let native = fs::read_to_string(input.join(part.path.as_path())).unwrap();
            visit(
                &part.descriptor.object,
                &part.path,
                &native,
                base,
                &mut targets,
            );
        }
        if targets.len() == 3 {
            break;
        }
    }
    targets.into_values().collect()
}

/// Prefer nonzero small code lengths and a text attribute's multiline/fill-check flags.
fn visit(
    node: &MetadataNode,
    path: &SourcePath,
    native: &str,
    base: &AcceptedBase,
    targets: &mut BTreeMap<PropertyName, Target>,
) {
    for section in &node.sections {
        match section {
            Section::Properties(properties) => {
                for property in properties {
                    if let Property::Typed { name, value } = property
                        && !targets.contains_key(name)
                        && base
                            .permission(
                                &node.uuid,
                                eska::project_core::source_format::Operation::Property(*name),
                            )
                            .allowed
                        && let Some((before, after, value)) = candidate(node, *name, value, native)
                    {
                        targets.insert(
                            *name,
                            Target {
                                native: path.clone(),
                                uuid: node.uuid.clone(),
                                property: *name,
                                before,
                                after,
                                value,
                            },
                        );
                    }
                }
            }
            Section::Children(children) => {
                for child in children {
                    if let Child::Inline { object } = child {
                        visit(object, path, native, base, targets);
                    }
                }
            }
            Section::Preserved(_) | Section::TabularSectionIdentities(_) => {}
        }
    }
}

/// Values selected for this gate stay in ordinary native domains; this is no general migration rule.
fn candidate(
    node: &MetadataNode,
    name: PropertyName,
    value: &PropertyValue,
    native: &str,
) -> Option<(String, String, PropertyValue)> {
    match (name, value) {
        (PropertyName::CodeLength, PropertyValue::Integer(value)) => {
            let previous = value.as_str().parse::<u32>().ok()?;
            if !(1..20).contains(&previous) {
                return None;
            }
            let updated = (previous + 1).to_string();
            Some((
                value.as_str().into(),
                updated.clone(),
                PropertyValue::Integer(ExactUnsigned::parse(&updated).unwrap()),
            ))
        }
        (PropertyName::MultiLine, PropertyValue::Boolean(false))
            if node.kind == MetadataKind::Attribute && string_attribute(native, &node.uuid) =>
        {
            Some(("false".into(), "true".into(), PropertyValue::Boolean(true)))
        }
        (PropertyName::FillChecking, PropertyValue::Enumeration(value))
            if value == "DontCheck" && node.kind == MetadataKind::Attribute =>
        {
            Some((
                value.clone(),
                "ShowError".into(),
                PropertyValue::Enumeration("ShowError".into()),
            ))
        }
        _ => None,
    }
}

/// Check actual native type context before selecting a multiline-string editing example.
fn string_attribute(input: &str, uuid: &NativeUuid) -> bool {
    let document = roxmltree::Document::parse(input).unwrap();
    document
        .descendants()
        .find(|node| node.attribute("uuid") == Some(uuid.as_str()))
        .and_then(|node| {
            node.children()
                .find(|child| child.has_tag_name((MD_NAMESPACE, "Properties")))
        })
        .and_then(|properties| {
            properties
                .children()
                .find(|child| child.has_tag_name((MD_NAMESPACE, "Type")))
        })
        .is_some_and(|types| {
            types.children().any(|child| {
                child.is_element()
                    && child.tag_name().name() == "Type"
                    && child.text() == Some("xs:string")
            })
        })
}

/// The control edits original XML text directly and does not call the source writer or editor.
fn native_edit(input: &str, target: &Target) -> String {
    let document = roxmltree::Document::parse(input).unwrap();
    let node = document
        .descendants()
        .find(|node| node.attribute("uuid") == Some(target.uuid.as_str()))
        .unwrap();
    let properties = node
        .children()
        .find(|child| child.has_tag_name((MD_NAMESPACE, "Properties")))
        .unwrap();
    let property = properties
        .children()
        .find(|child| child.has_tag_name((MD_NAMESPACE, target.property.as_str())))
        .unwrap();
    assert!(property.children().all(|child| child.is_text()));
    let value = property.first_child().unwrap();
    assert_eq!(value.text(), Some(target.before.as_str()));
    let mut result = input.to_owned();
    result.replace_range(value.range(), &target.after);
    result
}
