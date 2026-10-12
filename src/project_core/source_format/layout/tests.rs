use super::migration::{LayoutPublication, prepare_descriptor_layout};
use super::*;
use crate::project_core::source_format::files::Inventory;
use crate::{
    project_core::{
        metadata_parser::{self, ParsedDescriptor, PropertiesMode},
        source_format::{
            AcceptedBase, NativeUuid, OriginTexts, PropertyName, PropertyValue, SourceReader,
            accept_source, export_designer, import_designer, import_designer_separated,
            prepare_property_edit, prepare_protected,
        },
    },
    test_support::TestDir,
};
use std::{
    fs,
    path::{Path, PathBuf},
};

/// Keep all synthetic inputs and resources within one exclusively owned playground directory.
pub(super) fn native(directory: &TestDir) -> PathBuf {
    let destination = directory.0.join("native");
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/eska-format/designer");
    let mut pending = vec![PathBuf::new()];
    while let Some(relative) = pending.pop() {
        fs::create_dir_all(destination.join(&relative)).unwrap();
        for entry in fs::read_dir(fixture.join(&relative)).unwrap() {
            let entry = entry.unwrap();
            let path = relative.join(entry.file_name());
            if entry.file_type().unwrap().is_dir() {
                pending.push(path);
            } else {
                fs::copy(entry.path(), destination.join(path)).unwrap();
            }
        }
    }
    // The public corpus omits real supplier CFs; a sentinel tests byte provenance, not CF validity.
    fs::create_dir_all(destination.join("Ext/ParentConfigurations")).unwrap();
    fs::write(
        destination.join("Ext/ParentConfigurations/SampleConfiguration.cf"),
        b"synthetic opaque CF sentinel",
    )
    .unwrap();
    let configuration = destination.join("Configuration.xml");
    fs::write(
        &configuration,
        fs::read_to_string(&configuration).unwrap().replace(
            "</ChildObjects>",
            "<Subsystem>Root</Subsystem></ChildObjects>",
        ),
    )
    .unwrap();
    let catalog = destination.join("Catalogs/Sample.xml");
    fs::write(
        &catalog,
        fs::read_to_string(&catalog).unwrap().replace(
            "<Form>Main</Form>",
            "<Template>Main</Template><Command>Run</Command><Form>Main</Form>",
        ),
    )
    .unwrap();
    for (path, kind, name, index, children) in [
        (
            "Catalogs/Sample/Templates/Main.xml",
            "Template",
            "Main",
            1,
            "",
        ),
        ("Catalogs/Sample/Commands/Run.xml", "Command", "Run", 2, ""),
        (
            "Subsystems/Root.xml",
            "Subsystem",
            "Root",
            3,
            "<Subsystem>Child</Subsystem>",
        ),
        (
            "Subsystems/Root/Subsystems/Child.xml",
            "Subsystem",
            "Child",
            4,
            "<Subsystem>Leaf</Subsystem>",
        ),
        (
            "Subsystems/Root/Subsystems/Child/Subsystems/Leaf.xml",
            "Subsystem",
            "Leaf",
            5,
            "",
        ),
    ] {
        let path = destination.join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, format!(
            "<MetaDataObject xmlns=\"http://v8.1c.ru/8.3/MDClasses\" version=\"2.20\"><{kind} uuid=\"90000000-0000-4000-8000-{index:012}\"><Properties><Name>{name}</Name><Comment>Preserved child</Comment></Properties><ChildObjects>{children}</ChildObjects></{kind}></MetaDataObject>"
        )).unwrap();
    }
    destination
}

/// Compare shared semantic values independently of the two source formats' byte coordinates.
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

/// Resolve only explicit authoring paths in tests; no directory scan activates metadata.
fn path(value: &str) -> SourcePath {
    SourcePath::parse(value).unwrap()
}

