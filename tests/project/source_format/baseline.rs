use std::{
    cell::Cell,
    collections::BTreeMap,
    fmt::Write,
    fs,
    path::{Path, PathBuf},
};

use eska::project::{
    source_format::{
        AcceptedBase, NativeUuid, Operation, PermissionReason, PropertyName, SourcePath,
        accept_source, import_designer,
    },
    support::{Reason, State},
};
use sha2::{Digest, Sha256};

use crate::support::TestDir;

/// Supplier payloads in unit fixtures are opaque sentinels; native CF validity is a separate gate.
pub(super) fn fixture(root: &Path) -> PathBuf {
    let input = super::fixture(root);
    fs::create_dir(input.join("Ext/ParentConfigurations")).unwrap();
    fs::write(
        input.join("Ext/ParentConfigurations/SampleConfiguration.cf"),
        b"synthetic opaque CF sentinel",
    )
    .unwrap();
    input
}

/// Use predictable fixture identities without repeating long UUID spellings in assertions.
pub(super) fn uuid(prefix: char) -> NativeUuid {
    NativeUuid::parse(&format!("{prefix}0000000-0000-4000-8000-000000000001")).unwrap()
}

/// Compare exact small source bytes, including empty directories.
pub(super) fn tree(root: &Path) -> BTreeMap<PathBuf, Option<Vec<u8>>> {
    fn visit(root: &Path, path: &Path, values: &mut BTreeMap<PathBuf, Option<Vec<u8>>>) {
        for entry in fs::read_dir(path).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            if entry.file_type().unwrap().is_dir() {
                values.insert(path.strip_prefix(root).unwrap().into(), None);
                visit(root, &path, values);
            } else {
                values.insert(
                    path.strip_prefix(root).unwrap().into(),
                    Some(fs::read(path).unwrap()),
                );
            }
        }
    }
    let mut result = BTreeMap::new();
    visit(root, root, &mut result);
    result
}

/// Early and late cancellation cannot grant permissions, alter storage or poison another open.
#[test]
fn accepted_verification_cancellation_keeps_trust_and_readonly_recovery() {
    let test = TestDir::new();
    let source = test.0.join("source");
    import_designer(&fixture(&test.0), &source).unwrap();
    let root = test.0.join("accepted");
    let reference = accept_source(&source, &root, &[]).unwrap();
    let before = tree(&root);
    let absent = test.0.join("absent");
    assert_eq!(
        AcceptedBase::open_cancellable(&absent, &reference, &|| true)
            .err()
            .unwrap()
            .code,
        "source_read_cancelled"
    );
    assert_eq!(
        AcceptedBase::open(&absent, &reference).err().unwrap().code,
        "source_baseline_missing"
    );
    let observed = Cell::new(0);
    let completed = AcceptedBase::open_cancellable(&root, &reference, &|| {
        observed.set(observed.get() + 1);
        false
    })
    .unwrap();
    let expected = completed.permission(&uuid('2'), Operation::Module);
    for stop_at in [1, observed.get() / 2, observed.get()] {
        let checked = Cell::new(0);
        let error = AcceptedBase::open_cancellable(&root, &reference, &|| {
            checked.set(checked.get() + 1);
            checked.get() >= stop_at
        })
        .err()
        .unwrap();
        assert_eq!(error.code, "source_read_cancelled");
        assert_eq!(tree(&root), before);
        let recovered = AcceptedBase::open(&root, &reference).unwrap();
        assert_eq!(
            recovered.permission(&uuid('2'), Operation::Module).reason,
            expected.reason
        );
    }
    let manifest = root.join("accepted.json");
    fs::write(&manifest, b"corrupted independently pinned manifest").unwrap();
    assert_eq!(
        AcceptedBase::open_cancellable(&root, &reference, &|| false)
            .err()
            .unwrap()
            .code,
        "source_baseline_untrusted"
    );
}

#[test]
fn accepted_bytes_restore_on_a_clean_checkout_without_originals_or_cache() {
    let test = TestDir::new();
    let input = fixture(&test.0);
    fs::create_dir_all(input.join("Empty/Deep")).unwrap();
    let source = test.0.join("source");
    import_designer(&input, &source).unwrap();
    let expected = tree(&source);
    let base = test.0.join("accepted");
    let reference = accept_source(&source, &base, &[]).unwrap();
    let clean = test.0.join("clean-base");
    super::copy_tree(&base, &clean);
    fs::remove_dir_all(base).unwrap();
    fs::remove_dir_all(input).unwrap();
    fs::remove_dir_all(source).unwrap();
    let restored = test.0.join("restored");
    AcceptedBase::open(&clean, &reference)
        .unwrap()
        .restore(&restored)
        .unwrap();
    assert_eq!(tree(&restored), expected);
    let again = test.0.join("accepted-again");
    assert_eq!(reference, accept_source(&restored, &again, &[]).unwrap());
}

