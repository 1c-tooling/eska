use super::*;
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
