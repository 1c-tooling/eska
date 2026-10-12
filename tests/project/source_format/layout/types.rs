//! Real type acceptance checks native control edits and complete metadata parity before 1C builds.

use std::collections::BTreeMap;

use super::*;
use eska::project_core::source_format::{
    Child, ExactUnsigned, MetadataNode, Property, Section, TypeDateFractions, TypeDescription,
    accept_source,
};

/// A target keeps one editable value and an independent native leaf address for its control.
struct TypeTarget {
    owner: SourcePath,
    native: SourcePath,
    uuid: NativeUuid,
    value: Box<TypeDescription>,
    group: &'static str,
    field: &'static str,
    before: String,
    after: String,
}

/// Fresh C1 types preserve all native bytes and permit independently controlled qualifier edits.
#[test]
#[ignore = "Requires explicit real native input and an exclusively owned playground output"]
fn real_type_description_acceptance() {
    let (input, output) = paths("ESKA_FORMAT_XML_INPUT");
    let input_before = super::super::baseline::tree(&input);
    let source = output.join("source");
    import_designer_readable(&input, &source).unwrap();
    let storage = output.join("accepted");
    let pin = accept_source(&source, &storage, &[]).unwrap();
    let base = AcceptedBase::open(&storage, &pin).unwrap();
    let reader = SourceReader::open(&source).unwrap();
    let corpus = parity(&input, &reader, &base);
    let mut targets = BTreeMap::new();
    let mut counts = BTreeMap::new();
    for owner in reader.owner_paths() {
        let snapshot = reader.read_owner(owner).unwrap();
        for part in std::iter::once(&snapshot.source.document).chain(&snapshot.source.nested) {
            visit(
                &part.descriptor.object,
                owner,
                &part.path,
                &base,
                &mut counts,
                &mut targets,
            );
        }
    }
    assert_eq!(
        targets.len(),
        3,
        "number, string and date targets are required"
    );
    let unchanged = prepare_protected(&source, &base, &output.join("generations"), true).unwrap();
    assert_eq!(super::super::baseline::tree(unchanged.xml()), input_before);
    let source_before = super::super::baseline::tree(&source);
    let control = output.join("control");
    super::super::copy_tree(&input, &control);
    let mut source_after = source_before;
    for target in targets.values() {
        let path = control.join(target.native.as_path());
        let updated = native_edit(&fs::read_to_string(&path).unwrap(), target);
        fs::write(path, updated).unwrap();
        let edit = prepare_property_edit(
            &source,
            &base,
            &target.uuid,
            PropertyName::Type,
            PropertyValue::TypeDescription(target.value.clone()),
        )
        .unwrap();
        assert_eq!(edit.path(), &target.owner);
        edit.publish(&base).unwrap();
        source_after.insert(
            target.owner.as_path().into(),
            Some(edit.updated().as_bytes().to_vec()),
        );
        assert!(
            prepare_property_edit(
                &source,
                &base,
                &target.uuid,
                PropertyName::Type,
                PropertyValue::TypeDescription(target.value.clone())
            )
            .unwrap()
            .is_empty()
        );
    }
    assert_eq!(super::super::baseline::tree(&source), source_after);
    let prepared = prepare_protected(&source, &base, &output.join("generations"), true).unwrap();
    assert!(prepared.diagnostics().is_empty());
    assert_eq!(
        super::super::baseline::tree(prepared.xml()),
        super::super::baseline::tree(&control)
    );
    let owners = targets
        .values()
        .map(|target| target.owner.clone())
        .collect::<std::collections::BTreeSet<_>>();
    let natives = targets
        .values()
        .map(|target| target.native.clone())
        .collect::<std::collections::BTreeSet<_>>();
    let report = serde_json::json!({"schema_version":1,"source_layout_version":3,"source_schema_version":2,
        "source_map_version":2,"accepted_base_sha256":pin.sha256,"native_descriptors":corpus.descriptors,
        "metadata_objects":corpus.objects,"semantic_parity":"All","type_states":counts,
        "qualifier_edits":targets.keys().collect::<Vec<_>>(),"changed_source_files":owners.len(),
        "changed_native_files":natives.len(),"native_input_unchanged":true,"no_op":true,
        "native_bytes_equal":true,"protected_exclusions":0,"platform_build_and_dump":"pending"});
    fs::write(
        output.join("preparation.json"),
        serde_json::to_vec_pretty(&report).unwrap(),
    )
    .unwrap();
    assert_eq!(super::super::baseline::tree(&input), input_before);
    println!("{report}");
}

