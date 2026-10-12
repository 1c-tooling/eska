//! Real named type edits are compared with independent native text changes and declared targets.

use std::collections::{BTreeMap, BTreeSet};

use super::*;
use eska::project_core::source_format::{
    Child, FileRole, MetadataNode, Property, Section, TypeAlternativeKind, TypeDescription,
    accept_source,
};

const CONFIGURATION_NAMESPACE: &str = "http://v8.1c.ru/8.1/data/enterprise/current-config";
const READABLE_NAMESPACE: &str = "http://v8.1c.ru/8.3/xcf/readable";

/// Native controls retain numeric types for columns whose managed forms require aggregation.
struct NativeTypes {
    names: BTreeSet<String>,
    numeric_definitions: BTreeSet<String>,
}

/// Each selected alternative has one source owner and an independent native text-node control.
struct Target {
    owner: SourcePath,
    native: SourcePath,
    uuid: NativeUuid,
    next: Box<TypeDescription>,
    before: String,
    after: String,
    tag: &'static str,
}

/// A fresh C1 validates every named type target and two actual reference/defined-type edits.
#[test]
#[ignore = "Requires explicit real native input and a new exclusively owned playground output"]
fn real_named_type_target_acceptance() {
    let (input, output) = paths("ESKA_FORMAT_XML_INPUT");
    let input_before = super::super::baseline::tree(&input);
    let source = output.join("source");
    import_designer_readable(&input, &source).unwrap();
    let storage = output.join("accepted");
    let pin = accept_source(&source, &storage, &[]).unwrap();
    let base = AcceptedBase::open(&storage, &pin).unwrap();
    let base_before = super::super::baseline::tree(&storage);
    let reader = SourceReader::open(&source).unwrap();
    let corpus = parity(&input, &reader, &base);
    let names = generated_names(&input, &reader);
    let mut targets = BTreeMap::new();
    let mut counts = BTreeMap::new();
    for owner in reader.owner_paths() {
        let snapshot = reader.read_owner(owner).unwrap();
        for part in std::iter::once(&snapshot.source.document).chain(&snapshot.source.nested) {
            visit(
                &part.descriptor.object,
                owner,
                &part.path,
                &names,
                &base,
                &mut targets,
                &mut counts,
            );
        }
    }
    assert_eq!(
        targets.len(),
        2,
        "reference and defined-type targets required"
    );
    let unchanged = prepare_protected(&source, &base, &output.join("generations"), true).unwrap();
    assert_eq!(super::super::baseline::tree(unchanged.xml()), input_before);
    let mut expected_source = super::super::baseline::tree(&source);
    let control = output.join("control");
    super::super::copy_tree(&input, &control);
    for target in targets.values() {
        let path = control.join(target.native.as_path());
        fs::write(
            &path,
            native_edit(&fs::read_to_string(&path).unwrap(), target),
        )
        .unwrap();
        let next = PropertyValue::TypeDescription(target.next.clone());
        let edit = prepare_property_edit(
            &source,
            &base,
            &target.uuid,
            PropertyName::Type,
            next.clone(),
        )
        .unwrap();
        assert_eq!(edit.path(), &target.owner);
        edit.publish(&base).unwrap();
        expected_source.insert(
            target.owner.as_path().into(),
            Some(edit.updated().as_bytes().to_vec()),
        );
        assert!(
            prepare_property_edit(&source, &base, &target.uuid, PropertyName::Type, next)
                .unwrap()
                .is_empty()
        );
    }
    assert_eq!(super::super::baseline::tree(&source), expected_source);
    let prepared = prepare_protected(&source, &base, &output.join("generations"), true).unwrap();
    assert!(prepared.diagnostics().is_empty());
    assert_eq!(
        super::super::baseline::tree(prepared.xml()),
        super::super::baseline::tree(&control)
    );
    let owners = targets
        .values()
        .map(|target| &target.owner)
        .collect::<BTreeSet<_>>();
    let native = targets
        .values()
        .map(|target| &target.native)
        .collect::<BTreeSet<_>>();
    let report = serde_json::json!({"schema_version":1,"source_layout_version":3,"descriptor_schema_version":2,
        "accepted_base_sha256":pin.sha256,"native_descriptors":corpus.descriptors,"metadata_objects":corpus.objects,
        "semantic_parity":"All","generated_type_declarations":names.names.len(),"named_alternatives":counts,
        "edit_domains":targets.keys().collect::<Vec<_>>(),"changed_source_files":owners.len(),"changed_native_files":native.len(),
        "native_bytes_equal":true,"protected_exclusions":0,"no_op":true,"native_input_unchanged":true,
        "source_and_base_unchanged":true,"platform_build_and_dump":"pending"});
    fs::write(
        output.join("preparation.json"),
        serde_json::to_vec_pretty(&report).unwrap(),
    )
    .unwrap();
    assert_eq!(super::super::baseline::tree(&storage), base_before);
    assert_eq!(super::super::baseline::tree(&input), input_before);
    println!("{report}");
}

