use super::*;
use crate::{
    project_core::source_format::{
        NativeUuid, Operation, PermissionReason, PropertyName, SourceReader, accept_source,
        export_designer, import_designer, import_designer_separated,
    },
    test_support::TestDir,
};
use std::{fs, path::PathBuf};

/// Copy only the public synthetic corpus; its supplier sentinel makes no native CF claim.
fn fixture(directory: &TestDir, separated: bool, frozen: &[NativeUuid]) -> (PathBuf, AcceptedBase) {
    let native = directory.0.join("native");
    let input = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/eska-format/designer");
    let mut pending = vec![PathBuf::new()];
    while let Some(path) = pending.pop() {
        fs::create_dir_all(native.join(&path)).unwrap();
        for entry in fs::read_dir(input.join(&path)).unwrap() {
            let entry = entry.unwrap();
            let path = path.join(entry.file_name());
            if entry.file_type().unwrap().is_dir() {
                pending.push(path);
            } else {
                fs::copy(entry.path(), native.join(path)).unwrap();
            }
        }
    }
    fs::create_dir_all(native.join("Ext/ParentConfigurations")).unwrap();
    fs::write(
        native.join("Ext/ParentConfigurations/SampleConfiguration.cf"),
        b"synthetic opaque CF sentinel",
    )
    .unwrap();
    let source = directory.0.join("source");
    if separated {
        import_designer_separated(&native, &source).unwrap();
    } else {
        import_designer(&native, &source).unwrap();
    }
    let accepted = directory.0.join("accepted");
    let reference = accept_source(&source, &accepted, frozen).unwrap();
    (source, AcceptedBase::open(&accepted, &reference).unwrap())
}

/// Relocation changes the pin but never native policy, inherited freeze or original XML evidence.
#[test]
fn accepted_layout_preserves_policy_freeze_and_exact_native_output() {
    let directory = TestDir::new();
    let frozen = NativeUuid::parse("20000000-0000-4000-8000-000000000001").unwrap();
    let (_, base) = fixture(&directory, false, &[frozen]);
    let before = Inventory::read(&base.root).unwrap();
    let plan = prepare_accepted_descriptor_layout(&base).unwrap();
    assert!(!plan.is_empty());
    assert!(!plan.source_changes().is_empty());
    assert_eq!(plan.source_manifest().schema_version, 2);
    assert_eq!(plan.previous_reference(), base.reference());
    assert_ne!(plan.accepted_reference(), base.reference());
    let destination = directory.0.join("relocated");
    assert_eq!(
        plan.publish(&destination).unwrap(),
        LayoutPublication::Created
    );
    assert_eq!(
        plan.publish(&destination).unwrap(),
        LayoutPublication::Unchanged
    );
    let relocated = AcceptedBase::open(&destination, plan.accepted_reference()).unwrap();
    assert_eq!(
        AcceptedBase::open(&destination, base.reference())
            .err()
            .unwrap()
            .code,
        "source_baseline_untrusted"
    );
    assert_eq!(
        relocated.manifest.input_revision,
        base.manifest.input_revision
    );
    assert_eq!(
        relocated.manifest.configuration_uuid,
        base.manifest.configuration_uuid
    );
    assert_eq!(relocated.manifest.frozen, base.manifest.frozen);
    assert_eq!(relocated.manifest.suppliers, base.manifest.suppliers);
    for key in base.objects.keys() {
        let uuid = NativeUuid::parse(key).unwrap();
        for operation in [
            Operation::Property(PropertyName::Comment),
            Operation::Module,
            Operation::Create,
        ] {
            let old = base.permission(&uuid, operation);
            let new = relocated.permission(&uuid, operation);
            assert_eq!(
                (old.allowed, old.state, old.native_reason, old.reason),
                (new.allowed, new.state, new.native_reason, new.reason)
            );
        }
    }
    let form = NativeUuid::parse("70000000-0000-4000-8000-000000000001").unwrap();
    assert_eq!(
        relocated
            .permission(&form, Operation::Property(PropertyName::Comment))
            .reason,
        PermissionReason::EskaFrozen
    );
    let old_source = directory.0.join("old-source");
    let new_source = directory.0.join("new-source");
    base.restore(&old_source).unwrap();
    relocated.restore(&new_source).unwrap();
    assert_eq!(
        SourceReader::open(&new_source)
            .unwrap()
            .owner_paths()
            .count(),
        4
    );
    let old_xml = directory.0.join("old-xml");
    let new_xml = directory.0.join("new-xml");
    export_designer(&old_source, &old_xml).unwrap();
    export_designer(&new_source, &new_xml).unwrap();
    assert_eq!(
        Inventory::read(&old_xml).unwrap(),
        Inventory::read(&new_xml).unwrap()
    );
    assert_eq!(Inventory::read(&base.root).unwrap(), before);
}

