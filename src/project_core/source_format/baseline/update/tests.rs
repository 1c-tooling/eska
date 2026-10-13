use super::*;
use crate::{
    project_core::source_format::{
        LayoutPublication, accept_source, import_designer_readable, prepare_protected,
    },
    test_support::TestDir,
};
use std::fs;

/// The synthetic native corpus remains inside one owned playground directory and has no supplier.
fn native(directory: &TestDir, name: &str) -> PathBuf {
    let root = directory.0.join(name);
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/eska-format/designer");
    let mut pending = vec![PathBuf::new()];
    while let Some(relative) = pending.pop() {
        fs::create_dir_all(root.join(&relative)).unwrap();
        for entry in fs::read_dir(fixture.join(&relative)).unwrap() {
            let entry = entry.unwrap();
            let path = relative.join(entry.file_name());
            if entry.file_type().unwrap().is_dir() {
                pending.push(path);
            } else {
                fs::copy(entry.path(), root.join(path)).unwrap();
            }
        }
    }
    fs::remove_file(root.join("Ext/ParentConfigurations.bin")).unwrap();
    root
}

/// Both imported bases retain independent content-addressed stores and caller-supplied pins.
fn accepted(
    directory: &TestDir,
    name: &str,
    native: &Path,
    frozen: &[NativeUuid],
) -> (PathBuf, AcceptedBase) {
    let source = directory.0.join(format!("{name}-source"));
    import_designer_readable(native, &source).unwrap();
    let root = directory.0.join(format!("{name}-accepted"));
    let pin = accept_source(&source, &root, frozen).unwrap();
    (source, AcceptedBase::open(&root, &pin).unwrap())
}

/// A local module and an unrelated incoming descriptor update keep all original text and authority.
#[test]
fn preserves_local_bytes_and_publishes_the_independently_supplied_base() {
    let directory = TestDir::new();
    let original = native(&directory, "original");
    let supplied = native(&directory, "supplied");
    let path = supplied.join("Configuration.xml");
    let text = fs::read_to_string(&path)
        .unwrap()
        .replace("Configuration &amp; resources", "New supplier comment");
    fs::write(path, text).unwrap();
    let (source, previous) = accepted(&directory, "previous", &original, &[]);
    let (_, incoming) = accepted(&directory, "incoming", &supplied, &[]);
    let module = SourcePath::parse("CommonModules/Tools/Module.bsl").unwrap();
    let mut local = fs::read(source.join(module.as_path())).unwrap();
    local.extend_from_slice(b"\r\n// Local change\r\n");
    fs::write(source.join(module.as_path()), &local).unwrap();
    let before = Inventory::read(&source).unwrap();
    let old = Inventory::read(&previous.root).unwrap();
    let plan = prepare_base_update(&source, &previous, &incoming).unwrap();
    assert!(plan.conflicts().is_empty(), "{:?}", plan.conflicts());
    assert_eq!(plan.incoming_reference(), incoming.reference());
    assert_eq!(plan.previous_reference(), previous.reference());
    let destination = directory.0.join("updated-source");
    assert_eq!(
        plan.publish_source(&destination).unwrap(),
        LayoutPublication::Created
    );
    assert_eq!(
        plan.publish_source(&destination).unwrap(),
        LayoutPublication::Unchanged
    );
    assert_eq!(fs::read(destination.join(module.as_path())).unwrap(), local);
    let new_store = directory.0.join("updated-accepted");
    plan.publish_accepted(&new_store).unwrap();
    let updated = AcceptedBase::open(&new_store, plan.incoming_reference()).unwrap();
    assert_eq!(
        fs::read(destination.join("configuration.yaml")).unwrap(),
        incoming
            .read_file(&SourcePath::parse("configuration.yaml").unwrap())
            .unwrap()
            .as_bytes()
    );
    prepare_protected(
        &destination,
        &updated,
        &directory.0.join("generations"),
        true,
    )
    .unwrap();
    assert_eq!(Inventory::read(&source).unwrap(), before);
    assert_eq!(Inventory::read(&previous.root).unwrap(), old);
}

