//! Real references preserve declared metadata and compare edits with independent native text changes.

use std::collections::BTreeMap;

use super::*;
use eska::project_core::source_format::{
    Child, MetadataNode, MetadataReference, Property, Section, accept_source,
};

/// One property has one editable value and independent native text-node replacements.
struct Target {
    owner: SourcePath,
    native: SourcePath,
    uuid: NativeUuid,
    property: PropertyName,
    next: PropertyValue,
    replacements: Vec<(usize, String, String)>,
}

/// A fresh C1 import validates named targets, inverse bytes, canonical semantics and two edit domains.
#[test]
#[ignore = "Requires explicit real native input and a new exclusively owned playground output"]
fn real_metadata_reference_acceptance() {
    let (input, output) = paths("ESKA_FORMAT_XML_INPUT");
    let input_before = super::super::baseline::tree(&input);
    let source = output.join("source");
    import_designer_readable(&input, &source).unwrap();
    let storage = output.join("accepted");
    let pin = accept_source(&source, &storage, &[]).unwrap();
    let base = AcceptedBase::open(&storage, &pin).unwrap();
    let reader = SourceReader::open(&source).unwrap();
    let corpus = parity(&input, &reader, &base);
    let forms = declared_forms(&reader);
    let mut targets = BTreeMap::new();
    let mut counts = BTreeMap::new();
    for owner in reader.owner_paths() {
        let snapshot = reader.read_owner(owner).unwrap();
        for part in std::iter::once(&snapshot.source.document).chain(&snapshot.source.nested) {
            visit(
                &part.descriptor.object,
                owner,
                &part.path,
                &forms,
                &base,
                &mut targets,
                &mut counts,
            );
        }
    }
    assert_eq!(
        targets.len(),
        2,
        "one single reference and one named list target are required"
    );
    let unchanged = prepare_protected(&source, &base, &output.join("generations"), true).unwrap();
    assert_eq!(super::super::baseline::tree(unchanged.xml()), input_before);
    let source_before = super::super::baseline::tree(&source);
    let mut expected_sources = source_before.clone();
    let control = output.join("control");
    super::super::copy_tree(&input, &control);
    for target in targets.values() {
        let path = control.join(target.native.as_path());
        let before = fs::read_to_string(&path).unwrap();
        let after = native_edit(&before, target);
        fs::write(path, after).unwrap();
        let edit = prepare_property_edit(
            &source,
            &base,
            &target.uuid,
            target.property,
            target.next.clone(),
        )
        .unwrap();
        assert_eq!(edit.path(), &target.owner);
        edit.publish(&base).unwrap();
        expected_sources.insert(
            target.owner.as_path().into(),
            Some(edit.updated().as_bytes().to_vec()),
        );
        assert!(
            prepare_property_edit(
                &source,
                &base,
                &target.uuid,
                target.property,
                target.next.clone()
            )
            .unwrap()
            .is_empty()
        );
    }
    assert_eq!(super::super::baseline::tree(&source), expected_sources);
    let prepared = prepare_protected(&source, &base, &output.join("generations"), true).unwrap();
    assert!(prepared.diagnostics().is_empty());
    assert_eq!(
        super::super::baseline::tree(prepared.xml()),
        super::super::baseline::tree(&control)
    );
    let changed_native = prepared
        .files()
        .filter(|entry| unchanged.source_map().file(&entry.path).unwrap().sha256 != entry.sha256)
        .count();
    let changed_sources = expected_sources
        .iter()
        .filter(|(path, bytes)| source_before.get(*path) != Some(bytes))
        .count();
    let edited = SourceReader::open(&source).unwrap();
    let changed = parity(&control, &edited, &base);
    assert_eq!(
        (changed.objects, changed.descriptors),
        (corpus.objects, corpus.descriptors)
    );
    assert_eq!(super::super::baseline::tree(&input), input_before);
    AcceptedBase::open(&storage, &pin).unwrap();
    let report = serde_json::json!({"schema_version":1,"source_layout_version":3,"descriptor_schema_version":2,"accepted_base_sha256":pin.sha256,"native_descriptors":corpus.descriptors,"metadata_objects":corpus.objects,"semantic_parity":"All","reference_states":counts,"edit_domains":targets.keys().collect::<Vec<_>>(),"changed_native_files":changed_native,"changed_source_files":changed_sources,"native_bytes_equal":true,"protected_exclusions":0,"no_op":true,"native_input_unchanged":true,"platform_build_and_dump":"pending"});
    fs::write(
        output.join("preparation.json"),
        serde_json::to_vec_pretty(&report).unwrap(),
    )
    .unwrap();
    println!("{report}");
}