/// Independent native declaration names come only from the imported composition's descriptor paths.
fn generated_names(input: &Path, reader: &SourceReader) -> NativeTypes {
    let mut result = NativeTypes {
        names: BTreeSet::new(),
        numeric_definitions: BTreeSet::new(),
    };
    for file in &reader.manifest().files {
        if !matches!(file.role, FileRole::Descriptor { .. }) {
            continue;
        }
        let text = fs::read_to_string(input.join(file.path.as_path())).unwrap();
        let document = roxmltree::Document::parse(&text).unwrap();
        for node in document
            .descendants()
            .filter(|node| node.has_tag_name((READABLE_NAMESPACE, "GeneratedType")))
        {
            if node
                .parent()
                .is_some_and(|node| node.has_tag_name((MD_NAMESPACE, "InternalInfo")))
            {
                let name = node.attribute("name").unwrap();
                assert!(result.names.insert(name.into()));
                if let Some(owner) = node.parent().and_then(|node| node.parent())
                    && owner.has_tag_name((MD_NAMESPACE, "DefinedType"))
                    && let Some(properties) = owner
                        .children()
                        .find(|node| node.has_tag_name((MD_NAMESPACE, "Properties")))
                    && let Some(description) = properties
                        .children()
                        .find(|node| node.has_tag_name((MD_NAMESPACE, "Type")))
                    && numeric(description)
                {
                    result.numeric_definitions.insert(name.into());
                }
            }
        }
    }
    result
}

/// A single XML Schema decimal alternative stays aggregatable regardless of numeric qualifiers.
fn numeric(description: roxmltree::Node<'_, '_>) -> bool {
    let core = "http://v8.1c.ru/8.1/data/core";
    let mut alternatives = description
        .children()
        .filter(|node| node.has_tag_name((core, "Type")) || node.has_tag_name((core, "TypeSet")));
    let Some(value) = alternatives.next() else {
        return false;
    };
    if alternatives.next().is_some() || !value.has_tag_name((core, "Type")) {
        return false;
    }
    let Some((prefix, local)) = value.text().and_then(|text| text.split_once(':')) else {
        return false;
    };
    local == "decimal"
        && value.lookup_namespace_uri(Some(prefix)) == Some("http://www.w3.org/2001/XMLSchema")
}

/// Count all typed named alternatives and select two allowed values with unchanged native slots.
fn visit(
    node: &MetadataNode,
    owner: &SourcePath,
    native: &SourcePath,
    names: &NativeTypes,
    base: &AcceptedBase,
    targets: &mut BTreeMap<&'static str, Target>,
    counts: &mut BTreeMap<&'static str, usize>,
) {
    for section in &node.sections {
        match section {
            Section::Properties(properties) => {
                for property in properties {
                    if let Property::Typed {
                        name: PropertyName::Type,
                        value: PropertyValue::TypeDescription(value),
                    } = property
                    {
                        for alternative in &value.data.alternatives {
                            if alternative
                                .name
                                .split_once(':')
                                .is_some_and(|(_, local)| names.names.contains(local))
                            {
                                let kind = if alternative.kind == TypeAlternativeKind::Type {
                                    "type"
                                } else {
                                    "type_set"
                                };
                                *counts.entry(kind).or_default() += 1;
                            }
                        }
                        if base
                            .permission(&node.uuid, Operation::Property(PropertyName::Type))
                            .allowed
                        {
                            select(node, owner, native, names, value, targets);
                        }
                    }
                }
            }
            Section::Children(children) => {
                for child in children {
                    if let Child::Inline { object } = child {
                        visit(object, owner, native, names, base, targets, counts);
                    }
                }
            }
            Section::Preserved(_) | Section::TabularSectionIdentities(_) => {}
        }
    }
}

/// Concrete existing `CatalogRef` and `DefinedType` alternatives preserve a set's native binding.
fn select(
    node: &MetadataNode,
    owner: &SourcePath,
    native: &SourcePath,
    names: &NativeTypes,
    value: &TypeDescription,
    targets: &mut BTreeMap<&'static str, Target>,
) {
    if value.data.alternatives.len() != 1 || targets.values().any(|target| target.uuid == node.uuid)
    {
        return;
    }
    let alternative = &value.data.alternatives[0];
    let Some((prefix, local)) = alternative.name.split_once(':') else {
        return;
    };
    let Some((family, _)) = local.split_once('.') else {
        return;
    };
    let (domain, tag) = match (alternative.kind, family) {
        (TypeAlternativeKind::Type, "CatalogRef") => ("reference", "Type"),
        (TypeAlternativeKind::TypeSet, "DefinedType") => ("defined_type", "TypeSet"),
        _ => return,
    };
    if targets.contains_key(domain) {
        return;
    }
    let Some(replacement) = names.names.iter().find(|name| {
        name.split_once('.').is_some_and(|(head, _)| head == family)
            && name.as_str() != local
            && (family != "DefinedType"
                || names.numeric_definitions.contains(local)
                    && names.numeric_definitions.contains(name.as_str()))
    }) else {
        return;
    };
    let after = format!("{prefix}:{replacement}");
    let mut next = Box::new(value.clone());
    next.data.alternatives[0].name.clone_from(&after);
    targets.insert(
        domain,
        Target {
            owner: owner.clone(),
            native: native.clone(),
            uuid: node.uuid.clone(),
            next,
            before: alternative.name.clone(),
            after,
            tag,
        },
    );
}

/// Change the native text node itself; the control does not invoke the type writer or YAML editor.
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
        .find(|node| node.has_tag_name((MD_NAMESPACE, "Type")))
        .unwrap();
    let alternative = property
        .children()
        .find(|node| node.has_tag_name(("http://v8.1c.ru/8.1/data/core", target.tag)))
        .unwrap();
    let (prefix, _) = target.before.split_once(':').unwrap();
    assert_eq!(
        alternative.lookup_namespace_uri(Some(prefix)),
        Some(CONFIGURATION_NAMESPACE)
    );
    assert_eq!(alternative.text(), Some(target.before.as_str()));
    let text = alternative
        .children()
        .find(roxmltree::Node::is_text)
        .unwrap();
    let mut output = input.to_owned();
    output.replace_range(text.range(), &target.after);
    output
}
