//! Real separated-descriptor acceptance; platform execution remains an explicit external gate.

use std::{
    fs,
    path::{Path, PathBuf},
};

use eska::project_core::{
    MetadataKind, ProjectName, creation,
    metadata_parser::{self, ParsedDescriptor, PropertiesMode},
    platform_settings::PlatformVersion,
    source_format::{
        AcceptedBase, AcceptedRef, FileOrigin, LayoutPublication, MD_NAMESPACE, NativeUuid,
        Operation, PropertyName, PropertyValue, SourcePath, SourceReader, export_designer,
        import_designer_resources, prepare_accepted_descriptor_layout,
        prepare_accepted_resource_layout, prepare_descriptor_layout, prepare_property_edit,
        prepare_protected, prepare_resource_layout,
    },
};

/// Validate one real C1 import, native semantic parity and a single standalone form edit.
#[test]
#[ignore = "Requires explicit real native input and a new exclusively owned playground output"]
fn real_separated_descriptor_acceptance() {
    let (input, output) = paths("ESKA_FORMAT_XML_INPUT");
    let imported = creation::import_separated_project(
        &input,
        &output.join("project"),
        ProjectName::parse("descriptor-acceptance".into()).unwrap(),
        Some(&PlatformVersion::parse("8.3.27.2325").unwrap()),
        &[],
    )
    .unwrap();
    let source = imported.project.source();
    let base =
        AcceptedBase::open(&output.join("project/.eska/accepted"), &imported.accepted).unwrap();
    let reader = SourceReader::open(source).unwrap();
    assert_eq!(reader.manifest().schema_version, 2);
    let Corpus {
        objects,
        descriptors,
        target,
    } = parity(&input, &reader, &base);
    let original = prepare_protected(source, &base, &output.join("generations"), true).unwrap();
    let Target {
        owner,
        native,
        uuid,
        before,
    } = target;
    let after = "ESKA standalone descriptor acceptance";
    let control = output.join("control");
    super::copy_tree(&input, &control);
    let path = control.join(native.as_path());
    let edited = native_comment(
        &fs::read_to_string(&path).unwrap(),
        uuid.as_str(),
        &before,
        after,
    );
    fs::write(&path, edited).unwrap();
    let original_sources = super::baseline::tree(source);
    let edit = prepare_property_edit(
        source,
        &base,
        &uuid,
        PropertyName::Comment,
        PropertyValue::Text(after.into()),
    )
    .unwrap();
    assert_eq!(edit.path(), &owner);
    edit.publish(&base).unwrap();
    let mut expected_sources = original_sources;
    expected_sources.insert(
        owner.as_path().into(),
        Some(edit.updated().as_bytes().to_vec()),
    );
    assert_eq!(super::baseline::tree(source), expected_sources);
    let prepared = prepare_protected(source, &base, &output.join("generations"), true).unwrap();
    assert!(prepared.diagnostics().is_empty());
    let changed = prepared
        .files()
        .filter(|file| original.source_map().file(&file.path).unwrap().sha256 != file.sha256)
        .collect::<Vec<_>>();
    assert_eq!(changed.len(), 1);
    assert_eq!(changed[0].path, native);
    let report = serde_json::json!({
        "schema_version":1, "source_layout_version":2,
        "accepted_base_sha256":imported.accepted.sha256, "source_owners":reader.owner_paths().count(),
        "native_descriptors":descriptors,"metadata_objects":objects,"semantic_parity":"All",
        "changed_native_files":1,"changed_source_files":1,"protected_exclusions":0,
        "native_files":prepared.files().count(),"platform_build_and_dump":"pending"
    });
    fs::write(
        output.join("preparation.json"),
        serde_json::to_vec_pretty(&report).unwrap(),
    )
    .unwrap();
    println!("{report}");
}

/// Migrate real family YAML without reaccepting working edits or modifying the original source.
#[test]
#[ignore = "Requires explicit real family input and a new exclusively owned playground output"]
fn real_descriptor_migration_acceptance() {
    let (input, output) = paths("ESKA_FORMAT_SOURCE_INPUT");
    let before = super::baseline::tree(&input);
    let plan = prepare_descriptor_layout(&input).unwrap();
    assert!(!plan.is_empty());
    let source = output.join("source");
    assert_eq!(plan.publish(&source).unwrap(), LayoutPublication::Created);
    assert_eq!(plan.publish(&source).unwrap(), LayoutPublication::Unchanged);
    assert!(prepare_descriptor_layout(&source).unwrap().is_empty());
    let original = SourceReader::open(&input).unwrap();
    let migrated = SourceReader::open(&source).unwrap();
    let (objects, descriptors) = migration_parity(&original, &migrated);
    export_designer(&input, &output.join("native-before")).unwrap();
    export_designer(&source, &output.join("native-after")).unwrap();
    assert_eq!(
        super::baseline::tree(&output.join("native-before")),
        super::baseline::tree(&output.join("native-after"))
    );
    assert_eq!(super::baseline::tree(&input), before);
    let report = serde_json::json!({
        "schema_version":1,"source_layout_version":2,
        "native_descriptors":descriptors,"metadata_objects":objects,
        "semantic_parity":"All","native_bytes_equal":true,
        "working_source_preserved":true,"repeat":"unchanged","next_plan":"empty",
        "accepted_base_migrated":false
    });
    fs::write(
        output.join("migration.json"),
        serde_json::to_vec_pretty(&report).unwrap(),
    )
    .unwrap();
    println!("{report}");
}