/// Concurrent module edits, new freezes and unmapped local files block every publication.
#[test]
fn conflicts_never_overwrite_working_source_or_either_authority() {
    for mode in ["both_modified", "freeze", "unmapped", "unmapped_directory"] {
        let directory = TestDir::new();
        let original = native(&directory, "original");
        let supplied = native(&directory, "supplied");
        if mode == "both_modified" {
            fs::write(
                supplied.join("CommonModules/Tools/Ext/Module.bsl"),
                "// Incoming module\n",
            )
            .unwrap();
        }
        let (source, previous) = accepted(&directory, "previous", &original, &[]);
        let frozen = if mode == "freeze" {
            vec![NativeUuid::parse("60000000-0000-4000-8000-000000000001").unwrap()]
        } else {
            vec![]
        };
        let (_, incoming) = accepted(&directory, "incoming", &supplied, &frozen);
        fs::write(
            source.join("CommonModules/Tools/Module.bsl"),
            "// Local module\r\n",
        )
        .unwrap();
        if mode == "unmapped" {
            fs::write(source.join("notes.txt"), "Keep this local data").unwrap();
        }
        if mode == "unmapped_directory" {
            fs::create_dir(source.join("local-empty-directory")).unwrap();
        }
        let before = Inventory::read(&source).unwrap();
        let plan = prepare_base_update(&source, &previous, &incoming).unwrap();
        let expected = match mode {
            "both_modified" => BaseUpdateConflictKind::BothModified,
            "freeze" => BaseUpdateConflictKind::PermissionDenied,
            "unmapped" => BaseUpdateConflictKind::UnmappedFile,
            _ => BaseUpdateConflictKind::UnmappedDirectory,
        };
        assert!(
            plan.conflicts()
                .iter()
                .any(|conflict| conflict.kind == expected),
            "{:?}",
            plan.conflicts()
        );
        let destination = directory.0.join("refused-source");
        let store = directory.0.join("refused-store");
        assert_eq!(
            plan.publish_source(&destination).unwrap_err().code,
            "source_update_conflicts"
        );
        assert_eq!(
            plan.publish_accepted(&store).unwrap_err().code,
            "source_update_conflicts"
        );
        assert!(!destination.exists() && !store.exists());
        assert_eq!(Inventory::read(&source).unwrap(), before);
        previous.verify().unwrap();
        incoming.verify().unwrap();
    }
}

/// A source path change carries local BSL only when the same UUID and native slot were retained.
#[test]
fn module_follows_a_supplier_rename_by_uuid() {
    let directory = TestDir::new();
    let original = native(&directory, "original");
    let supplied = native(&directory, "supplied");
    let root = supplied.join("Configuration.xml");
    fs::write(
        &root,
        fs::read_to_string(&root).unwrap().replace(
            "<CommonModule>Tools</CommonModule>",
            "<CommonModule>Renamed</CommonModule>",
        ),
    )
    .unwrap();
    let old = supplied.join("CommonModules/Tools.xml");
    fs::write(
        supplied.join("CommonModules/Renamed.xml"),
        fs::read_to_string(&old)
            .unwrap()
            .replace("<Name>Tools</Name>", "<Name>Renamed</Name>"),
    )
    .unwrap();
    fs::remove_file(old).unwrap();
    fs::rename(
        supplied.join("CommonModules/Tools"),
        supplied.join("CommonModules/Renamed"),
    )
    .unwrap();
    let (source, previous) = accepted(&directory, "previous", &original, &[]);
    let (_, incoming) = accepted(&directory, "incoming", &supplied, &[]);
    let local = b"\xef\xbb\xbf// Local module kept after rename\r\n";
    fs::write(source.join("CommonModules/Tools/Module.bsl"), local).unwrap();
    let plan = prepare_base_update(&source, &previous, &incoming).unwrap();
    assert!(plan.conflicts().is_empty(), "{:?}", plan.conflicts());
    assert!(plan.changes().iter().any(|change| {
        change
            .previous
            .as_ref()
            .is_some_and(|entry| entry.path.as_str() == "CommonModules/Tools/Module.bsl")
            && change
                .selected
                .as_ref()
                .is_some_and(|entry| entry.path.as_str() == "CommonModules/Renamed/Module.bsl")
    }));
    let destination = directory.0.join("renamed-source");
    plan.publish_source(&destination).unwrap();
    assert_eq!(
        fs::read(destination.join("CommonModules/Renamed/Module.bsl")).unwrap(),
        local
    );
    assert!(!destination.join("CommonModules/Tools/Module.bsl").exists());
}

