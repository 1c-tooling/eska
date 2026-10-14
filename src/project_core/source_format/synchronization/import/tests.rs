//! Source comparison preserves local edits while the existing approved baseline stays immutable.

mod additions;

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
        Self::with_link(frozen, None)
    }

    /// A preserved handler is a qualified reference, with independently approved owner freeze.
    fn with_link(frozen: bool, link: Option<bool>) -> Self {
        let test = TestDir::new();
        let source = source(&test);
        if link.is_some() {
            let native = test.0.join("native");
            fs::create_dir(native.join("EventSubscriptions")).unwrap();
            fs::write(
                native.join("EventSubscriptions/OnWrite.xml"),
                concat!(
                    "<MetaDataObject xmlns='http://v8.1c.ru/8.3/MDClasses' version='2.20'>",
                    "<EventSubscription uuid='30000000-0000-4000-8000-000000000001'><Properties>",
                    "<Name>OnWrite</Name><Handler>CommonModule.Tools.Perform</Handler>",
                    "</Properties></EventSubscription></MetaDataObject>"
                ),
            )
            .unwrap();
            let root = native.join("Configuration.xml");
            let original = fs::read_to_string(&root).unwrap();
            fs::write(
                root,
                original.replace(
                    "</ChildObjects>",
                    "<EventSubscription>OnWrite</EventSubscription></ChildObjects>",
                ),
            )
            .unwrap();
            fs::remove_dir_all(&source).unwrap();
            crate::project_core::source_format::import_designer_readable(&native, &source).unwrap();
        }
        let freeze = if frozen {
            vec![NativeUuid::parse("20000000-0000-4000-8000-000000000001").unwrap()]
        } else {
            Vec::new()
        };
        let mut freeze = freeze;
        if link == Some(true) {
            freeze.push(NativeUuid::parse("30000000-0000-4000-8000-000000000001").unwrap());
        }
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

/// Reimport a real reversible native path transition rather than hand-editing reverse-map hashes.
fn rename_incoming(fixture: &Fixture, name: &str) {
    let native = fixture.test.0.join("renamed-native");
    crate::project_core::source_format::export_designer(&fixture.incoming_source, &native).unwrap();
    for path in ["Configuration.xml", "CommonModules/Tools.xml"] {
        let path = native.join(path);
        let text = fs::read_to_string(&path).unwrap();
        fs::write(path, text.replace(">Tools<", &format!(">{name}<"))).unwrap();
    }
    fs::rename(
        native.join("CommonModules/Tools.xml"),
        native.join(format!("CommonModules/{name}.xml")),
    )
    .unwrap();
    fs::rename(
        native.join("CommonModules/Tools"),
        native.join(format!("CommonModules/{name}")),
    )
    .unwrap();
    fs::remove_dir_all(&fixture.incoming_source).unwrap();
    crate::project_core::source_format::import_designer_readable(&native, &fixture.incoming_source)
        .unwrap();
}

/// The incoming native graph removes one owner and all of its payloads without editing working data.
fn delete_incoming(fixture: &Fixture) {
    let native = fixture.test.0.join("deleted-native");
    crate::project_core::source_format::export_designer(&fixture.incoming_source, &native).unwrap();
    let root = native.join("Configuration.xml");
    let original = fs::read_to_string(&root).unwrap();
    fs::write(
        root,
        original.replace("<CommonModule>Tools</CommonModule>", ""),
    )
    .unwrap();
    fs::remove_file(native.join("CommonModules/Tools.xml")).unwrap();
    fs::remove_dir_all(native.join("CommonModules/Tools")).unwrap();
    fs::remove_dir_all(&fixture.incoming_source).unwrap();
    crate::project_core::source_format::import_designer_readable(&native, &fixture.incoming_source)
        .unwrap();
}

/// Native addition supplies complete bytes and fresh UUIDs rather than inventing metadata defaults.
fn add_incoming(fixture: &Fixture, prepend: bool) {
    let native = fixture.test.0.join("added-native");
    crate::project_core::source_format::export_designer(&fixture.incoming_source, &native).unwrap();
    let root = native.join("Configuration.xml");
    let text = fs::read_to_string(&root).unwrap();
    let old = "<CommonModule>Tools</CommonModule>";
    let added = "<CommonModule>Helpers</CommonModule>";
    let refs = if prepend {
        format!("{added}{old}")
    } else {
        format!("{old}{added}")
    };
    fs::write(root, text.replace(old, &refs)).unwrap();
    let text = fs::read_to_string(native.join("CommonModules/Tools.xml")).unwrap();
    fs::write(
        native.join("CommonModules/Helpers.xml"),
        text.replace(
            "20000000-0000-4000-8000-000000000001",
            "30000000-0000-4000-8000-000000000001",
        )
        .replace(">Tools<", ">Helpers<"),
    )
    .unwrap();
    fs::create_dir_all(native.join("CommonModules/Helpers/Ext")).unwrap();
    fs::write(
        native.join("CommonModules/Helpers/Ext/Module.bsl"),
        b"\xef\xbb\xbf// incoming\r\n",
    )
    .unwrap();
    fs::remove_dir_all(&fixture.incoming_source).unwrap();
    crate::project_core::source_format::import_designer_readable(&native, &fixture.incoming_source)
        .unwrap();
}

/// A new owner has exact working origins; independent local bytes and the original pin survive repeat.
#[test]
fn added_owner_keeps_original_approval_and_uses_working_evidence() {
    use crate::project_core::source_format::{FileOrigin, InputAuthority, OriginTexts};
    let fixture = Fixture::new(false);
    add_incoming(&fixture, false);
    let root = fixture.source.join("configuration.yaml");
    let text = fs::read_to_string(&root).unwrap();
    fs::write(root, text.replace("1.0.0", "1.0.1")).unwrap();
    fs::write(fixture.source.join(MODULE), b"\xef\xbb\xbf// local\r\n").unwrap();
    let authority = Inventory::read(fixture.approved.storage_root()).unwrap();
    let incoming = fixture.incoming();
    let plan = prepare_source_import(
        &fixture.source,
        &fixture.ancestor,
        &incoming,
        &fixture.approved,
    )
    .unwrap();
    assert!(plan.conflicts().is_empty(), "{:?}", plan.conflicts());
    let output = fixture.test.0.join("output");
    plan.publish_source(&output).unwrap();
    assert_eq!(
        fs::read(output.join(MODULE)).unwrap(),
        b"\xef\xbb\xbf// local\r\n"
    );
    assert_eq!(
        fs::read(output.join("CommonModules/Helpers/Module.bsl")).unwrap(),
        b"\xef\xbb\xbf// incoming\r\n"
    );
    assert!(
        fs::read_to_string(output.join("configuration.yaml"))
            .unwrap()
            .contains("1.0.1")
    );
    let prepared = prepare_protected(
        &output,
        &fixture.approved,
        &fixture.test.0.join("generations"),
        true,
    )
    .unwrap();
    let map = prepared.source_map();
    assert_eq!(map.schema_version, 7);
    let mapped = map
        .file(&SourcePath::parse("CommonModules/Helpers.xml").unwrap())
        .unwrap();
    let FileOrigin::CreatedMetadata { input, .. } = &mapped.origin else {
        panic!("working-only metadata origin");
    };
    let text = fs::read_to_string(output.join(input.path.as_path())).unwrap();
    let native = fs::read(prepared.xml().join(mapped.path.as_path())).unwrap();
    let start = String::from_utf8_lossy(&native).find("Helpers").unwrap();
    let location = mapped
        .locate(
            &native,
            start..start + 7,
            Some(OriginTexts {
                accepted: "",
                working: Some(&text),
            }),
        )
        .unwrap();
    assert_eq!(location.authority, InputAuthority::Working);
    assert_eq!(location.input, *input);
    assert_eq!(&text[location.range.unwrap()], "\"Helpers\"");
    let mut old_schema = map.clone();
    old_schema.schema_version = 6;
    assert_eq!(
        old_schema.validate().unwrap_err().code,
        "source_map_origin_invalid"
    );
    let repeat = prepare_source_import(&output, &incoming, &incoming, &fixture.approved).unwrap();
    assert!(repeat.conflicts().is_empty(), "{:?}", repeat.conflicts());
    assert_eq!(repeat.working_revision(), repeat.output_revision());
    assert_eq!(
        Inventory::read(fixture.approved.storage_root()).unwrap(),
        authority
    );
}

/// A frozen parent denies addition, and insertion cannot shift a frozen existing sibling.
#[test]
fn additions_obey_parent_and_surviving_sibling_freeze() {
    for mode in ["parent", "prepend", "append"] {
        let mut fixture = Fixture::new(mode != "parent");
        if mode == "parent" {
            let root = fixture.test.0.join("root-frozen");
            let pin = accept_source(
                &fixture.source,
                &root,
                &[NativeUuid::parse("10000000-0000-4000-8000-000000000001").unwrap()],
            )
            .unwrap();
            fixture.approved = AcceptedBase::open(&root, &pin).unwrap();
        }
        add_incoming(&fixture, mode == "prepend");
        let incoming = fixture.incoming();
        let plan = prepare_source_import(
            &fixture.source,
            &fixture.ancestor,
            &incoming,
            &fixture.approved,
        )
        .unwrap();
        if mode == "append" {
            assert!(plan.conflicts().is_empty(), "{:?}", plan.conflicts());
        } else {
            assert!(
                plan.conflicts()
                    .iter()
                    .any(|conflict| conflict.kind == SourceImportConflictKind::PermissionDenied),
                "{:?}",
                plan.conflicts()
            );
            assert_eq!(
                plan.publish_source(&fixture.test.0.join("output"))
                    .unwrap_err()
                    .code,
                "source_import_conflicts"
            );
        }
    }
}

/// Whole-owner deletion keeps independent local values and rejects modified, frozen or referenced owners.
#[test]
fn deleted_object_removes_its_files_only_under_original_permissions_and_clean_ancestry() {
    for mode in ["allowed", "local_module", "frozen", "referenced"] {
        let fixture = if mode == "referenced" {
            Fixture::with_link(false, Some(false))
        } else {
            Fixture::new(mode == "frozen")
        };
        delete_incoming(&fixture);
        let root = fixture.source.join("configuration.yaml");
        let original = fs::read_to_string(&root).unwrap();
        fs::write(root, original.replace("1.0.0", "1.0.1")).unwrap();
        if mode == "local_module" {
            fs::write(fixture.source.join(MODULE), "// local change\r\n").unwrap();
        }
        let incoming = fixture.incoming();
        let before = Inventory::read(fixture.approved.storage_root()).unwrap();
        let result = prepare_source_import(
            &fixture.source,
            &fixture.ancestor,
            &incoming,
            &fixture.approved,
        );
        if mode == "referenced" {
            assert_eq!(
                result.err().expect("surviving handler").code,
                "source_structural_reference_preserved"
            );
            continue;
        }
        let plan = result.unwrap();
        let output = fixture.test.0.join("output");
        if mode != "allowed" {
            let expected = if mode == "local_module" {
                SourceImportConflictKind::DeletedModified
            } else {
                SourceImportConflictKind::PermissionDenied
            };
            assert!(
                plan.conflicts()
                    .iter()
                    .any(|conflict| conflict.kind == expected),
                "{:?}",
                plan.conflicts()
            );
            assert!(plan.publish_source(&output).is_err());
            assert!(!output.exists());
            continue;
        }
        assert!(plan.conflicts().is_empty(), "{:?}", plan.conflicts());
        plan.publish_source(&output).unwrap();
        assert!(!output.join("CommonModules/Tools").exists());
        assert!(
            fs::read_to_string(output.join("configuration.yaml"))
                .unwrap()
                .contains("1.0.1")
        );
        let prepared = prepare_protected(
            &output,
            &fixture.approved,
            &fixture.test.0.join("generations"),
            true,
        )
        .unwrap();
        assert!(!prepared.xml().join("CommonModules/Tools.xml").exists());
        assert!(
            !prepared
                .xml()
                .join("CommonModules/Tools/Ext/Module.bsl")
                .exists()
        );
        let repeat =
            prepare_source_import(&output, &incoming, &incoming, &fixture.approved).unwrap();
        assert!(repeat.conflicts().is_empty());
        assert_eq!(repeat.output_revision(), repeat.working_revision());
        assert_eq!(
            Inventory::read(fixture.approved.storage_root()).unwrap(),
            before
        );
    }
}

/// An otherwise unrestricted owner cannot erase an independently frozen inline descendant.
#[test]
fn deleted_owner_keeps_the_freeze_of_its_inline_descendant() {
    let directory = TestDir::new();
    let native = directory.0.join("native");
    fs::create_dir_all(native.join("Catalogs")).unwrap();
    fs::write(native.join("Configuration.xml"), concat!(
        "<MetaDataObject xmlns='http://v8.1c.ru/8.3/MDClasses' version='2.20'>",
        "<Configuration uuid='10000000-0000-4000-8000-000000000001'><Properties><Name>Sample</Name></Properties>",
        "<ChildObjects><Catalog>Sample</Catalog></ChildObjects></Configuration></MetaDataObject>"
    )).unwrap();
    fs::write(native.join("Catalogs/Sample.xml"), concat!(
        "<MetaDataObject xmlns='http://v8.1c.ru/8.3/MDClasses' version='2.20'>",
        "<Catalog uuid='20000000-0000-4000-8000-000000000001'><Properties><Name>Sample</Name></Properties>",
        "<ChildObjects><Attribute uuid='30000000-0000-4000-8000-000000000001'>",
        "<Properties><Name>FrozenField</Name></Properties></Attribute></ChildObjects></Catalog></MetaDataObject>"
    )).unwrap();
    let source = directory.0.join("source");
    crate::project_core::source_format::import_designer_readable(&native, &source).unwrap();
    let frozen = NativeUuid::parse("30000000-0000-4000-8000-000000000001").unwrap();
    let base_path = directory.0.join("accepted");
    let pin = accept_source(&source, &base_path, std::slice::from_ref(&frozen)).unwrap();
    let base = AcceptedBase::open(&base_path, &pin).unwrap();
    let old_path = directory.0.join("ancestor");
    let old_pin = capture_import_ancestor(&source, &old_path).unwrap();
    let old = ImportAncestor::open(&old_path, &old_pin).unwrap();
    let root = native.join("Configuration.xml");
    let original = fs::read_to_string(&root).unwrap();
    fs::write(root, original.replace("<Catalog>Sample</Catalog>", "")).unwrap();
    fs::remove_file(native.join("Catalogs/Sample.xml")).unwrap();
    let incoming_source = directory.0.join("incoming-source");
    crate::project_core::source_format::import_designer_readable(&native, &incoming_source)
        .unwrap();
    let incoming_path = directory.0.join("incoming");
    let incoming_pin = capture_import_ancestor(&incoming_source, &incoming_path).unwrap();
    let incoming = ImportAncestor::open(&incoming_path, &incoming_pin).unwrap();
    let plan = prepare_source_import(&source, &old, &incoming, &base).unwrap();
    assert!(plan.conflicts().iter().any(|conflict| conflict.kind
        == SourceImportConflictKind::PermissionDenied
        && conflict.uuid.as_ref() == Some(&frozen)));
    assert!(plan.publish_source(&directory.0.join("output")).is_err());
    assert!(source.join("Catalogs/Sample/object.yaml").is_file());
}

/// A renamed UUID retains independent local descriptor/module edits, its pin and exact old origins.
#[test]
fn renamed_object_moves_local_files_and_retains_original_policy_and_origins() {
    let fixture = Fixture::new(false);
    rename_incoming(&fixture, "Helpers");
    let path = fixture.source.join("CommonModules/Tools/object.yaml");
    let text = fs::read_to_string(&path).unwrap();
    fs::write(
        path,
        format!(
            "# local annotation\n{}",
            text.replace("\"Server\": true", "\"Server\": false")
        ),
    )
    .unwrap();
    fs::write(
        fixture.source.join(MODULE),
        b"\xef\xbb\xbf// independent local\r\n",
    )
    .unwrap();
    let incoming = fixture.incoming();
    let original_base = Inventory::read(fixture.approved.storage_root()).unwrap();
    let plan = prepare_source_import(
        &fixture.source,
        &fixture.ancestor,
        &incoming,
        &fixture.approved,
    )
    .unwrap();
    assert!(plan.conflicts().is_empty(), "{:?}", plan.conflicts());
    let output = fixture.test.0.join("output");
    plan.publish_source(&output).unwrap();
    assert!(!output.join("CommonModules/Tools").exists());
    assert_eq!(
        fs::read(output.join("CommonModules/Helpers/Module.bsl")).unwrap(),
        b"\xef\xbb\xbf// independent local\r\n"
    );
    let text = fs::read_to_string(output.join("CommonModules/Helpers/object.yaml")).unwrap();
    assert!(
        text.contains("\"Name\": \"Helpers\"")
            && text.contains("\"Server\": false")
            && text.contains("# local annotation"),
        "{text}"
    );
    let prepared = prepare_protected(
        &output,
        &fixture.approved,
        &fixture.test.0.join("generations"),
        true,
    )
    .unwrap();
    let map = prepared.source_map();
    assert_eq!(map.schema_version, 6);
    let mapped = map
        .file(&SourcePath::parse("CommonModules/Helpers.xml").unwrap())
        .unwrap();
    let crate::project_core::source_format::FileOrigin::Metadata {
        accepted,
        working,
        uses_working_values,
        ..
    } = &mapped.origin
    else {
        panic!("metadata origin");
    };
    assert_eq!(accepted.path.as_str(), "CommonModules/Tools/object.yaml");
    assert_eq!(
        working.as_ref().unwrap().path.as_str(),
        "CommonModules/Helpers/object.yaml"
    );
    assert!(*uses_working_values);
    let native = fs::read(prepared.xml().join("CommonModules/Helpers.xml")).unwrap();
    let offset = String::from_utf8_lossy(&native).find("Helpers").unwrap();
    let old = fixture.approved.read_file(&accepted.path).unwrap();
    let location = mapped
        .locate(
            &native,
            offset..offset + 7,
            Some(crate::project_core::source_format::OriginTexts {
                accepted: &old,
                working: Some(&text),
            }),
        )
        .unwrap();
    assert_eq!(
        location.authority,
        crate::project_core::source_format::InputAuthority::Working
    );
    assert_eq!(
        location.input.path.as_str(),
        "CommonModules/Helpers/object.yaml"
    );
    let repeat = prepare_source_import(&output, &incoming, &incoming, &fixture.approved).unwrap();
    assert!(repeat.conflicts().is_empty(), "{:?}", repeat.conflicts());
    assert_eq!(repeat.working_revision(), repeat.output_revision());
    assert_eq!(
        Inventory::read(fixture.approved.storage_root()).unwrap(),
        original_base
    );
}

/// Freeze blocks rename through both incoming and manually edited maps under the same UUID.
#[test]
fn renamed_frozen_object_cannot_gain_permission_through_its_new_path() {
    let fixture = Fixture::new(true);
    rename_incoming(&fixture, "Helpers");
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
            .any(|conflict| conflict.kind == SourceImportConflictKind::PermissionDenied)
    );
    let output = fixture.test.0.join("output");
    assert!(plan.publish_source(&output).is_err());
    assert!(!output.exists());
    assert_eq!(
        prepare_protected(
            &fixture.incoming_source,
            &fixture.approved,
            &fixture.test.0.join("generations"),
            true
        )
        .unwrap_err()
        .error
        .code,
        "source_import_structure_denied"
    );
}