#[test]
fn accepted_policy_keeps_mixed_inline_permissions_and_ignores_working_edits() {
    let test = TestDir::new();
    let input = fixture(&test.0);
    let source = test.0.join("source");
    import_designer(&input, &source).unwrap();
    let base = test.0.join("accepted");
    let reference = accept_source(&source, &base, &[]).unwrap();
    let accepted = AcceptedBase::open(&base, &reference).unwrap();
    let parent = accepted.permission(&uuid('2'), Operation::Property(PropertyName::Comment));
    assert!(!parent.allowed);
    assert_eq!(parent.reason, PermissionReason::NativeRestriction);
    assert_eq!(parent.native_reason, Reason::VendorLocked);
    let child = accepted.permission(&uuid('3'), Operation::Property(PropertyName::Comment));
    assert!(child.allowed);
    assert_eq!(child.state, State::EditableWithSupport);
    let removed = accepted.permission(&uuid('4'), Operation::Resource);
    assert!(removed.allowed);
    assert_eq!(removed.native_reason, Reason::SupportRemoved);
    let own = accepted.permission(&uuid('6'), Operation::Module);
    assert!(own.allowed);
    assert_eq!(own.native_reason, Reason::OwnObject);
    fs::write(
        source.join("Catalogs/Sample/object.yaml"),
        "uuid: changed\nstate: unrestricted\n",
    )
    .unwrap();
    fs::write(source.join("Ext/ParentConfigurations.bin"), "{6,0,0,0,0,0}").unwrap();
    let accepted = AcceptedBase::open(&base, &reference).unwrap();
    assert!(!accepted.permission(&uuid('2'), Operation::Resource).allowed);
    assert!(
        accepted
            .permission(&uuid('3'), Operation::Property(PropertyName::Comment))
            .allowed
    );
    let unknown = accepted.permission(&uuid('f'), Operation::Module);
    assert!(!unknown.allowed);
    assert_eq!(unknown.reason, PermissionReason::UnknownObject);
    assert_eq!(unknown.baseline, reference.sha256);
    assert!(!accepted.permission(&uuid('6'), Operation::Rename).allowed);
}

/// Text-module previews resolve pinned ownership, rejecting resources, binaries and undeclared paths.
#[test]
fn module_permission_uses_accepted_paths_and_ignores_working_identity_changes() {
    let test = TestDir::new();
    let source = test.0.join("source");
    let input = fixture(&test.0);
    fs::write(
        input.join("Catalogs/Sample/Ext/ObjectModule.bsl"),
        "// Locked module.\r\n",
    )
    .unwrap();
    import_designer(&input, &source).unwrap();
    let root = test.0.join("accepted");
    let reference = accept_source(&source, &root, &[]).unwrap();
    let base = AcceptedBase::open(&root, &reference).unwrap();
    let own = SourcePath::parse("CommonModules/Tools/Ext/Module.bsl").unwrap();
    let locked = SourcePath::parse("Catalogs/Sample/Ext/ObjectModule.bsl").unwrap();
    assert!(base.module_permission(&own).unwrap().allowed);
    let permission = base.module_permission(&locked).unwrap();
    assert_eq!(permission.uuid, uuid('2'));
    assert_eq!(permission.reason, PermissionReason::NativeRestriction);
    fs::write(
        source.join("Catalogs/Sample/object.yaml"),
        "uuid: altered\nstate: unrestricted\n",
    )
    .unwrap();
    assert!(!base.module_permission(&locked).unwrap().allowed);
    for path in [
        "CommonModules/Tools/object.yaml",
        "CommonModules/Tools/Ext/Module.bin",
        "CommonModules/Tools/Ext/UnknownModule.bsl",
    ] {
        assert_eq!(
            base.module_permission(&SourcePath::parse(path).unwrap())
                .unwrap_err()
                .code,
            "source_edit_module_unsupported"
        );
    }
    let frozen_root = test.0.join("accepted-frozen");
    // Restore the source before making a distinct accepted base with a genuine freeze.
    let pristine = test.0.join("pristine");
    import_designer(&input, &pristine).unwrap();
    let reference = accept_source(&pristine, &frozen_root, &[uuid('6')]).unwrap();
    let frozen = AcceptedBase::open(&frozen_root, &reference).unwrap();
    assert_eq!(
        frozen.module_permission(&own).unwrap().reason,
        PermissionReason::EskaFrozen
    );
}