/// Every captured operand is checked again; unrelated output and dropped freeze are not accepted.
#[test]
fn rejects_stale_preview_changed_declarations_and_removed_freeze() {
    let directory = TestDir::new();
    let original = native(&directory, "original");
    let supplied = native(&directory, "supplied");
    let frozen = NativeUuid::parse("20000000-0000-4000-8000-000000000001").unwrap();
    let (source, previous) = accepted(&directory, "previous", &original, &[frozen]);
    let (_, incoming) = accepted(&directory, "incoming", &supplied, &[]);
    let plan = prepare_base_update(&source, &previous, &incoming).unwrap();
    assert!(
        plan.conflicts()
            .iter()
            .any(|conflict| conflict.kind == BaseUpdateConflictKind::FreezeRemoved)
    );
    let (_, incoming) = accepted(
        &directory,
        "preserved",
        &supplied,
        &previous.manifest.frozen,
    );
    let plan = prepare_base_update(&source, &previous, &incoming).unwrap();
    let foreign = directory.0.join("foreign");
    fs::create_dir(&foreign).unwrap();
    fs::write(foreign.join("notes.txt"), "User data").unwrap();
    assert_eq!(
        plan.publish_source(&foreign).unwrap_err().code,
        "source_destination_exists"
    );
    fs::write(
        source.join("CommonModules/Tools/Module.bsl"),
        "// Changed after preview",
    )
    .unwrap();
    let destination = directory.0.join("stale");
    assert_eq!(
        plan.publish_source(&destination).unwrap_err().code,
        "source_input_changed"
    );
    assert!(!destination.exists());
    let path = source.join(files::MANIFEST);
    let mut map: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    map["input_revision"] = serde_json::Value::String("0".repeat(64));
    fs::write(&path, serde_json::to_vec(&map).unwrap()).unwrap();
    assert_eq!(
        prepare_base_update(&source, &previous, &incoming)
            .err()
            .unwrap()
            .code,
        "source_update_declarations_changed"
    );
}

/// Supplier deletion and reuse of a physical name never transfer edits to another metadata UUID.
#[test]
fn deletion_and_path_reuse_preserve_identity_and_report_edited_removed_data() {
    for reused in [false, true] {
        let directory = TestDir::new();
        let original = native(&directory, "original");
        let supplied = native(&directory, "supplied");
        if reused {
            let path = supplied.join("CommonModules/Tools.xml");
            fs::write(
                &path,
                fs::read_to_string(&path).unwrap().replace(
                    "60000000-0000-4000-8000-000000000001",
                    "60000000-0000-4000-8000-000000000002",
                ),
            )
            .unwrap();
        } else {
            let path = supplied.join("Configuration.xml");
            fs::write(
                &path,
                fs::read_to_string(&path)
                    .unwrap()
                    .replace("<CommonModule>Tools</CommonModule>", ""),
            )
            .unwrap();
            fs::remove_file(supplied.join("CommonModules/Tools.xml")).unwrap();
            fs::remove_dir_all(supplied.join("CommonModules/Tools")).unwrap();
        }
        let (source, previous) = accepted(&directory, "previous", &original, &[]);
        let (_, incoming) = accepted(&directory, "incoming", &supplied, &[]);
        let pristine = prepare_base_update(&source, &previous, &incoming).unwrap();
        assert!(
            pristine.conflicts().is_empty(),
            "{:?}",
            pristine.conflicts()
        );
        let destination = directory.0.join("updated");
        pristine.publish_source(&destination).unwrap();
        assert_eq!(
            destination.join("CommonModules/Tools/Module.bsl").exists(),
            reused
        );
        if reused {
            assert_eq!(
                fs::read(destination.join("CommonModules/Tools/Module.bsl")).unwrap(),
                fs::read(supplied.join("CommonModules/Tools/Ext/Module.bsl")).unwrap()
            );
        }
        let local = b"// Edited removed metadata must remain available\r\n";
        fs::write(source.join("CommonModules/Tools/Module.bsl"), local).unwrap();
        let plan = prepare_base_update(&source, &previous, &incoming).unwrap();
        assert!(plan.conflicts().iter().any(|conflict| {
            conflict.kind == BaseUpdateConflictKind::DeletedModified
                && conflict
                    .uuid
                    .as_ref()
                    .is_some_and(|uuid| uuid.as_str() == "60000000-0000-4000-8000-000000000001")
        }));
        assert_eq!(
            fs::read(source.join("CommonModules/Tools/Module.bsl")).unwrap(),
            local
        );
    }
}