/// Standalone files retain semantic ancestry and order while inline properties remain with owners.
#[test]
fn separate_import_has_one_descriptor_per_file_and_native_semantic_parity() {
    let directory = TestDir::new();
    let native = native(&directory);
    let source = directory.0.join("separate");
    let manifest = import_designer_separated(&native, &source).unwrap();
    assert_eq!(manifest.schema_version, DESCRIPTOR_LAYOUT_VERSION);
    assert_eq!(manifest.descriptor_contexts.len(), 9);
    let reader = SourceReader::open(&source).unwrap();
    assert_eq!(reader.owner_paths().count(), 9);
    for owner in reader.owner_paths() {
        let identity = reader.declared_owner(owner).unwrap();
        let snapshot = reader.read_owner(owner).unwrap();
        assert!(snapshot.source.nested.is_empty());
        assert_eq!(
            reader.owner_for_native_main(identity.native_path),
            Some(owner)
        );
        for mode in [PropertiesMode::Summary, PropertiesMode::All] {
            let current = reader.read_metadata(owner, mode).unwrap();
            assert_eq!(current.documents.len(), 1);
            let document = current.documents.into_iter().next().unwrap();
            let original = metadata_parser::parse(
                &fs::read_to_string(native.join(identity.native_path.as_path())).unwrap(),
                identity.parent.cloned(),
                mode,
            )
            .unwrap();
            assert_eq!(
                unlocated(document.descriptor),
                unlocated(original),
                "{owner:?} {mode:?}"
            );
        }
    }
    for (owner, expected) in [
        (
            "Catalogs/Sample/Forms/Main/form.yaml",
            "catalog:Sample/form:Main",
        ),
        (
            "Catalogs/Sample/Templates/Main/template.yaml",
            "catalog:Sample/template:Main",
        ),
        (
            "Catalogs/Sample/Commands/Run/command.yaml",
            "catalog:Sample/command:Run",
        ),
        (
            "Subsystems/Root/Subsystems/Child/Subsystems/Leaf/object.yaml",
            "subsystem:Root/subsystem:Child/subsystem:Leaf",
        ),
    ] {
        let snapshot = reader
            .read_metadata(&path(owner), PropertiesMode::All)
            .unwrap();
        assert_eq!(
            snapshot.documents[0].descriptor.objects[0]
                .metadata
                .id()
                .as_str(),
            expected
        );
    }
    let catalog = reader
        .read_metadata(&path("Catalogs/Sample/object.yaml"), PropertiesMode::All)
        .unwrap();
    assert_eq!(catalog.documents[0].descriptor.objects.len(), 4);
    assert_eq!(
        catalog.documents[0]
            .descriptor
            .references
            .iter()
            .map(|item| item.name.as_str())
            .collect::<Vec<_>>(),
        ["Main", "Run", "Main"]
    );
    assert!(!source.join("Catalogs/Sample/Attributes").exists());
    let form = reader
        .declared_form(&path("Catalogs/Sample/Forms/Main.xml"))
        .unwrap();
    assert_eq!(form.owner.as_str(), "Catalogs/Sample/Forms/Main/form.yaml");
    assert_eq!(
        form.dialog.unwrap().as_str(),
        "Catalogs/Sample/Forms/Main/Ext/Form.xml"
    );
}

/// A complete old source has an exact reviewable plan; native outputs are identical across layouts.
#[test]
fn migration_preserves_native_bytes_and_supports_noop_and_repeated_application() {
    let directory = TestDir::new();
    let native = native(&directory);
    let family = directory.0.join("family");
    let separate = directory.0.join("separate");
    let old = import_designer(&native, &family).unwrap();
    let before = Inventory::read(&family).unwrap();
    let plan = prepare_descriptor_layout(&family).unwrap();
    assert!(!plan.is_empty());
    assert_eq!(plan.input_revision(), before.revision);
    assert_eq!(plan.manifest().input_revision, old.input_revision);
    assert!(
        plan.changes()
            .iter()
            .all(
                |change| change.path.as_path().extension() == Some(std::ffi::OsStr::new("yaml"))
                    || change.path.as_str() == "source.json"
            )
    );
    assert!(
        plan.changes()
            .iter()
            .all(|change| change.after.as_ref().is_none_or(|after| {
                let yaml = plan.updated_text(&change.path).unwrap();
                after.bytes == yaml.len() as u64
            }))
    );
    assert_eq!(plan.publish(&separate).unwrap(), LayoutPublication::Created);
    let after = Inventory::read(&separate).unwrap();
    assert_eq!(after.revision, plan.output_revision());
    assert_eq!(Inventory::read(&family).unwrap(), before);
    assert_eq!(
        plan.publish(&separate).unwrap(),
        LayoutPublication::Unchanged
    );
    assert_eq!(Inventory::read(&separate).unwrap(), after);
    let next = prepare_descriptor_layout(&separate).unwrap();
    assert!(next.is_empty());
    assert_eq!(
        next.publish(&separate).unwrap(),
        LayoutPublication::Unchanged
    );
    let copy = directory.0.join("copy");
    assert_eq!(next.publish(&copy).unwrap(), LayoutPublication::Created);
    assert_eq!(Inventory::read(&copy).unwrap(), after);
    let first_xml = directory.0.join("first-xml");
    let second_xml = directory.0.join("second-xml");
    export_designer(&family, &first_xml).unwrap();
    export_designer(&separate, &second_xml).unwrap();
    assert_eq!(
        Inventory::read(&first_xml).unwrap(),
        Inventory::read(&second_xml).unwrap()
    );
}

