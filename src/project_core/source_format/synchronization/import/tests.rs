//! Source comparison preserves local edits while the existing approved baseline stays immutable.

use super::super::{capture_import_ancestor, tests::source};
use super::*;
use crate::{
    project_core::source_format::{
        LayoutPublication, NativeUuid, SourceManifest, accept_source, prepare_protected,
    },
    test_support::TestDir,
};
use std::fs;

const MODULE: &str = "CommonModules/Tools/Module.bsl";

/// Each plan owns separate working, incoming and ancestry trees and receives an independent base.
struct Fixture {
    test: TestDir,
    source: PathBuf,
    incoming_source: PathBuf,
    ancestor: ImportAncestor,
    approved: AcceptedBase,
}

impl Fixture {
    /// Freeze is attached only to the approved store; neither ancestor gets permission state.
    fn new(frozen: bool) -> Self {
        let test = TestDir::new();
        let source = source(&test);
        let freeze = if frozen {
            vec![NativeUuid::parse("20000000-0000-4000-8000-000000000001").unwrap()]
        } else {
            Vec::new()
        };
        let root = test.0.join("approved");
        let pin = accept_source(&source, &root, &freeze).unwrap();
        let approved = AcceptedBase::open(&root, &pin).unwrap();
        let root = test.0.join("ancestor");
        let reference = capture_import_ancestor(&source, &root).unwrap();
        let ancestor = ImportAncestor::open(&root, &reference).unwrap();
        let incoming_source = test.0.join("incoming-source");
        ancestor.restore(&incoming_source).unwrap();
        Self {
            test,
            source,
            incoming_source,
            ancestor,
            approved,
        }
    }

    /// Incoming data is captured as source ancestry, with no CI or support approval conversion.
    fn incoming(&self) -> ImportAncestor {
        let root = self.test.0.join("incoming");
        let reference = capture_import_ancestor(&self.incoming_source, &root).unwrap();
        ImportAncestor::open(&root, &reference).unwrap()
    }
}

/// A local module and unrelated incoming version edit are retained under the old approved pin.
#[test]
fn preserves_independent_edits_without_reaccepting_the_baseline() {
    let fixture = Fixture::new(false);
    let path = fixture.incoming_source.join("configuration.yaml");
    let text = fs::read_to_string(&path).unwrap();
    assert!(text.contains("1.0.0"));
    fs::write(path, text.replace("1.0.0", "1.0.1")).unwrap();
    fs::write(fixture.source.join(MODULE), b"\xef\xbb\xbf// local\r\n").unwrap();
    let incoming = fixture.incoming();
    let before = Inventory::read(&fixture.source).unwrap();
    let authority = Inventory::read(fixture.approved.storage_root()).unwrap();
    let plan = prepare_source_import(
        &fixture.source,
        &fixture.ancestor,
        &incoming,
        &fixture.approved,
    )
    .unwrap();
    assert!(plan.conflicts().is_empty(), "{:?}", plan.conflicts());
    assert_eq!(plan.approved_reference(), fixture.approved.reference());
    let output = fixture.test.0.join("output");
    assert_eq!(
        plan.publish_source(&output).unwrap(),
        LayoutPublication::Created
    );
    assert_eq!(
        plan.publish_source(&output).unwrap(),
        LayoutPublication::Unchanged
    );
    assert_eq!(
        fs::read(output.join(MODULE)).unwrap(),
        b"\xef\xbb\xbf// local\r\n"
    );
    assert!(
        fs::read_to_string(output.join("configuration.yaml"))
            .unwrap()
            .contains("1.0.1")
    );
    prepare_protected(
        &output,
        &fixture.approved,
        &fixture.test.0.join("generations"),
        true,
    )
    .unwrap();
    assert_eq!(Inventory::read(&fixture.source).unwrap(), before);
    assert_eq!(
        Inventory::read(fixture.approved.storage_root()).unwrap(),
        authority
    );
}