/// Relocate real independently pinned C1 evidence and validate the complete protected consumer.
#[test]
#[ignore = "Requires explicit real accepted input, independent old pin and a new playground output"]
fn real_accepted_descriptor_migration_acceptance() {
    let (input, output) = paths("ESKA_FORMAT_ACCEPTED_INPUT");
    let reference = AcceptedRef {
        sha256: std::env::var("ESKA_FORMAT_ACCEPTED_SHA256").unwrap(),
    };
    let before = super::baseline::tree(&input);
    let base = AcceptedBase::open(&input, &reference).unwrap();
    let plan = prepare_accepted_descriptor_layout(&base).unwrap();
    assert!(!plan.is_empty());
    assert_eq!(plan.previous_reference(), &reference);
    assert_ne!(plan.accepted_reference(), &reference);
    let accepted = output.join("accepted");
    assert_eq!(plan.publish(&accepted).unwrap(), LayoutPublication::Created);
    assert_eq!(
        plan.publish(&accepted).unwrap(),
        LayoutPublication::Unchanged
    );
    let migrated_base = AcceptedBase::open(&accepted, plan.accepted_reference()).unwrap();
    assert!(
        prepare_accepted_descriptor_layout(&migrated_base)
            .unwrap()
            .is_empty()
    );
    let original = output.join("original-source");
    let migrated = output.join("source");
    base.restore(&original).unwrap();
    migrated_base.restore(&migrated).unwrap();
    let reader = SourceReader::open(&original).unwrap();
    let (objects, descriptors) = migration_parity(&reader, &SourceReader::open(&migrated).unwrap());
    let decisions = policy_parity(&reader, &base, &migrated_base);
    let old_generation =
        prepare_protected(&original, &base, &output.join("old-generations"), true).unwrap();
    let new_generation =
        prepare_protected(&migrated, &migrated_base, &output.join("generations"), true).unwrap();
    assert!(old_generation.diagnostics().is_empty());
    assert!(new_generation.diagnostics().is_empty());
    assert_eq!(
        super::baseline::tree(old_generation.xml()),
        super::baseline::tree(new_generation.xml())
    );
    assert_eq!(super::baseline::tree(&input), before);
    let report = serde_json::json!({
        "schema_version":1,"source_layout_version":2,
        "native_descriptors":descriptors,"metadata_objects":objects,
        "semantic_parity":"All","native_bytes_equal":true,
        "old_accepted_storage_unchanged":true,"old_and_new_pins_differ":true,
        "policy_decisions_equal":decisions,"repeat":"unchanged","next_plan":"empty",
        "protected_exclusions":0,"project_and_ci_pins_updated":false
    });
    fs::write(
        output.join("accepted-reference.json"),
        serde_json::to_vec_pretty(plan.accepted_reference()).unwrap(),
    )
    .unwrap();
    fs::write(
        output.join("migration.json"),
        serde_json::to_vec_pretty(&report).unwrap(),
    )
    .unwrap();
    println!("{report}");
}