/// Recovery discards only its private staging file and refuses edits to already published bytes.
#[test]
fn migration_recovers_an_interrupted_write_and_refuses_foreign_or_edited_output() {
    let directory = TestDir::new();
    let family = directory.0.join("family");
    import_designer(&native(&directory), &family).unwrap();
    let plan = prepare_descriptor_layout(&family).unwrap();
    let separate = directory.0.join("separate");
    migration::publication::interrupt(plan.publication_view(), &separate);
    fs::write(
        separate.join(".eska-layout.write"),
        b"interrupted partial payload",
    )
    .unwrap();
    assert_eq!(
        SourceReader::open(&separate).err().unwrap().code,
        "source_layout_incomplete"
    );
    assert_eq!(plan.publish(&separate).unwrap(), LayoutPublication::Resumed);
    assert_eq!(
        Inventory::read(&separate).unwrap().revision,
        plan.output_revision()
    );
    let edited = directory.0.join("edited");
    migration::publication::interrupt(plan.publication_view(), &edited);
    fs::write(edited.join("configuration.yaml"), b"user edit").unwrap();
    let before = Inventory::read(&edited).unwrap();
    assert_eq!(
        plan.publish(&edited).unwrap_err().code,
        "source_layout_destination_mismatch"
    );
    assert_eq!(Inventory::read(&edited).unwrap(), before);
    let foreign = directory.0.join("foreign");
    fs::create_dir(&foreign).unwrap();
    fs::write(foreign.join("foreign.txt"), b"user bytes").unwrap();
    assert!(plan.publish(&foreign).is_err());
    assert_eq!(
        fs::read(foreign.join("foreign.txt")).unwrap(),
        b"user bytes"
    );
}

/// A changed payload invalidates the entire plan before any destination can be claimed.
#[test]
fn migration_rechecks_input_bytes_and_refuses_overlapping_trees() {
    let directory = TestDir::new();
    let family = directory.0.join("family");
    import_designer(&native(&directory), &family).unwrap();
    let plan = prepare_descriptor_layout(&family).unwrap();
    assert_eq!(
        plan.publish(&family.join("nested")).unwrap_err().code,
        "source_tree_overlap"
    );
    fs::write(
        family.join("CommonModules/Tools/Ext/Module.bsl"),
        b"changed bytes",
    )
    .unwrap();
    let destination = directory.0.join("separate");
    assert_eq!(
        plan.publish(&destination).unwrap_err().code,
        "source_input_changed"
    );
    assert!(!destination.exists());
}

