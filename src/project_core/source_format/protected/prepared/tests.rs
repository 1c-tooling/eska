use super::*;
use crate::project_core::source_format::{
    FileOrigin, InputAuthority, NativeUuid, ObjectSource, Property, PropertyName, PropertyValue,
    Section, SourceMap, SourcePath,
};
use crate::{
    project_core::source_format::{
        AcceptedBase, accept_source, import_designer, prepare_protected,
    },
    test_support::TestDir,
};

/// Prepare one owned tree with metadata, module bytes, an orphan resource and an empty directory.
fn prepare(root: &Path) -> (std::path::PathBuf, PreparedSource) {
    let native = root.join("native");
    fs::create_dir(&native).expect("native root");
    fs::create_dir(native.join("Ext")).expect("module directory");
    fs::create_dir(native.join("Empty")).expect("empty directory");
    fs::write(native.join("Configuration.xml"), format!(
        "<MetaDataObject xmlns=\"{}\" version=\"2.20\"><Configuration uuid=\"10000000-0000-4000-8000-000000000001\"><Properties><Name>Prepared</Name><Comment>Accepted</Comment><Version>1.2.3.4</Version></Properties></Configuration></MetaDataObject>",
        crate::project_core::source_format::MD_NAMESPACE,
    )).expect("descriptor");
    fs::write(
        native.join("Ext/ManagedApplicationModule.bsl"),
        b"\xef\xbb\xbfProcedure Sample()\r\nEndProcedure\r\n",
    )
    .expect("module");
    fs::write(native.join("orphan.bin"), [0, 255, 13, 10]).expect("orphan resource");
    let source = root.join("source");
    import_designer(&native, &source).expect("import");
    let base_path = root.join("base");
    let reference = accept_source(&source, &base_path, &[]).expect("accept");
    let base = AcceptedBase::open(&base_path, &reference).expect("verified base");
    let prepared = prepare_protected(&source, &base, &root.join("generations"), true)
        .expect("protected input");
    (source, prepared)
}

/// Copy a tiny owned fixture without excluding resources or empty directories.
fn copy_tree(source: &Path, target: &Path) {
    fs::create_dir(target).expect("snapshot directory");
    for entry in fs::read_dir(source).expect("source entries") {
        let entry = entry.expect("entry");
        let target = target.join(entry.file_name());
        if entry.file_type().expect("entry kind").is_dir() {
            copy_tree(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), target).expect("snapshot payload");
        }
    }
}

#[test]
/// Presentation data cannot redirect a prepared input to arbitrary or stale XML.
fn modifying_export_report_does_not_change_the_private_proof() {
    let fixture = TestDir::new();
    let (_, prepared) = prepare(&fixture.0);
    let original = prepared.xml().to_owned();
    let mut report = prepared.report();
    report.xml = fixture.0.join("stale-xml");
    report.generation = "forged generation".to_owned();
    report.baseline = "forged policy".to_owned();
    assert_eq!(prepared.xml(), original);
    assert_ne!(prepared.generation(), report.generation);
    assert_ne!(prepared.baseline(), report.baseline);
    assert_eq!(prepared.files().count(), 3);
    prepared.validate().expect("private proof remains valid");
}

#[test]
/// Reuse protected payload selection and distinguish accepted structure from working metadata values.
fn source_map_records_selected_working_and_restored_accepted_inputs() {
    let fixture = TestDir::new();
    let (source, original) = prepare(&fixture.0);
    let module = SourcePath::parse("Ext/ManagedApplicationModule.bsl").expect("module path");
    let native = SourcePath::parse("Configuration.xml").expect("native path");
    let candidate = original.source_map().file(&module).expect("module origin");
    assert!(matches!(
        &candidate.origin,
        FileOrigin::Payload {
            authority: InputAuthority::Working,
            module: true,
            ..
        }
    ));
    assert!(matches!(
        &original
            .source_map()
            .file(&native)
            .expect("metadata origin")
            .origin,
        FileOrigin::Metadata {
            uses_working_values: false,
            working: Some(_),
            ..
        }
    ));
    let original_map = original.source_map().checksum().expect("original map");
    let path = source.join("configuration.yaml");
    let mut document =
        ObjectSource::from_yaml(&fs::read_to_string(&path).expect("owner")).expect("owner model");
    for section in &mut document.document.descriptor.object.sections {
        if let Section::Properties(properties) = section {
            for property in properties {
                if let Property::Typed {
                    name: PropertyName::Comment,
                    value,
                } = property
                {
                    *value = PropertyValue::Text("Working comment".to_owned());
                }
            }
        }
    }
    fs::write(path, document.to_yaml().expect("changed owner")).expect("working owner");
    fs::write(source.join(module.as_path()), b"working module").expect("working module");
    let base_path = fixture.0.join("base");
    let base = AcceptedBase::open(
        &base_path,
        &crate::project_core::source_format::AcceptedRef {
            sha256: original.baseline().to_owned(),
        },
    )
    .expect("base");
    let changed = prepare_protected(&source, &base, &fixture.0.join("generations"), true)
        .expect("changed input");
    assert!(matches!(
        &changed
            .source_map()
            .file(&native)
            .expect("changed metadata")
            .origin,
        FileOrigin::Metadata {
            uses_working_values: true,
            working: Some(_),
            ..
        }
    ));
    assert_ne!(
        changed.source_map().checksum().expect("changed map"),
        original_map
    );
    fs::remove_file(source.join(module.as_path())).expect("missing working module");
    let restored = prepare_protected(&source, &base, &fixture.0.join("generations"), false)
        .expect("restored module");
    assert!(matches!(
        &restored
            .source_map()
            .file(&module)
            .expect("restored origin")
            .origin,
        FileOrigin::Payload {
            authority: InputAuthority::Accepted,
            module: true,
            ..
        }
    ));
}