/// A corrupt working policy and local YAML edit cannot enter a plan derived exclusively from blobs.
#[test]
fn accepted_layout_never_reaccepts_current_working_values() {
    let directory = TestDir::new();
    let (source, base) = fixture(&directory, false, &[]);
    let catalog = source.join("Catalogs/Sample/object.yaml");
    let original = fs::read_to_string(&catalog).unwrap();
    fs::write(
        &catalog,
        original.replace("Preserved form", "Untrusted working change"),
    )
    .unwrap();
    fs::write(
        source.join("Ext/ParentConfigurations.bin"),
        b"untrusted rules",
    )
    .unwrap();
    let working = Inventory::read(&source).unwrap();
    let plan = prepare_accepted_descriptor_layout(&base).unwrap();
    let destination = directory.0.join("relocated");
    plan.publish(&destination).unwrap();
    let relocated = AcceptedBase::open(&destination, plan.accepted_reference()).unwrap();
    let restored = directory.0.join("restored");
    relocated.restore(&restored).unwrap();
    assert!(
        fs::read_to_string(restored.join("Catalogs/Sample/Forms/Main/form.yaml"))
            .unwrap()
            .contains("Preserved form")
    );
    assert_eq!(Inventory::read(&source).unwrap(), working);
    assert_eq!(relocated.manifest.suppliers, base.manifest.suppliers);
}

/// An already separated authority preserves exact pins, authored style and even unrelated storage bytes.
#[test]
fn separated_base_migration_is_an_exact_noop_or_copy() {
    let directory = TestDir::new();
    let (_, base) = fixture(&directory, true, &[]);
    fs::write(base.root.join("note.txt"), b"unrelated storage note").unwrap();
    let before = Inventory::read(&base.root).unwrap();
    let plan = prepare_accepted_descriptor_layout(&base).unwrap();
    assert!(plan.is_empty());
    assert!(plan.source_changes().is_empty());
    assert_eq!(plan.accepted_reference(), base.reference());
    assert_eq!(
        plan.publish(&base.root).unwrap(),
        LayoutPublication::Unchanged
    );
    let destination = directory.0.join("copy");
    assert_eq!(
        plan.publish(&destination).unwrap(),
        LayoutPublication::Created
    );
    assert_eq!(Inventory::read(&destination).unwrap(), before);
    AcceptedBase::open(&destination, base.reference()).unwrap();
}

/// The marker alone cannot make partial migration trusted; recovery preserves only matching bytes.
#[test]
fn accepted_layout_resumes_owned_output_and_refuses_edits_or_foreign_storage() {
    let directory = TestDir::new();
    let (_, base) = fixture(&directory, false, &[]);
    let plan = prepare_accepted_descriptor_layout(&base).unwrap();
    let destination = directory.0.join("resume");
    publication::interrupt(plan.publication_view(), &destination);
    fs::write(
        destination.join(BASE_MANIFEST),
        plan.text
            .get(&SourcePath::parse(BASE_MANIFEST).unwrap())
            .unwrap(),
    )
    .unwrap();
    fs::write(
        destination.join(crate::project_core::source_format::layout::PENDING_WRITE),
        b"interrupted staging bytes",
    )
    .unwrap();
    assert_eq!(
        AcceptedBase::open(&destination, plan.accepted_reference())
            .err()
            .unwrap()
            .code,
        "source_layout_incomplete"
    );
    assert_eq!(
        plan.publish(&destination).unwrap(),
        LayoutPublication::Resumed
    );
    AcceptedBase::open(&destination, plan.accepted_reference()).unwrap();
    let edited = directory.0.join("edited");
    publication::interrupt(plan.publication_view(), &edited);
    fs::write(edited.join(BASE_MANIFEST), b"edited marker").unwrap();
    let before = Inventory::read(&edited).unwrap();
    assert_eq!(
        plan.publish(&edited).unwrap_err().code,
        "source_layout_destination_mismatch"
    );
    assert_eq!(Inventory::read(&edited).unwrap(), before);
    let foreign = directory.0.join("foreign");
    fs::create_dir(&foreign).unwrap();
    fs::write(foreign.join("note.txt"), b"user bytes").unwrap();
    assert_eq!(
        plan.publish(&foreign).unwrap_err().code,
        "source_destination_exists"
    );
}

/// Changing the old authority after preview blocks publication before any destination is claimed.
#[test]
fn accepted_layout_rechecks_all_old_blobs_and_refuses_overlap() {
    let directory = TestDir::new();
    let (_, base) = fixture(&directory, false, &[]);
    let plan = prepare_accepted_descriptor_layout(&base).unwrap();
    assert_eq!(
        plan.publish(&base.root.join("nested")).unwrap_err().code,
        "source_tree_overlap"
    );
    let entry = base.package.snapshot.files.values().next().unwrap();
    fs::write(
        base.root.join(blob_path(&entry.sha256).unwrap().as_path()),
        b"changed authority",
    )
    .unwrap();
    let destination = directory.0.join("relocated");
    assert_eq!(
        plan.publish(&destination).unwrap_err().code,
        "source_input_changed"
    );
    assert!(!destination.exists());
    assert!(prepare_accepted_descriptor_layout(&base).is_err());
}

/// Reserved staging names cannot be preserved as unrelated storage notes in a no-op copy.
#[test]
fn accepted_layout_refuses_reserved_storage_names_before_publication() {
    let directory = TestDir::new();
    let (_, base) = fixture(&directory, true, &[]);
    let reserved = base
        .root
        .join(crate::project_core::source_format::layout::PENDING_WRITE);
    fs::write(&reserved, b"existing bytes").unwrap();
    let before = Inventory::read(&base.root).unwrap();
    assert_eq!(
        prepare_accepted_descriptor_layout(&base).unwrap_err().code,
        "source_path_collision"
    );
    assert_eq!(Inventory::read(&base.root).unwrap(), before);
    fs::remove_file(&reserved).unwrap();
    fs::create_dir(&reserved).unwrap();
    assert_eq!(
        prepare_accepted_descriptor_layout(&base).unwrap_err().code,
        "source_path_collision"
    );
}