/// Overlapping bytes, deletion against edits and frozen edits are explicit publication barriers.
#[test]
fn conflicts_keep_every_original_input_and_create_no_output() {
    for mode in ["both", "deleted", "frozen", "binary"] {
        let fixture = Fixture::new(mode == "frozen");
        let path = if mode == "binary" {
            "Payload.bin"
        } else {
            MODULE
        };
        fs::write(fixture.incoming_source.join(path), "incoming").unwrap();
        if mode == "deleted" {
            fs::remove_file(fixture.source.join(path)).unwrap();
        } else if mode != "frozen" {
            fs::write(fixture.source.join(path), "local").unwrap();
        }
        let incoming = fixture.incoming();
        let plan = prepare_source_import(
            &fixture.source,
            &fixture.ancestor,
            &incoming,
            &fixture.approved,
        )
        .unwrap();
        let kind = match mode {
            "deleted" => SourceImportConflictKind::ModifiedDeleted,
            "frozen" => SourceImportConflictKind::PermissionDenied,
            _ => SourceImportConflictKind::BothModified,
        };
        assert!(
            plan.conflicts()
                .iter()
                .any(|conflict| conflict.kind == kind),
            "{:?}",
            plan.conflicts()
        );
        let before = Inventory::read(&fixture.source).unwrap();
        let output = fixture.test.0.join("output");
        assert_eq!(
            plan.publish_source(&output).unwrap_err().code,
            "source_import_conflicts"
        );
        assert!(!output.exists());
        assert_eq!(Inventory::read(&fixture.source).unwrap(), before);
        fixture.ancestor.verify().unwrap();
        incoming.verify().unwrap();
        fixture.approved.verify().unwrap();
    }
}

/// Changed UUIDs require a new verified mapping; stale source or incoming data cannot be published.
#[test]
fn mapping_changes_and_stale_inputs_never_bypass_approval() {
    let fixture = Fixture::new(false);
    let manifest_path = fixture.incoming_source.join(files::MANIFEST);
    let text = fs::read_to_string(&manifest_path).unwrap();
    fs::write(
        &manifest_path,
        text.replace(
            "20000000-0000-4000-8000-000000000001",
            "30000000-0000-4000-8000-000000000001",
        ),
    )
    .unwrap();
    let manifest: SourceManifest =
        serde_json::from_str(&fs::read_to_string(manifest_path).unwrap()).unwrap();
    let owner = manifest.files.iter().find(|file| matches!(&file.role, crate::project_core::source_format::FileRole::Descriptor { uuid, .. } if uuid.key() == "30000000-0000-4000-8000-000000000001")).unwrap().source_path();
    let path = fixture.incoming_source.join(owner.as_path());
    let text = fs::read_to_string(&path).unwrap();
    fs::write(
        path,
        text.replace(
            "20000000-0000-4000-8000-000000000001",
            "30000000-0000-4000-8000-000000000001",
        ),
    )
    .unwrap();
    let incoming = fixture.incoming();
    let plan = prepare_source_import(
        &fixture.source,
        &fixture.ancestor,
        &incoming,
        &fixture.approved,
    )
    .unwrap();
    assert!(
        plan.conflicts()
            .iter()
            .any(|conflict| conflict.kind == SourceImportConflictKind::MappingUnsupported)
    );
    assert!(plan.publish_source(&fixture.test.0.join("output")).is_err());
    assert!(!fixture.test.0.join("output").exists());

    let fixture = Fixture::new(false);
    let incoming = fixture.incoming();
    let plan = prepare_source_import(
        &fixture.source,
        &fixture.ancestor,
        &incoming,
        &fixture.approved,
    )
    .unwrap();
    fs::write(fixture.source.join(MODULE), "intervening change").unwrap();
    assert_eq!(
        plan.publish_source(&fixture.test.0.join("output"))
            .unwrap_err()
            .code,
        "source_input_changed"
    );
    assert!(!fixture.test.0.join("output").exists());
}