#[test]
/// A frozen payload maps to accepted bytes even when an edited candidate exists on disk.
fn source_map_records_accepted_authority_for_a_frozen_module() {
    let fixture = TestDir::new();
    let (source, _) = prepare(&fixture.0);
    let module = SourcePath::parse("Ext/ManagedApplicationModule.bsl").expect("module path");
    let frozen_path = fixture.0.join("frozen-base");
    fs::write(source.join(module.as_path()), b"working module")
        .expect("complete accepted composition");
    let uuid = NativeUuid::parse("10000000-0000-4000-8000-000000000001").expect("identity");
    let reference =
        accept_source(&source, &frozen_path, &[uuid]).expect("freeze accepted current input");
    let frozen = AcceptedBase::open(&frozen_path, &reference).expect("frozen base");
    fs::write(source.join(module.as_path()), b"denied module edit").expect("changed candidate");
    let protected = prepare_protected(
        &source,
        &frozen,
        &fixture.0.join("frozen-generations"),
        false,
    )
    .expect("excluded candidate");
    assert!(matches!(
        &protected
            .source_map()
            .file(&module)
            .expect("frozen module origin")
            .origin,
        FileOrigin::Payload {
            authority: InputAuthority::Accepted,
            module: true,
            ..
        }
    ));
}

#[test]
/// Validate serialized origins and content fingerprints without trusting them as an export proof.
fn source_map_rejects_forged_hashes_missing_entries_and_unknown_schema() {
    let fixture = TestDir::new();
    let (_, prepared) = prepare(&fixture.0);
    let original = prepared.source_map();
    let bytes = serde_json::to_vec(original).expect("map JSON");
    let parsed: SourceMap = serde_json::from_slice(&bytes).expect("map DTO");
    assert_eq!(&parsed, original);
    assert_eq!(
        parsed.checksum().expect("parsed map"),
        original.checksum().expect("map")
    );
    let mut invalid = parsed.clone();
    invalid.files.pop();
    assert_eq!(
        invalid.validate().expect_err("missing payload").code,
        "source_map_inventory_invalid"
    );
    let mut invalid = parsed.clone();
    invalid.schema_version = u32::MAX;
    assert_eq!(
        invalid.validate().expect_err("unknown schema").code,
        "source_map_schema_unsupported"
    );
    let mut invalid = parsed.clone();
    invalid.baseline = "forged".to_owned();
    assert_eq!(
        invalid.validate().expect_err("bad fingerprint").code,
        "source_map_fingerprint_invalid"
    );
    let mut invalid = parsed;
    let payload = invalid
        .files
        .iter_mut()
        .find(|file| matches!(file.origin, FileOrigin::Payload { .. }))
        .expect("payload");
    if let FileOrigin::Payload { input, .. } = &mut payload.origin {
        input.sha256 = "0".repeat(64);
    }
    assert_eq!(
        invalid.validate().expect_err("forged payload origin").code,
        "source_map_origin_invalid"
    );
}

#[test]
/// Rehashed inventory still cannot omit a parent directory or duplicate a native identity.
fn source_map_rejects_impossible_directory_and_identity_composition() {
    use crate::project_core::source_format::{FileEntry, FileRole, files};

    /// Recompute the native checksum so this test exercises topology rather than stale hashes.
    fn refresh_revision(map: &mut SourceMap) {
        let files = map
            .files
            .iter()
            .map(|entry| {
                (
                    entry.path.clone(),
                    FileEntry {
                        path: entry.path.clone(),
                        bytes: entry.bytes,
                        sha256: entry.sha256.clone(),
                        role: FileRole::Preserved,
                    },
                )
            })
            .collect();
        map.native_revision = files::revision(&files, &map.directories);
    }

    let fixture = TestDir::new();
    let (_, prepared) = prepare(&fixture.0);
    let mut invalid = prepared.source_map().clone();
    invalid.directories.retain(|path| path.as_str() != "Ext");
    refresh_revision(&mut invalid);
    assert_eq!(
        invalid.validate().expect_err("missing parent").code,
        "source_map_inventory_invalid"
    );
    let mut invalid = prepared.source_map().clone();
    let mut duplicate = invalid.files[0].clone();
    duplicate.path = SourcePath::parse("Other.xml").expect("duplicate descriptor path");
    invalid.files.push(duplicate);
    invalid
        .files
        .sort_by(|left, right| left.path.cmp(&right.path));
    refresh_revision(&mut invalid);
    assert_eq!(
        invalid.validate().expect_err("duplicate identity").code,
        "source_map_origin_invalid"
    );
}