#[test]
fn additional_freeze_inherits_through_inline_and_external_ownership() {
    for target in ['1', '2', '3'] {
        let test = TestDir::new();
        let source = test.0.join("source");
        import_designer(&fixture(&test.0), &source).unwrap();
        let base = test.0.join("accepted");
        let reference = accept_source(&source, &base, &[uuid(target)]).unwrap();
        let accepted = AcceptedBase::open(&base, &reference).unwrap();
        let child = accepted.permission(&uuid('3'), Operation::Property(PropertyName::Comment));
        assert!(!child.allowed);
        assert_eq!(child.reason, PermissionReason::EskaFrozen);
        assert_eq!(child.state, State::EditableWithSupport);
        let form = accepted.permission(&uuid('7'), Operation::Resource);
        assert_eq!(form.allowed, target == '3');
        assert_eq!(
            accepted.permission(&uuid('6'), Operation::Module).allowed,
            target != '1'
        );
    }
}

/// Different suppliers intersect their rules rather than letting one editable flag win.
#[test]
fn accepted_permissions_intersect_multiple_complete_supplier_deliveries() {
    let test = TestDir::new();
    let input = fixture(&test.0);
    fs::write(
        input.join("Ext/ParentConfigurations/SecondSupplier.cf"),
        b"second opaque CF sentinel",
    )
    .unwrap();
    let supplier = |id: char, name: &str, rule: u8| {
        format!(
            "{},0,{},\"1.0.0\",\"Fixture vendor\",\"{name}\",1,{rule},0,{},{},0,0",
            uuid(id).as_str(),
            uuid('1').as_str(),
            uuid('3').as_str(),
            uuid('3').as_str()
        )
    };
    let first = supplier('a', "SampleConfiguration", 1);
    let second = supplier('b', "SecondSupplier", 0);
    let suffix = std::iter::repeat_n("0", 23).collect::<Vec<_>>().join(",");
    fs::write(
        input.join("Ext/ParentConfigurations.bin"),
        format!("{{6,0,2,{first},{second},{suffix}}}"),
    )
    .unwrap();
    let source = test.0.join("source");
    import_designer(&input, &source).unwrap();
    let base = test.0.join("accepted");
    let reference = accept_source(&source, &base, &[]).unwrap();
    let permission = AcceptedBase::open(&base, &reference)
        .unwrap()
        .permission(&uuid('3'), Operation::Property(PropertyName::Comment));
    assert!(!permission.allowed);
    assert_eq!(permission.native_reason, Reason::VendorLocked);
}

#[test]
fn explicitly_accepted_configuration_without_support_has_only_known_own_objects() {
    let test = TestDir::new();
    let input = super::fixture(&test.0);
    fs::remove_file(input.join("Ext/ParentConfigurations.bin")).unwrap();
    let source = test.0.join("source");
    import_designer(&input, &source).unwrap();
    let base = test.0.join("accepted");
    let reference = accept_source(&source, &base, &[]).unwrap();
    let accepted = AcceptedBase::open(&base, &reference).unwrap();
    assert!(accepted.permission(&uuid('2'), Operation::Module).allowed);
    assert!(!accepted.permission(&uuid('f'), Operation::Module).allowed);
}

#[test]
fn baseline_requires_every_blob_and_an_independent_trust_reference() {
    for fault in ["pin", "manifest", "blob_missing", "blob_corrupt", "header"] {
        let test = TestDir::new();
        let source = test.0.join("source");
        import_designer(&fixture(&test.0), &source).unwrap();
        let base = test.0.join("accepted");
        let mut reference = accept_source(&source, &base, &[]).unwrap();
        let marker = base.join("accepted.json");
        let mut manifest: serde_json::Value =
            serde_json::from_slice(&fs::read(&marker).unwrap()).unwrap();
        let hash = manifest["files"][0]["sha256"].as_str().unwrap();
        let blob = base.join(format!("blobs/{}/{}", &hash[..2], hash));
        let code = match fault {
            "pin" => {
                reference.sha256 = "0".repeat(64);
                "source_baseline_untrusted"
            }
            "manifest" => {
                fs::remove_file(&marker).unwrap();
                "source_baseline_missing"
            }
            "blob_missing" => {
                fs::remove_file(blob).unwrap();
                "source_baseline_blob_missing"
            }
            "blob_corrupt" => {
                fs::write(blob, b"corrupt").unwrap();
                "source_baseline_corrupt"
            }
            _ => {
                manifest["configuration_uuid"] = serde_json::json!(uuid('2'));
                let bytes = serde_json::to_vec_pretty(&manifest).unwrap();
                reference.sha256 =
                    Sha256::digest(&bytes)
                        .iter()
                        .fold(String::new(), |mut value, byte| {
                            write!(value, "{byte:02x}").unwrap();
                            value
                        });
                fs::write(marker, bytes).unwrap();
                "source_baseline_corrupt"
            }
        };
        assert_eq!(
            AcceptedBase::open(&base, &reference).err().unwrap().code,
            code,
            "{fault}"
        );
    }
}