/// Select ordinary native qualifiers only from accepted editable UUIDs and complete typed values.
fn visit(
    node: &MetadataNode,
    owner: &SourcePath,
    native: &SourcePath,
    base: &AcceptedBase,
    counts: &mut BTreeMap<&'static str, usize>,
    targets: &mut BTreeMap<&'static str, TypeTarget>,
) {
    for section in &node.sections {
        match section {
            Section::Properties(properties) => {
                for property in properties {
                    match property {
                        Property::Typed {
                            name: PropertyName::Type,
                            value: PropertyValue::TypeDescription(value),
                        } => {
                            *counts.entry("description").or_default() += 1;
                            if base
                                .permission(&node.uuid, Operation::Property(PropertyName::Type))
                                .allowed
                            {
                                select(node, owner, native, value, targets);
                            }
                        }
                        Property::Typed {
                            name: PropertyName::Type,
                            value: PropertyValue::Empty,
                        } => *counts.entry("empty").or_default() += 1,
                        Property::Typed {
                            name: PropertyName::Type,
                            value: PropertyValue::Nil,
                        } => *counts.entry("nil").or_default() += 1,
                        Property::Preserved { fragment }
                            if ["<Type>", "<Type ", "<Type/"]
                                .iter()
                                .any(|prefix| fragment.xml.starts_with(prefix)) =>
                        {
                            *counts.entry("preserved").or_default() += 1;
                        }
                        _ => {}
                    }
                }
            }
            Section::Children(children) => {
                for child in children {
                    if let Child::Inline { object } = child {
                        visit(object, owner, native, base, counts, targets);
                    }
                }
            }
            Section::Preserved(_) => {}
        }
    }
}

/// Preserve composition and change one observed qualifier, leaving independent control text explicit.
fn select(
    node: &MetadataNode,
    owner: &SourcePath,
    native: &SourcePath,
    value: &TypeDescription,
    targets: &mut BTreeMap<&'static str, TypeTarget>,
) {
    if value.data.alternatives.len() != 1 || targets.values().any(|target| target.uuid == node.uuid)
    {
        return;
    }
    let mut candidate = value.clone();
    let selected = if !targets.contains_key("number")
        && let Some(number) = &mut candidate.data.number
        && let Some(before) = number
            .digits
            .as_ref()
            .map(|value| value.as_str().to_owned())
        && let Ok(digits) = before.parse::<u32>()
        && digits < 30
    {
        let after = (digits + 1).to_string();
        number.digits = Some(ExactUnsigned::parse(&after).unwrap());
        Some(("number", "NumberQualifiers", "Digits", before, after))
    } else if !targets.contains_key("string")
        && let Some(string) = &mut candidate.data.string
        && let Some(before) = string
            .length
            .as_ref()
            .map(|value| value.as_str().to_owned())
        && let Ok(length) = before.parse::<u32>()
        && length > 0
        && length < 1024
    {
        let after = (length + 1).to_string();
        string.length = Some(ExactUnsigned::parse(&after).unwrap());
        Some(("string", "StringQualifiers", "Length", before, after))
    } else if !targets.contains_key("date")
        && let Some(date) = &mut candidate.data.date
        && date.fractions == Some(TypeDateFractions::DateTime)
    {
        date.fractions = Some(TypeDateFractions::Date);
        Some((
            "date",
            "DateQualifiers",
            "DateFractions",
            "DateTime".into(),
            "Date".into(),
        ))
    } else {
        None
    };
    if let Some((key, group, field, before, after)) = selected {
        targets.insert(
            key,
            TypeTarget {
                owner: owner.clone(),
                native: native.clone(),
                uuid: node.uuid.clone(),
                value: Box::new(candidate),
                group,
                field,
                before,
                after,
            },
        );
    }
}

/// Native controls edit the addressed XML text node directly, independently from the typed writer.
fn native_edit(input: &str, target: &TypeTarget) -> String {
    let document = roxmltree::Document::parse(input).unwrap();
    let object = document
        .descendants()
        .find(|node| {
            node.attribute("uuid")
                .is_some_and(|uuid| uuid.eq_ignore_ascii_case(target.uuid.as_str()))
        })
        .unwrap();
    let properties = object
        .children()
        .find(|node| node.has_tag_name((MD_NAMESPACE, "Properties")))
        .unwrap();
    let value = properties
        .children()
        .find(|node| node.has_tag_name((MD_NAMESPACE, "Type")))
        .unwrap();
    let core = "http://v8.1c.ru/8.1/data/core";
    let group = value
        .children()
        .find(|node| node.has_tag_name((core, target.group)))
        .unwrap();
    let field = group
        .children()
        .find(|node| node.has_tag_name((core, target.field)))
        .unwrap();
    let leaf = field.children().find(roxmltree::Node::is_text).unwrap();
    assert_eq!(leaf.text(), Some(target.before.as_str()));
    let mut output = input.to_owned();
    output.replace_range(leaf.range(), &target.after);
    output
}