#[test]
/// A generated tree modified after publication no longer qualifies as prepared input.
fn rejects_changed_generation_bytes_and_marker() {
    let fixture = TestDir::new();
    let (_, prepared) = prepare(&fixture.0);
    let path = prepared.xml().join("Configuration.xml");
    let original = fs::read(&path).expect("descriptor bytes");
    fs::write(&path, b"changed native bytes").expect("tampered descriptor");
    assert_eq!(
        prepared.validate().expect_err("changed generation").code,
        "source_generation_corrupt"
    );
    fs::write(&path, original).expect("restore owned bytes");
    prepared.validate().expect("exact bytes restored");
    let marker = prepared
        .xml()
        .parent()
        .expect("generation root")
        .join("generation.json");
    let mut value: serde_json::Value =
        serde_json::from_slice(&fs::read(&marker).expect("marker")).expect("marker JSON");
    value["baseline"] = serde_json::json!("forged policy");
    fs::write(&marker, serde_json::to_vec(&value).expect("JSON")).expect("changed marker");
    assert_eq!(
        prepared.validate().expect_err("changed binding").code,
        "source_generation_corrupt"
    );
}

#[test]
/// Verify the complete copied tree rather than only its main descriptor or active object list.
fn rejects_changed_missing_extra_payloads_and_lost_empty_directories() {
    let fixture = TestDir::new();
    let (_, prepared) = prepare(&fixture.0);
    for change in ["changed", "missing", "extra", "empty"] {
        let root = fixture.0.join(change);
        copy_tree(prepared.xml(), &root);
        prepared.validate_snapshot(&root).expect("exact snapshot");
        match change {
            "changed" => {
                fs::write(root.join("orphan.bin"), [1, 255, 13, 10]).expect("changed resource");
            }
            "missing" => fs::remove_file(root.join("orphan.bin")).expect("missing resource"),
            "extra" => fs::write(root.join("extra.bin"), b"extra").expect("additional resource"),
            "empty" => fs::remove_dir(root.join("Empty")).expect("missing empty directory"),
            _ => unreachable!("fixed cases"),
        }
        assert_eq!(
            prepared
                .validate_snapshot(&root)
                .expect_err("different snapshot")
                .code,
            "source_snapshot_changed",
        );
    }
}

#[test]
/// Later working edits do not rewrite the revision or bytes of an already prepared build input.
fn retains_original_snapshot_identity_after_working_files_change() {
    let fixture = TestDir::new();
    let (source, prepared) = prepare(&fixture.0);
    let revision = prepared.input_revision().to_owned();
    let original =
        fs::read(prepared.xml().join("Ext/ManagedApplicationModule.bsl")).expect("module");
    fs::write(
        source.join("Ext/ManagedApplicationModule.bsl"),
        b"later edit",
    )
    .expect("working change");
    prepared.validate().expect("original prepared input");
    assert_eq!(prepared.input_revision(), revision);
    assert_eq!(
        fs::read(prepared.xml().join("Ext/ManagedApplicationModule.bsl")).expect("original bytes"),
        original,
    );
}

#[cfg(unix)]
#[test]
/// Aliases remain invalid even when they lead to byte-identical XML or snapshot trees.
fn rejects_linked_generation_roots_ancestors_and_payloads() {
    use std::os::unix::fs::symlink;

    let fixture = TestDir::new();
    let (_, prepared) = prepare(&fixture.0);
    let alias = fixture.0.join("alias");
    symlink(prepared.xml(), &alias).expect("root alias");
    assert_eq!(
        prepared
            .validate_snapshot(&alias)
            .expect_err("linked root")
            .code,
        "source_entry_unsupported"
    );
    let parent_alias = fixture.0.join("parent-alias");
    symlink(prepared.xml().parent().expect("parent"), &parent_alias).expect("parent alias");
    assert_eq!(
        prepared
            .validate_snapshot(&parent_alias.join("xml"))
            .expect_err("linked ancestor")
            .code,
        "source_entry_unsupported"
    );
    let module = prepared.xml().join("Ext/ManagedApplicationModule.bsl");
    let bytes = fs::read(&module).expect("module bytes");
    let outside = fixture.0.join("identical.bsl");
    fs::write(&outside, bytes).expect("identical external payload");
    fs::remove_file(&module).expect("remove owned module");
    symlink(outside, &module).expect("payload alias");
    assert_eq!(
        prepared.validate().expect_err("linked payload").code,
        "source_entry_unsupported"
    );
}