#[test]
fn acceptance_refuses_incomplete_policy_or_changed_supplier_bytes() {
    for fault in [
        "policy",
        "missing_cf",
        "changed_cf",
        "changed_rules",
        "freeze",
    ] {
        let test = TestDir::new();
        let input = fixture(&test.0);
        if fault == "policy" {
            fs::write(input.join("Ext/ParentConfigurations.bin"), "{6,broken}").unwrap();
        }
        if fault == "missing_cf" {
            fs::remove_file(input.join("Ext/ParentConfigurations/SampleConfiguration.cf")).unwrap();
        }
        let source = test.0.join("source");
        import_designer(&input, &source).unwrap();
        if fault == "changed_cf" {
            fs::write(
                source.join("Ext/ParentConfigurations/SampleConfiguration.cf"),
                b"changed",
            )
            .unwrap();
        }
        if fault == "changed_rules" {
            fs::write(source.join("Ext/ParentConfigurations.bin"), "{6,0,0,0,0,0}").unwrap();
        }
        let base = test.0.join("accepted");
        let freeze = if fault == "freeze" {
            vec![uuid('f')]
        } else {
            vec![]
        };
        let code = match fault {
            "policy" => "source_baseline_support_invalid",
            "missing_cf" => "source_baseline_supplier_missing",
            "freeze" => "source_freeze_object_unknown",
            _ => "source_baseline_policy_changed",
        };
        assert_eq!(
            accept_source(&source, &base, &freeze).unwrap_err().code,
            code
        );
        assert!(!base.exists());
    }
}

#[test]
fn acceptance_and_restore_never_replace_existing_user_directories() {
    let test = TestDir::new();
    let source = test.0.join("source");
    import_designer(&fixture(&test.0), &source).unwrap();
    let before = tree(&source);
    assert_eq!(
        accept_source(&source, &source.join("inside"), &[])
            .unwrap_err()
            .code,
        "source_tree_overlap"
    );
    assert_eq!(
        accept_source(&source, &source, &[]).unwrap_err().code,
        "source_tree_overlap"
    );
    let base = test.0.join("accepted");
    let reference = accept_source(&source, &base, &[]).unwrap();
    assert_eq!(
        accept_source(&source, &base, &[]).unwrap_err().code,
        "source_destination_exists"
    );
    assert_eq!(
        AcceptedBase::open(&base, &reference)
            .unwrap()
            .restore(&source)
            .unwrap_err()
            .code,
        "source_destination_exists"
    );
    assert_eq!(tree(&source), before);
}

/// Verify real pinned CF/rule bytes separately from the synthetic supplier-payload sentinels.
#[test]
#[ignore = "Real baseline acceptance; requires explicit playground source and new destinations"]
fn real_playground_baseline() {
    let root = PathBuf::from(std::env::var_os("ESKA_TEST_ROOT").unwrap())
        .canonicalize()
        .unwrap();
    let source = PathBuf::from(std::env::var_os("ESKA_FORMAT_INPUT").unwrap())
        .canonicalize()
        .unwrap();
    let base = PathBuf::from(std::env::var_os("ESKA_FORMAT_BASE").unwrap());
    let restored = PathBuf::from(std::env::var_os("ESKA_FORMAT_OUTPUT").unwrap());
    assert!(source.starts_with(&root));
    for path in [&base, &restored] {
        assert!(
            path.parent()
                .unwrap()
                .canonicalize()
                .unwrap()
                .starts_with(&root)
        );
    }
    let reference = accept_source(&source, &base, &[]).unwrap();
    let accepted = AcceptedBase::open(&base, &reference).unwrap();
    accepted.restore(&restored).unwrap();
    let manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(base.join("accepted.json")).unwrap()).unwrap();
    for file in manifest["files"].as_array().unwrap() {
        let path = file["path"].as_str().unwrap();
        assert_eq!(
            fs::read(source.join(path)).unwrap(),
            fs::read(restored.join(path)).unwrap(),
            "{path}"
        );
    }
    println!(
        "{}",
        serde_json::json!({"files":manifest["files"].as_array().unwrap().len(), "suppliers":manifest["suppliers"].as_array().unwrap().len(), "supplier_bytes":manifest["suppliers"].as_array().unwrap().iter().map(|supplier|supplier["bytes"].as_u64().unwrap()).sum::<u64>(), "reference":reference.sha256})
    );
}
