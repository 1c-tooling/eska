//! Saved edits, import publication and recovery share one nonblocking project write interval.

use super::*;
use crate::project_core::source_format::{prepare_module_edit, prepare_yaml_edit, source_write};

/// Every writer refuses before changing authoring bytes or journal state while another owns the lock.
#[test]
fn save_import_and_recovery_busy_preserve_source_and_receipts() {
    let fixture = Fixture::new(false);
    let root = fixture.project.root();
    let source = fixture.project.source();
    let authority = EskaSource::open(fixture.project.clone(), ProjectScope::Standalone)
        .unwrap()
        .edit_authority(&fixture.approved)
        .unwrap();
    let original_module = fs::read_to_string(source.join(MODULE)).unwrap();
    let module = prepare_module_edit(
        source,
        &authority,
        &SourcePath::parse(MODULE).unwrap(),
        Some(&original_module),
        "// saved after release\r\n",
    )
    .unwrap();
    let owner = SourcePath::parse("configuration.yaml").unwrap();
    let original = fs::read_to_string(source.join(owner.as_path())).unwrap();
    let updated = original.replace("1.0.0", "1.0.2");
    let yaml = prepare_yaml_edit(source, &authority, &owner, &original, &updated).unwrap();
    let noop = prepare_yaml_edit(source, &authority, &owner, &original, &original).unwrap();
    let import = fixture.plan();
    let guard = source_write::acquire(root).unwrap();
    let before = Inventory::read(root).unwrap();
    assert_eq!(
        module.publish(&authority).unwrap_err().code,
        "source_write_busy"
    );
    assert_eq!(
        yaml.publish(&authority).unwrap_err().code,
        "source_write_busy"
    );
    assert_eq!(
        noop.publish(&authority).unwrap_err().code,
        "source_write_busy"
    );
    assert_eq!(import.publish().unwrap_err().code, "source_write_busy");
    assert_eq!(
        recover_project_source_import(
            root,
            import.reference(),
            SourceImportRecovery::Complete,
            &|| false
        )
        .unwrap_err()
        .code,
        "source_write_busy"
    );
    assert_eq!(Inventory::read(root).unwrap(), before);
    assert!(!root.join(PROJECT_SOURCE_IMPORT_RECEIPT).exists());
    assert!(!root.join(SERVICE).exists());
    drop(guard);
    module.publish(&authority).unwrap();
    assert_eq!(
        fs::read(source.join(MODULE)).unwrap(),
        b"// saved after release\r\n"
    );
    assert!(root.join(".eska/source-write.lock").is_file());
}

/// Lock admission precedes the import marker check, which must still refuse after admission succeeds.
#[test]
fn stale_import_marker_refuses_save_after_the_write_lock_is_acquired() {
    let fixture = Fixture::new(false);
    let root = fixture.project.root();
    let source = fixture.project.source();
    let authority = EskaSource::open(fixture.project.clone(), ProjectScope::Standalone)
        .unwrap()
        .edit_authority(&fixture.approved)
        .unwrap();
    let original_module = fs::read_to_string(source.join(MODULE)).unwrap();
    let module = prepare_module_edit(
        source,
        &authority,
        &SourcePath::parse(MODULE).unwrap(),
        Some(&original_module),
        "// refused\r\n",
    )
    .unwrap();
    let owner = SourcePath::parse("configuration.yaml").unwrap();
    let original = fs::read_to_string(source.join(owner.as_path())).unwrap();
    let noop = prepare_yaml_edit(source, &authority, &owner, &original, &original).unwrap();
    fs::write(root.join(PROJECT_SOURCE_IMPORT_RECEIPT), "stale guard").unwrap();
    let guard = source_write::acquire(root).unwrap();
    let before = Inventory::read(root).unwrap();
    assert_eq!(
        module.publish(&authority).unwrap_err().code,
        "source_write_busy"
    );
    assert_eq!(
        noop.publish(&authority).unwrap_err().code,
        "source_write_busy"
    );
    drop(guard);
    assert_eq!(
        module.publish(&authority).unwrap_err().code,
        "source_import_incomplete"
    );
    assert_eq!(
        noop.publish(&authority).unwrap_err().code,
        "source_import_incomplete"
    );
    assert_eq!(Inventory::read(root).unwrap(), before);
    drop(source_write::acquire(root).unwrap());
}

/// A real guarded journal remains exact while both resume paths contend with another writer.
#[test]
fn interrupted_import_recovery_waits_for_the_same_stable_lock() {
    let fixture = Fixture::new(false);
    let root = fixture.project.root();
    let plan = fixture.plan();
    interrupt(&plan, "prepared");
    let guard = source_write::acquire(root).unwrap();
    let before = Inventory::read(root).unwrap();
    assert_eq!(plan.publish().unwrap_err().code, "source_write_busy");
    for action in [
        SourceImportRecovery::Complete,
        SourceImportRecovery::RollBack,
    ] {
        assert_eq!(
            recover_project_source_import(root, plan.reference(), action, &|| false)
                .unwrap_err()
                .code,
            "source_write_busy"
        );
        assert_eq!(Inventory::read(root).unwrap(), before);
    }
    drop(guard);
    assert_eq!(
        recover_project_source_import(
            root,
            plan.reference(),
            SourceImportRecovery::Complete,
            &|| false
        )
        .unwrap(),
        LayoutPublication::Resumed
    );
    assert_eq!(plan.publish().unwrap(), LayoutPublication::Unchanged);
    assert!(!root.join(PROJECT_SOURCE_IMPORT_RECEIPT).exists());
}