/// Fresh import and both migration authorities preserve real payload bytes, context and support.
#[test]
#[ignore = "Requires real XML/accepted input, independent old pin and a new playground output"]
fn real_resource_layout_acceptance() {
    let (input, output) = paths("ESKA_FORMAT_ACCEPTED_INPUT");
    let reference = AcceptedRef {
        sha256: std::env::var("ESKA_FORMAT_ACCEPTED_SHA256").unwrap(),
    };
    let before = super::baseline::tree(&input);
    let base = AcceptedBase::open(&input, &reference).unwrap();
    let plan = prepare_accepted_resource_layout(&base).unwrap();
    let accepted = output.join("accepted");
    assert_eq!(plan.publish(&accepted).unwrap(), LayoutPublication::Created);
    assert_eq!(
        plan.publish(&accepted).unwrap(),
        LayoutPublication::Unchanged
    );
    let relocated = AcceptedBase::open(&accepted, plan.accepted_reference()).unwrap();
    assert!(
        prepare_accepted_resource_layout(&relocated)
            .unwrap()
            .is_empty()
    );
    let original = output.join("original-source");
    let source = output.join("source");
    base.restore(&original).unwrap();
    relocated.restore(&source).unwrap();
    let reader = SourceReader::open(&original).unwrap();
    let current = SourceReader::open(&source).unwrap();
    let (objects, descriptors) = migration_parity(&reader, &current);
    let decisions = policy_parity(&reader, &base, &relocated);
    let working = prepare_resource_layout(&original).unwrap();
    let migrated = output.join("migrated-source");
    assert_eq!(
        working.publish(&migrated).unwrap(),
        LayoutPublication::Created
    );
    assert_eq!(
        working.publish(&migrated).unwrap(),
        LayoutPublication::Unchanged
    );
    assert!(prepare_resource_layout(&migrated).unwrap().is_empty());
    assert_eq!(
        super::baseline::tree(&source),
        super::baseline::tree(&migrated)
    );
    let old_generation =
        prepare_protected(&original, &base, &output.join("old-generations"), true).unwrap();
    let generation =
        prepare_protected(&source, &relocated, &output.join("generations"), true).unwrap();
    assert!(generation.diagnostics().is_empty());
    assert_eq!(
        super::baseline::tree(old_generation.xml()),
        super::baseline::tree(generation.xml())
    );
    let (payloads, moved) = real_resource_import(&output, &base, (objects, descriptors));
    for file in &generation.source_map().files {
        if let FileOrigin::Payload { input, .. } = &file.origin {
            assert_eq!(
                fs::read(source.join(input.path.as_path())).unwrap(),
                fs::read(generation.xml().join(file.path.as_path())).unwrap()
            );
        }
    }
    assert_eq!(super::baseline::tree(&input), before);
    fs::write(
        output.join("accepted-reference.json"),
        serde_json::to_vec_pretty(plan.accepted_reference()).unwrap(),
    )
    .unwrap();
    let report = serde_json::json!({"schema_version":1,"source_layout_version":3,"source_map_version":2,
        "native_descriptors":descriptors,"metadata_objects":objects,"semantic_parity":"All",
        "policy_decisions_equal":decisions,"payloads":payloads,"relocated_payloads":moved,
        "native_bytes_equal":true,"fresh_import_payload_bytes_equal":true,"working_and_accepted_layouts_equal":true,
        "old_accepted_storage_unchanged":true,"native_input_unchanged":true,"repeat":"unchanged","next_plan":"empty",
        "protected_exclusions":0,"platform_build_and_dump":"pending"});
    fs::write(
        output.join("migration.json"),
        serde_json::to_vec_pretty(&report).unwrap(),
    )
    .unwrap();
    println!("{report}");
}

/// Independent native XML parsing and byte comparison validate a fresh v3 import separately.
fn real_resource_import(
    output: &Path,
    base: &AcceptedBase,
    counts: (usize, usize),
) -> (usize, usize) {
    let xml = PathBuf::from(std::env::var_os("ESKA_FORMAT_XML_INPUT").unwrap())
        .canonicalize()
        .unwrap();
    assert!(
        xml.starts_with(
            PathBuf::from(std::env::var_os("ESKA_TEST_ROOT").unwrap())
                .canonicalize()
                .unwrap()
        )
    );
    let xml_before = super::baseline::tree(&xml);
    let fresh = output.join("fresh");
    let manifest = import_designer_resources(&xml, &fresh).unwrap();
    let corpus = parity(&xml, &SourceReader::open(&fresh).unwrap(), base);
    assert_eq!((corpus.objects, corpus.descriptors), counts);
    let mut payloads = 0;
    let mut moved = 0;
    for file in manifest.files.iter().filter(|file| file.is_payload()) {
        assert_eq!(
            fs::read(fresh.join(file.source_path().as_path())).unwrap(),
            fs::read(xml.join(file.path.as_path())).unwrap()
        );
        payloads += 1;
        moved += usize::from(file.source_path() != &file.path);
    }
    assert_eq!(super::baseline::tree(&xml), xml_before);
    (payloads, moved)
}

/// Compare complete family and separated snapshots without confusing source byte coordinates.
fn migration_parity(original: &SourceReader, migrated: &SourceReader) -> (usize, usize) {
    let mut descriptors = 0;
    let mut objects = 0;
    for owner in original.owner_paths() {
        let snapshot = original.read_metadata(owner, PropertiesMode::All).unwrap();
        for document in snapshot.documents {
            let owner = migrated.owner_for_native_main(&document.path).unwrap();
            let current = migrated.read_metadata(owner, PropertiesMode::All).unwrap();
            assert_eq!(current.documents.len(), 1);
            assert_eq!(
                unlocated(document.descriptor.clone()),
                unlocated(current.documents[0].descriptor.clone())
            );
            objects += document.descriptor.objects.len();
            descriptors += 1;
        }
    }
    assert_eq!(migrated.owner_paths().count(), descriptors);
    (objects, descriptors)
}