/// Native restrictions and CF payloads belong to the independently pinned supplier authority.
#[test]
fn new_native_rule_and_local_authority_edit_are_separate_conflicts() {
    let directory = TestDir::new();
    let original = native(&directory, "original");
    let supplied = native(&directory, "supplied");
    let uuid = "60000000-0000-4000-8000-000000000001";
    for (root, rule) in [(&original, 1), (&supplied, 0)] {
        fs::create_dir(root.join("Ext/ParentConfigurations")).unwrap();
        // This sentinel covers authority transport only; actual platform CF pairs are a separate gate.
        fs::write(
            root.join("Ext/ParentConfigurations/SampleConfiguration.cf"),
            b"synthetic opaque CF sentinel",
        )
        .unwrap();
        fs::write(
            root.join("Ext/ParentConfigurations.bin"),
            format!(
                "{{6,0,1,a0000000-0000-4000-8000-000000000001,0,10000000-0000-4000-8000-000000000001,\"1.0.0.0\",\"Fixture vendor\",\"SampleConfiguration\",1,{rule},0,{uuid},{uuid},0,0,0,1,0,0,0,1,0,1,0,1,1,1,1}}"
            ),
        )
        .unwrap();
    }
    let (source, previous) = accepted(&directory, "previous", &original, &[]);
    let (_, incoming) = accepted(&directory, "incoming", &supplied, &[]);
    fs::write(source.join("CommonModules/Tools/Module.bsl"), "// Local\n").unwrap();
    let plan = prepare_base_update(&source, &previous, &incoming).unwrap();
    assert!(plan.support_changes().iter().any(|change| {
        change.uuid.as_str() == uuid && change.previous.allowed && !change.incoming.allowed
    }));
    assert!(plan.conflicts().iter().any(|conflict| {
        conflict.kind == BaseUpdateConflictKind::PermissionDenied
            && conflict.exclusion.as_ref().is_some_and(|exclusion| {
                exclusion.reason
                    == crate::project_core::source_format::PermissionReason::NativeRestriction
            })
    }));
    let authority = previous
        .package
        .manifest
        .files
        .iter()
        .find(|file| file.path.as_str() == "Ext/ParentConfigurations.bin")
        .unwrap()
        .source_path();
    fs::write(source.join(authority.as_path()), "{6,0,0,0,0,0}").unwrap();
    let plan = prepare_base_update(&source, &previous, &incoming).unwrap();
    assert!(
        plan.conflicts()
            .iter()
            .any(|conflict| conflict.kind == BaseUpdateConflictKind::AuthorityEdited)
    );
}

/// Caller-supplied identity is checked independently; removing a local required file stays explicit.
#[test]
fn wrong_root_identity_and_local_deletion_cannot_be_silently_accepted() {
    let directory = TestDir::new();
    let original = native(&directory, "original");
    let supplied = native(&directory, "supplied");
    let (source, previous) = accepted(&directory, "previous", &original, &[]);
    let (_, incoming) = accepted(&directory, "incoming", &supplied, &[]);
    fs::remove_file(source.join("CommonModules/Tools/Module.bsl")).unwrap();
    let plan = prepare_base_update(&source, &previous, &incoming).unwrap();
    assert!(
        plan.conflicts()
            .iter()
            .any(|conflict| conflict.kind == BaseUpdateConflictKind::ModifiedDeleted)
    );
    let root = supplied.join("Configuration.xml");
    fs::write(
        &root,
        fs::read_to_string(&root).unwrap().replace(
            "10000000-0000-4000-8000-000000000001",
            "10000000-0000-4000-8000-000000000002",
        ),
    )
    .unwrap();
    let (_, foreign) = accepted(&directory, "foreign", &supplied, &[]);
    let error = prepare_base_update(&source, &previous, &foreign)
        .err()
        .unwrap();
    assert_eq!(error.code, "source_identity_mismatch");
}

/// A reused configuration UUID cannot turn a CF project into CFE by sharing the native root tag.
#[test]
fn extension_and_configuration_are_distinct_native_profiles() {
    let directory = TestDir::new();
    let original = native(&directory, "original");
    let supplied = native(&directory, "supplied");
    let root = supplied.join("Configuration.xml");
    fs::write(
        &root,
        fs::read_to_string(&root).unwrap().replacen(
            "<Properties>",
            "<Properties><ConfigurationExtensionPurpose>Customization</ConfigurationExtensionPurpose>",
            1,
        ),
    )
    .unwrap();
    let (source, previous) = accepted(&directory, "previous", &original, &[]);
    let (_, incoming) = accepted(&directory, "incoming", &supplied, &[]);
    assert_eq!(
        prepare_base_update(&source, &previous, &incoming)
            .err()
            .unwrap()
            .code,
        "source_update_profile_mismatch"
    );
}