/// Extra local files and empty directories cannot disappear from a preview or successful import.
#[test]
fn records_unmapped_local_data_and_noop_comparison() {
    let fixture = Fixture::new(false);
    let incoming = fixture.incoming();
    let plan = prepare_source_import(
        &fixture.source,
        &fixture.ancestor,
        &incoming,
        &fixture.approved,
    )
    .unwrap();
    assert!(plan.changes().is_empty());
    assert!(plan.conflicts().is_empty());
    assert_eq!(plan.working_revision(), plan.output_revision());
    fs::write(fixture.source.join("notes.txt"), "keep").unwrap();
    fs::create_dir(fixture.source.join("Notes")).unwrap();
    let plan = prepare_source_import(
        &fixture.source,
        &fixture.ancestor,
        &incoming,
        &fixture.approved,
    )
    .unwrap();
    for kind in [
        SourceImportConflictKind::UnmappedFile,
        SourceImportConflictKind::UnmappedDirectory,
    ] {
        assert!(
            plan.conflicts()
                .iter()
                .any(|conflict| conflict.kind == kind)
        );
    }
    assert!(plan.publish_source(&fixture.test.0.join("output")).is_err());
    assert_eq!(
        fs::read_to_string(fixture.source.join("notes.txt")).unwrap(),
        "keep"
    );
}

/// A CF and CFE with the same UUID remain different inputs; another UUID never targets this project.
#[test]
fn rejects_wrong_native_type_and_root_identity() {
    for mode in ["extension", "uuid"] {
        let fixture = Fixture::new(false);
        let native = fixture.test.0.join("native");
        let path = native.join("Configuration.xml");
        let text = fs::read_to_string(&path).unwrap();
        let text = if mode == "extension" {
            text.replace(
                "<Version>",
                "<ConfigurationExtensionPurpose>Patch</ConfigurationExtensionPurpose><Version>",
            )
        } else {
            text.replace(
                "10000000-0000-4000-8000-000000000001",
                "90000000-0000-4000-8000-000000000001",
            )
        };
        fs::write(path, text).unwrap();
        let source = fixture.test.0.join("wrong-source");
        crate::project_core::source_format::import_designer_readable(&native, &source).unwrap();
        let store = fixture.test.0.join("wrong-ancestor");
        let pin = capture_import_ancestor(&source, &store).unwrap();
        let incoming = ImportAncestor::open(&store, &pin).unwrap();
        let error = prepare_source_import(
            &fixture.source,
            &fixture.ancestor,
            &incoming,
            &fixture.approved,
        )
        .err()
        .unwrap();
        assert_eq!(
            error.code,
            if mode == "extension" {
                "source_import_profile_mismatch"
            } else {
                "source_identity_mismatch"
            }
        );
        fixture.approved.verify().unwrap();
    }
}

/// Incoming policy bytes cannot become the current permission authority through ordinary import.
#[test]
fn changed_support_bytes_require_the_separate_base_update() {
    let test = TestDir::new();
    source(&test);
    let native = test.0.join("native");
    fs::create_dir(native.join("Ext")).unwrap();
    fs::write(native.join("Ext/ParentConfigurations.bin"), "{6,0,0,0,0,0}").unwrap();
    let source = test.0.join("policy-source");
    crate::project_core::source_format::import_designer_readable(&native, &source).unwrap();
    let base = test.0.join("approved");
    let pin = accept_source(&source, &base, &[]).unwrap();
    let approved = AcceptedBase::open(&base, &pin).unwrap();
    let store = test.0.join("ancestor");
    let pin = capture_import_ancestor(&source, &store).unwrap();
    let ancestor = ImportAncestor::open(&store, &pin).unwrap();
    let incoming_source = test.0.join("incoming-source");
    ancestor.restore(&incoming_source).unwrap();
    let policy = approved
        .package
        .manifest
        .files
        .iter()
        .find(|file| file.path.as_str() == "Ext/ParentConfigurations.bin")
        .unwrap()
        .source_path();
    fs::write(incoming_source.join(policy.as_path()), "{6,1,0,0,0,0}").unwrap();
    let store = test.0.join("incoming");
    let pin = capture_import_ancestor(&incoming_source, &store).unwrap();
    let incoming = ImportAncestor::open(&store, &pin).unwrap();
    let plan = prepare_source_import(&source, &ancestor, &incoming, &approved).unwrap();
    assert!(
        plan.conflicts()
            .iter()
            .any(|conflict| conflict.kind == SourceImportConflictKind::SupportUpdateRequired)
    );
    assert!(plan.publish_source(&test.0.join("output")).is_err());
    assert!(!test.0.join("output").exists());
    approved.verify().unwrap();
}