/// Detached form declarations supply exact native parent UUIDs without path-derived identity.
fn declared_forms(reader: &SourceReader) -> BTreeMap<NativeUuid, Vec<String>> {
    let mut forms = BTreeMap::<NativeUuid, Vec<String>>::new();
    for owner in reader.owner_paths() {
        if reader.declared_owner(owner).unwrap().kind != MetadataKind::Form {
            continue;
        }
        let snapshot = reader.read_owner(owner).unwrap();
        let root = &snapshot.declarations[0];
        if let Some(parent) = &root.parent {
            forms
                .entry(parent.clone())
                .or_default()
                .push(root.name.clone());
        }
    }
    forms
}

/// Count all active typed values while selecting only allowed edits with known declared alternatives.
fn visit(
    node: &MetadataNode,
    owner: &SourcePath,
    native: &SourcePath,
    forms: &BTreeMap<NativeUuid, Vec<String>>,
    base: &AcceptedBase,
    targets: &mut BTreeMap<&'static str, Target>,
    counts: &mut BTreeMap<&'static str, usize>,
) {
    for section in &node.sections {
        match section {
            Section::Properties(properties) => {
                for property in properties {
                    let Property::Typed { name, value } = property else {
                        continue;
                    };
                    let label = match value {
                        PropertyValue::Reference(_) => "single",
                        PropertyValue::ReferenceList(_) => "list",
                        _ => continue,
                    };
                    *counts.entry(label).or_default() += 1;
                    if targets.contains_key(label)
                        || !base
                            .permission(&node.uuid, Operation::Property(*name))
                            .allowed
                    {
                        continue;
                    }
                    match value {
                        PropertyValue::Reference(old) => {
                            let Some(PropertyValue::Text(object_name)) =
                                node.property(PropertyName::Name)
                            else {
                                continue;
                            };
                            let Some(other) = forms
                                .get(&node.uuid)
                                .into_iter()
                                .flatten()
                                .map(|form| {
                                    format!(
                                        "{}.{}.Form.{form}",
                                        node.kind.designer_tag(),
                                        object_name
                                    )
                                })
                                .find(|target| target != old.as_str())
                            else {
                                continue;
                            };
                            let next = MetadataReference::parse(&other).unwrap();
                            targets.insert(
                                label,
                                Target {
                                    owner: owner.clone(),
                                    native: native.clone(),
                                    uuid: node.uuid.clone(),
                                    property: *name,
                                    next: PropertyValue::Reference(next),
                                    replacements: vec![(0, old.as_str().into(), other)],
                                },
                            );
                        }
                        PropertyValue::ReferenceList(old)
                            if old.items.len() > 1 && old.items[0] != old.items[1] =>
                        {
                            let mut next = old.clone();
                            next.items.swap(0, 1);
                            let replacements = (0..2)
                                .map(|index| {
                                    (
                                        index,
                                        old.items[index].as_str().into(),
                                        next.items[index].as_str().into(),
                                    )
                                })
                                .collect();
                            targets.insert(
                                label,
                                Target {
                                    owner: owner.clone(),
                                    native: native.clone(),
                                    uuid: node.uuid.clone(),
                                    property: *name,
                                    next: PropertyValue::ReferenceList(next),
                                    replacements,
                                },
                            );
                        }
                        _ => {}
                    }
                }
            }
            Section::Children(children) => {
                for child in children {
                    if let Child::Inline { object } = child {
                        visit(object, owner, native, forms, base, targets, counts);
                    }
                }
            }
            Section::Preserved(_) | Section::TabularSectionIdentities(_) => {}
        }
    }
}

/// Native control changes exact property text nodes, independently of the typed writer or YAML.
fn native_edit(input: &str, target: &Target) -> String {
    let document = roxmltree::Document::parse(input).unwrap();
    let object = document
        .descendants()
        .find(|node| node.attribute("uuid") == Some(target.uuid.as_str()))
        .unwrap();
    let properties = object
        .children()
        .find(|node| node.has_tag_name((MD_NAMESPACE, "Properties")))
        .unwrap();
    let property = properties
        .children()
        .find(|node| node.has_tag_name((MD_NAMESPACE, target.property.as_str())))
        .unwrap();
    let values = if matches!(target.next, PropertyValue::ReferenceList(_)) {
        property
            .children()
            .filter(roxmltree::Node::is_element)
            .collect::<Vec<_>>()
    } else {
        vec![property]
    };
    let mut edits = Vec::new();
    for (index, before, after) in &target.replacements {
        let value = values[*index];
        assert_eq!(value.text(), Some(before.as_str()));
        let text = value.children().find(roxmltree::Node::is_text).unwrap();
        edits.push((text.range(), after));
    }
    let mut output = input.to_owned();
    edits.sort_by_key(|(range, _)| range.start);
    for (range, value) in edits.into_iter().rev() {
        output.replace_range(range, value);
    }
    output
}