/// Incoming rename cannot silently leave a surviving native metadata reference at its old name.
#[test]
fn renamed_object_refuses_an_independent_stale_metadata_reference() {
    let fixture = Fixture::new(false);
    rename_incoming(&fixture, "Helpers");
    fs::write(fixture.source.join(MODULE), "// CommonModule.Tools\r\n").unwrap();
    let incoming = fixture.incoming();
    let error = prepare_source_import(
        &fixture.source,
        &fixture.ancestor,
        &incoming,
        &fixture.approved,
    )
    .err()
    .expect("stale reference");
    assert_eq!(error.code, "source_structural_reference_preserved");
    assert!(!fixture.test.0.join("generations").exists());
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

/// Only an exact supplied target-label transition may change an opaque handler, under its owner rights.
#[test]
fn renamed_object_preserves_handler_identity_and_refuses_extra_or_frozen_link_edits() {
    for mode in ["allowed", "extra_method", "frozen_owner"] {
        let fixture = Fixture::with_link(false, Some(mode == "frozen_owner"));
        rename_incoming(&fixture, "Helpers");
        let path = "EventSubscriptions/OnWrite/object.yaml";
        let text = fs::read_to_string(fixture.incoming_source.join(path)).unwrap();
        let next = if mode == "extra_method" {
            "CommonModule.Helpers.Other"
        } else {
            "CommonModule.Helpers.Perform"
        };
        fs::write(
            fixture.incoming_source.join(path),
            text.replace("CommonModule.Tools.Perform", next),
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
        let output = fixture.test.0.join("output");
        if mode != "allowed" {
            assert!(
                plan.conflicts()
                    .iter()
                    .any(|conflict| conflict.kind == SourceImportConflictKind::PermissionDenied),
                "{:?}",
                plan.conflicts()
            );
            assert!(plan.publish_source(&output).is_err());
            assert!(!output.exists());
            continue;
        }
        assert!(plan.conflicts().is_empty(), "{:?}", plan.conflicts());
        plan.publish_source(&output).unwrap();
        let prepared = prepare_protected(
            &output,
            &fixture.approved,
            &fixture.test.0.join("generations"),
            true,
        )
        .unwrap();
        let mapped = prepared
            .source_map()
            .file(&SourcePath::parse("EventSubscriptions/OnWrite.xml").unwrap())
            .unwrap();
        let current = fs::read_to_string(output.join(path)).unwrap();
        let accepted = fixture
            .approved
            .read_file(&SourcePath::parse(path).unwrap())
            .unwrap();
        let native = fs::read(prepared.xml().join("EventSubscriptions/OnWrite.xml")).unwrap();
        let start = String::from_utf8_lossy(&native).find(next).unwrap();
        let origin = mapped
            .locate(
                &native,
                start..start + next.len(),
                Some(crate::project_core::source_format::OriginTexts {
                    accepted: &accepted,
                    working: Some(&current),
                }),
            )
            .unwrap();
        assert_eq!(
            origin.authority,
            crate::project_core::source_format::InputAuthority::Working
        );
        assert!(current[origin.range.unwrap()].contains(next));
    }
}

/// Form/query bindings and a typed default-form reference follow the same UUID and local plain value.
#[test]
fn renamed_form_rebinds_query_slots_and_default_reference_without_copying_local_values() {
    use crate::project_core::source_format::queries::test_support::{LIST, fixture};
    let directory = TestDir::new();
    let native = fixture(&directory);
    let catalog = native.join("Catalogs/Sample.xml");
    let text = fs::read_to_string(&catalog).unwrap();
    fs::write(catalog, text.replace("<Template>Opaque</Template>", "")).unwrap();
    fs::remove_file(native.join("Catalogs/Sample/Templates/Opaque.xml")).unwrap();
    fs::remove_dir_all(native.join("Catalogs/Sample/Templates/Opaque")).unwrap();
    let working = directory.0.join("source");
    crate::project_core::source_format::import_designer_queries(&native, &working).unwrap();
    let base_path = directory.0.join("accepted");
    let pin = accept_source(&working, &base_path, &[]).unwrap();
    let base = AcceptedBase::open(&base_path, &pin).unwrap();
    let old_path = directory.0.join("ancestor");
    let old_pin = capture_import_ancestor(&working, &old_path).unwrap();
    let old = ImportAncestor::open(&old_path, &old_pin).unwrap();
    for path in ["Catalogs/Sample.xml", "Catalogs/Sample/Forms/Main.xml"] {
        let path = native.join(path);
        let text = fs::read_to_string(&path).unwrap();
        fs::write(
            path,
            text.replace(">Main<", ">RenamedMain<")
                .replace(".Form.Main<", ".Form.RenamedMain<"),
        )
        .unwrap();
    }
    fs::rename(
        native.join("Catalogs/Sample/Forms/Main.xml"),
        native.join("Catalogs/Sample/Forms/RenamedMain.xml"),
    )
    .unwrap();
    fs::rename(
        native.join("Catalogs/Sample/Forms/Main"),
        native.join("Catalogs/Sample/Forms/RenamedMain"),
    )
    .unwrap();
    let incoming_source = directory.0.join("incoming-source");
    crate::project_core::source_format::import_designer_queries(&native, &incoming_source).unwrap();
    let incoming_path = directory.0.join("incoming");
    let incoming_pin = capture_import_ancestor(&incoming_source, &incoming_path).unwrap();
    let incoming = ImportAncestor::open(&incoming_path, &incoming_pin).unwrap();
    let local = "SELECT 'local';\r\n";
    fs::write(working.join(LIST), local).unwrap();
    let plan = prepare_source_import(&working, &old, &incoming, &base).unwrap();
    assert!(plan.conflicts().is_empty(), "{:?}", plan.conflicts());
    let output = directory.0.join("output");
    plan.publish_source(&output).unwrap();
    let next = LIST.replace("/Main/", "/RenamedMain/");
    assert_eq!(fs::read_to_string(output.join(&next)).unwrap(), local);
    assert!(!output.join(LIST).exists());
    let prepared =
        prepare_protected(&output, &base, &directory.0.join("generations"), true).unwrap();
    let path = SourcePath::parse("Catalogs/Sample/Forms/RenamedMain/Ext/Form.xml").unwrap();
    let bytes = fs::read(prepared.xml().join(path.as_path())).unwrap();
    let start = String::from_utf8_lossy(&bytes)
        .find("SELECT 'local'")
        .unwrap();
    let location = prepared
        .source_map()
        .file(&path)
        .unwrap()
        .locate(&bytes, start..start + 6, None)
        .unwrap();
    assert_eq!(location.input.path.as_str(), next);
    assert_eq!(
        location.authority,
        crate::project_core::source_format::InputAuthority::Working
    );
    assert_eq!(&local[location.range.unwrap()], "SELECT");
}