/// Splitting preserves authored comments, quotes and the exact main subtree in block/flow YAML.
#[test]
fn migration_moves_cst_subtrees_without_reserializing_authored_values() {
    for (flow, eol) in [(false, "\n"), (false, "\r\n"), (true, "\n"), (true, "\r\n")] {
        let directory = TestDir::new();
        let family = directory.0.join("family");
        import_designer(&native(&directory), &family).unwrap();
        let catalog = path("Catalogs/Sample/object.yaml");
        let original = fs::read_to_string(family.join(catalog.as_path())).unwrap();
        let original = if flow {
            format!(
                "# authored header\n{}\n",
                serde_json::to_string(&ObjectSource::from_yaml(&original).unwrap()).unwrap()
            )
        } else {
            format!("# authored header\n{original}")
                .replace("\"Editable part\"", "'Editable part' # inline comment")
                .replace("\"Preserved form\"", "'Preserved form' # form comment")
        }
        .replace('\n', eol);
        fs::write(family.join(catalog.as_path()), &original).unwrap();
        let plan = prepare_descriptor_layout(&family).unwrap();
        let main = plan.updated_text(&catalog).unwrap();
        assert!(main.contains("# authored header"));
        let (_, before) =
            crate::project_core::source_format::yaml::decode_with_tree(&original).unwrap();
        let (_, after) = crate::project_core::source_format::yaml::decode_with_tree(main).unwrap();
        let steps = [crate::project_core::source_format::yaml::Step::Key(
            "document",
        )];
        let before = crate::project_core::source_format::yaml::range_at(&before, &steps).unwrap();
        let after = crate::project_core::source_format::yaml::range_at(&after, &steps).unwrap();
        assert_eq!(original[before], main[after]);
        let form = plan
            .updated_text(&path("Catalogs/Sample/Forms/Main/form.yaml"))
            .unwrap();
        if !flow {
            assert!(main.contains("'Editable part' # inline comment"));
            assert!(form.contains("'Preserved form' # form comment"));
        }
        for document in [main, form] {
            if eol == "\r\n" {
                assert!(!document.replace("\r\n", "").contains('\n'));
            }
        }
        let output = directory.0.join("separate");
        plan.publish(&output).unwrap();
        assert_eq!(
            fs::read_to_string(family.join(catalog.as_path())).unwrap(),
            original
        );
    }
}

/// Provenance mismatches and unsupported structural changes never become ready metadata.
#[test]
fn reverse_contexts_and_current_composition_are_both_validated_without_eager_owner_reads() {
    let directory = TestDir::new();
    let separate = directory.0.join("separate");
    let manifest = import_designer_separated(&native(&directory), &separate).unwrap();
    let mut invalid = manifest.clone();
    invalid.descriptor_contexts.pop();
    assert_eq!(
        invalid.validate().unwrap_err().code,
        "source_layout_context_invalid"
    );
    let mut invalid = manifest;
    invalid
        .descriptor_contexts
        .iter_mut()
        .find(|context| context.metadata_kind == MetadataKind::Configuration)
        .unwrap()
        .parent = Some(path("Configuration.xml"));
    assert!(invalid.validate().is_err());
    let reader = SourceReader::open(&separate).unwrap();
    let catalog = path("Catalogs/Sample/object.yaml");
    let yaml = fs::read_to_string(separate.join(catalog.as_path())).unwrap();
    let mut source = ObjectSource::from_yaml(&yaml).unwrap();
    for section in &mut source.document.descriptor.object.sections {
        if let crate::project_core::source_format::Section::Children(items) = section {
            items.retain(|item| {
                !matches!(
                    item,
                    crate::project_core::source_format::Child::Reference {
                        metadata_kind: MetadataKind::Form,
                        ..
                    }
                )
            });
        }
    }
    assert_eq!(
        reader
            .parse_metadata(&catalog, &source.to_yaml().unwrap(), PropertiesMode::All)
            .unwrap_err()
            .code,
        "source_declaration_mismatch"
    );
    fs::remove_file(separate.join(catalog.as_path())).unwrap();
    let reopened = SourceReader::open(&separate).unwrap();
    assert_eq!(reopened.owner_paths().count(), 9);
    assert_eq!(
        reopened.read_owner(&catalog).unwrap_err().code,
        "source_file_missing"
    );
    assert_eq!(
        reopened
            .read_owner(&path("Catalogs/Sample/Forms/Main/form.yaml"))
            .unwrap()
            .declarations[0]
            .parent,
        Some(NativeUuid::parse("20000000-0000-4000-8000-000000000001").unwrap())
    );
}