/// Every accepted inline and external identity retains operation-specific policy after relocation.
fn policy_parity(reader: &SourceReader, original: &AcceptedBase, migrated: &AcceptedBase) -> usize {
    let mut decisions = 0;
    for owner in reader.owner_paths() {
        for declaration in reader.read_owner(owner).unwrap().declarations {
            for operation in [
                Operation::Property(PropertyName::Comment),
                Operation::Module,
                Operation::Resource,
                Operation::Create,
            ] {
                let old = original.permission(&declaration.uuid, operation);
                let new = migrated.permission(&declaration.uuid, operation);
                assert_eq!(
                    (old.allowed, old.state, old.native_reason, old.reason),
                    (new.allowed, new.state, new.native_reason, new.reason)
                );
                decisions += 1;
            }
        }
    }
    decisions
}

/// Native paths and accepted UUIDs identify one independently editable form.
struct Target {
    owner: SourcePath,
    native: SourcePath,
    uuid: NativeUuid,
    before: String,
}

/// Counts and a permitted edit are obtained from the same complete metadata comparison.
struct Corpus {
    objects: usize,
    descriptors: usize,
    target: Target,
}

/// Compare every original descriptor independently, retaining only one allowed edit target.
fn parity(input: &Path, reader: &SourceReader, base: &AcceptedBase) -> Corpus {
    let mut objects = 0;
    let mut descriptors = 0;
    let mut target = None;
    for owner in reader.owner_paths() {
        let declared = reader.declared_owner(owner).unwrap();
        let snapshot = reader.read_metadata(owner, PropertiesMode::All).unwrap();
        assert_eq!(snapshot.documents.len(), 1);
        let document = &snapshot.documents[0];
        let original = fs::read_to_string(input.join(document.path.as_path())).unwrap();
        let native =
            metadata_parser::parse(&original, declared.parent.cloned(), PropertiesMode::All)
                .unwrap();
        objects += native.objects.len();
        descriptors += 1;
        assert!(native.diagnostics.is_empty());
        assert_eq!(unlocated(document.descriptor.clone()), unlocated(native));
        if target.is_none()
            && declared.kind == MetadataKind::Form
            && base
                .permission(declared.uuid, Operation::Property(PropertyName::Comment))
                .allowed
            && let Some(value) = reader
                .read_owner(owner)
                .unwrap()
                .source
                .document
                .descriptor
                .object
                .property(PropertyName::Comment)
                .and_then(comment_text)
        {
            target = Some(Target {
                owner: owner.clone(),
                native: document.path.clone(),
                uuid: declared.uuid.clone(),
                before: value,
            });
        }
    }
    Corpus {
        objects,
        descriptors,
        target: target.expect("real editable standalone form with Comment"),
    }
}

/// Empty and text comments are accepted scalar states; nil and opaque values are not selected.
fn comment_text(value: &PropertyValue) -> Option<String> {
    match value {
        PropertyValue::Text(value) => Some(value.clone()),
        PropertyValue::Empty => Some(String::new()),
        _ => None,
    }
}

/// Exclusively claim a new output inside the sibling playground while keeping native input read-only.
fn paths(input_variable: &str) -> (PathBuf, PathBuf) {
    let playground = PathBuf::from(std::env::var_os("ESKA_TEST_ROOT").unwrap())
        .canonicalize()
        .unwrap();
    let input = PathBuf::from(std::env::var_os(input_variable).unwrap())
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

/// Native control edits only the original text scalar, without using the ESKA writer or editor.
fn native_comment(original: &str, uuid: &str, before: &str, after: &str) -> String {
    let document = roxmltree::Document::parse(original).unwrap();
    let property = document
        .descendants()
        .find(|node| node.attribute("uuid") == Some(uuid))
        .unwrap()
        .children()
        .find(|node| node.has_tag_name((MD_NAMESPACE, "Properties")))
        .unwrap()
        .children()
        .find(|node| node.has_tag_name((MD_NAMESPACE, "Comment")))
        .unwrap();
    let mut updated = original.to_owned();
    if let Some(value) = property.first_child() {
        assert!(value.is_text());
        assert_eq!(value.text(), Some(before));
        updated.replace_range(value.range(), after);
    } else {
        assert!(before.is_empty());
        assert_eq!(property.attributes().len(), 0);
        updated.replace_range(property.range(), &format!("<Comment>{after}</Comment>"));
    }
    updated
}

/// Source byte spans differ by format; every other parsed metadata fact must be identical.
fn unlocated(mut descriptor: ParsedDescriptor) -> ParsedDescriptor {
    for object in &mut descriptor.objects {
        object.range = 0..0;
        for property in object.properties.iter_mut().flatten() {
            property.range = 0..0;
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