/// Opaque payload names cannot alias descriptor YAML, its parents or migration-owned files.
#[test]
fn separated_map_refuses_payload_aliases_before_materialization() {
    let directory = TestDir::new();
    let manifest =
        import_designer_separated(&native(&directory), &directory.0.join("source")).unwrap();
    let payload = manifest
        .files
        .iter()
        .find(|entry| matches!(entry.role, FileRole::Preserved))
        .unwrap();
    for alias in [
        "Catalogs/Sample/Forms/Main/form.yaml",
        "Catalogs/Sample/Forms/Main/FORM.YAML",
        "Catalogs/Sample/Forms/Main/form.yaml/raw.bin",
        ".eska-layout.pending.json",
        ".eska-layout.pending.json/raw.bin",
        ".eska-layout.write",
    ] {
        let mut invalid = manifest.clone();
        let mut extra = payload.clone();
        extra.path = path(alias);
        invalid.files.push(extra);
        assert_eq!(
            invalid.validate().unwrap_err().code,
            "source_path_collision"
        );
    }
    for alias in [PENDING_RECEIPT, PENDING_WRITE] {
        let mut invalid = manifest.clone();
        invalid.directories.push(path(alias));
        assert_eq!(
            invalid.validate().unwrap_err().code,
            "source_path_collision"
        );
    }
}

/// Local CST editing, inherited freeze and native diagnostic mapping address the standalone form.
#[test]
fn standalone_form_edits_keep_parent_bytes_and_use_its_own_source_map_coordinates() {
    let directory = TestDir::new();
    let source = directory.0.join("source");
    import_designer_separated(&native(&directory), &source).unwrap();
    let reference = accept_source(&source, &directory.0.join("accepted"), &[]).unwrap();
    let base = AcceptedBase::open(&directory.0.join("accepted"), &reference).unwrap();
    let form_uuid = NativeUuid::parse("70000000-0000-4000-8000-000000000001").unwrap();
    let form = path("Catalogs/Sample/Forms/Main/form.yaml");
    let accepted = fs::read_to_string(source.join(form.as_path())).unwrap();
    let before = Inventory::read(&source).unwrap();
    let edit = prepare_property_edit(
        &source,
        &base,
        &form_uuid,
        PropertyName::Comment,
        PropertyValue::Text("Changed form".into()),
    )
    .unwrap();
    assert_eq!(edit.path(), &form);
    edit.publish(&base).unwrap();
    let after = Inventory::read(&source).unwrap();
    assert_eq!(
        before
            .files
            .keys()
            .filter(|path| before.files.get(*path) != after.files.get(*path))
            .collect::<Vec<_>>(),
        [&form]
    );
    assert!(before.files.keys().eq(after.files.keys()));
    let prepared =
        prepare_protected(&source, &base, &directory.0.join("generations"), true).unwrap();
    let native_path = path("Catalogs/Sample/Forms/Main.xml");
    let xml = fs::read(prepared.xml().join(native_path.as_path())).unwrap();
    let native_text = std::str::from_utf8(&xml).unwrap();
    let offset = native_text.find("Changed form").unwrap();
    let working = fs::read_to_string(source.join(form.as_path())).unwrap();
    let location = prepared
        .source_map()
        .file(&native_path)
        .unwrap()
        .locate(
            &xml,
            offset..offset + "Changed form".len(),
            Some(OriginTexts {
                accepted: &accepted,
                working: Some(&working),
            }),
        )
        .unwrap();
    assert_eq!(location.input.path, form);
    assert!(working[location.range.unwrap()].contains("Changed form"));
    let catalog = NativeUuid::parse("20000000-0000-4000-8000-000000000001").unwrap();
    let frozen_ref = accept_source(&source, &directory.0.join("frozen"), &[catalog]).unwrap();
    let frozen = AcceptedBase::open(&directory.0.join("frozen"), &frozen_ref).unwrap();
    assert_eq!(
        prepare_property_edit(
            &source,
            &frozen,
            &form_uuid,
            PropertyName::Comment,
            PropertyValue::Empty
        )
        .unwrap_err()
        .code,
        "source_edit_frozen"
    );
}
