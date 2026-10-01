//! Simulated termination tests interrupt real journal actions and reopen the writer lock.

use super::*;
use crate::{
    project::{
        metadata_model::ObjectId,
        metadata_rename::{RenameFile, RenameReplacement, inventory::Inventory},
    },
    test_support::TestDir,
};

/// Build a small source with exact BOM/CRLF bytes and a renamed payload containing an untouched binary.
fn fixture() -> (TestDir, PathBuf, RenamePlan) {
    let directory = TestDir::new();
    let source = directory.0.join("src");
    fs::create_dir_all(source.join("Catalogs/Old/Ext/Empty")).unwrap();
    let mut files = Vec::new();
    for (path, input) in [
        ("Configuration.xml", "\u{feff}<Child>Old</Child>\r\n"),
        ("Catalogs/Old.xml", "\u{feff}<Name>Old</Name>\r\n"),
        (
            "Catalogs/Old/Ext/Module.bsl",
            "\u{feff}A = Catalogs.Old;\r\n",
        ),
    ] {
        fs::write(source.join(path), input).unwrap();
        let start = input.find("Old").unwrap();
        files.push(RenameFile {
            path: path.into(),
            snapshot: snapshot(input),
            replacements: vec![RenameReplacement {
                range: start..start + 3,
                before: "Old".into(),
                after: "Новое".into(),
            }],
            uncertain: Vec::new(),
        });
    }
    fs::write(source.join("Catalogs/Old/Ext/picture.bin"), [0, 255, 1, 3]).unwrap();
    let id = ObjectId::from_parts(None, "catalog", "Old");
    let plan = RenamePlan {
        new_object_id: ObjectId::from_parts(None, "catalog", "Новое"),
        snapshot: super::super::inventory::snapshot(&source, &[], id.as_str(), "Новое").unwrap(),
        object_id: id,
        uuid: "uuid".into(),
        old_name: "Old".into(),
        new_name: "Новое".into(),
        files,
        moves: vec![
            RenameMove {
                from: "Catalogs/Old.xml".into(),
                to: "Catalogs/Новое.xml".into(),
                directory: false,
            },
            RenameMove {
                from: "Catalogs/Old".into(),
                to: "Catalogs/Новое".into(),
                directory: true,
            },
        ],
        issues: Vec::new(),
    };
    (directory, source, plan)
}

/// Include empty directories in byte equality checks, so losing a payload branch cannot pass.
fn bytes(
    source: &Path,
) -> (
    BTreeMap<PathBuf, Vec<u8>>,
    std::collections::BTreeSet<PathBuf>,
) {
    let inventory = Inventory::read(source, &[]).unwrap();
    (
        inventory
            .files
            .into_iter()
            .map(|path| {
                let bytes = fs::read(source.join(&path)).unwrap();
                (path, bytes)
            })
            .collect(),
        inventory.directories,
    )
}

/// Run the real preparation helper, retaining its directory as a simulated durable crash boundary.
fn prepare(guard: &Guard, source: &Path, plan: &RenamePlan) -> (Journal, PathBuf) {
    let directory = guard.directory.join("rename");
    fs::create_dir(&directory).unwrap();
    (Guard::prepare(source, plan, &directory).unwrap(), directory)
}

/// Every prefix of file writes and payload moves must restore byte-for-byte after reopening the lock.
#[test]
fn recovery_restores_every_interrupted_publication_step() {
    for count in 0..=5 {
        let (directory, source, plan) = fixture();
        let before = bytes(&source);
        let guard = Guard::acquire(&directory.0).unwrap();
        let (journal, staging) = prepare(&guard, &source, &plan);
        for (index, file) in journal.files.iter().enumerate().take(count) {
            journal.publish_file(&staging, index, &file.path).unwrap();
        }
        for movement in journal
            .moves
            .iter()
            .take(count.saturating_sub(journal.files.len()))
        {
            move_path(&source, movement, false).unwrap();
        }
        drop(guard);
        let guard = Guard::acquire(&directory.0).unwrap();
        assert!(matches!(
            guard.check_clear(&source),
            Err(EditError::RecoveryRequired)
        ));
        let status = guard.recovery_status(&source).unwrap();
        assert!(status.pending);
        assert!(
            status.conflicts.is_empty(),
            "step {count}: {:?}",
            status.conflicts
        );
        guard
            .recover(&source, status.snapshot.as_deref().unwrap())
            .unwrap();
        assert_eq!(bytes(&source), before, "step {count}");
        guard.check_clear(&source).unwrap();
    }
}

/// A completed operation preserves binary payloads, and a changed full-source snapshot publishes nothing.
#[test]
fn publication_checks_inventory_and_preserves_exact_payloads() {
    let (directory, source, plan) = fixture();
    let guard = Guard::acquire(&directory.0).unwrap();
    fs::write(source.join("new.bsl"), "// external").unwrap();
    let before = bytes(&source);
    assert!(matches!(
        guard.publish(&source, &plan, &[]),
        Err(EditError::Conflict)
    ));
    assert_eq!(bytes(&source), before);
    guard.check_clear(&source).unwrap();
    fs::remove_file(source.join("new.bsl")).unwrap();
    guard.publish(&source, &plan, &[]).unwrap();
    assert_eq!(
        fs::read_to_string(source.join("Catalogs/Новое/Ext/Module.bsl")).unwrap(),
        "\u{feff}A = Catalogs.Новое;\r\n"
    );
    assert_eq!(
        fs::read(source.join("Catalogs/Новое/Ext/picture.bin")).unwrap(),
        [0, 255, 1, 3]
    );
    assert!(!source.join("Catalogs/Old").exists());
    assert!(source.join("Catalogs/Новое/Ext/Empty").is_dir());
    guard.check_clear(&source).unwrap();
}

/// An outside edit after a crash remains intact; restoration requires the original journal token and bytes.
#[test]
fn recovery_preserves_external_edits_and_rejects_stale_tokens() {
    let (directory, source, plan) = fixture();
    let guard = Guard::acquire(&directory.0).unwrap();
    let (journal, staging) = prepare(&guard, &source, &plan);
    Guard::publish_steps(&journal, &staging).unwrap();
    let path = source.join("Catalogs/Новое/Ext/Module.bsl");
    let expected = fs::read(&path).unwrap();
    fs::write(&path, "external edit").unwrap();
    let before = bytes(&source);
    let status = guard.recovery_status(&source).unwrap();
    assert!(!status.conflicts.is_empty());
    assert!(matches!(
        guard.recover(&source, status.snapshot.as_deref().unwrap()),
        Err(EditError::Conflict)
    ));
    assert_eq!(bytes(&source), before);
    fs::write(path, expected).unwrap();
    assert!(matches!(
        guard.recover(&source, "wrong"),
        Err(EditError::Conflict)
    ));
    guard
        .recover(&source, status.snapshot.as_deref().unwrap())
        .unwrap();
}

/// A durable completion marker prevents partial cleanup from being mistaken for an unfinished write.
#[test]
fn recovery_finishes_interrupted_cleanup_without_undoing_a_committed_rename() {
    for remove_ready in [false, true] {
        let (directory, source, plan) = fixture();
        let guard = Guard::acquire(&directory.0).unwrap();
        let (journal, staging) = prepare(&guard, &source, &plan);
        Guard::publish_steps(&journal, &staging).unwrap();
        finish(&staging, b"committed").unwrap();
        fs::remove_file(staging.join("0.before")).unwrap();
        if remove_ready {
            fs::remove_file(staging.join("ready")).unwrap();
        }
        let before = bytes(&source);
        let status = guard.recovery_status(&source).unwrap();
        assert!(status.committed);
        guard
            .recover(&source, status.snapshot.as_deref().unwrap())
            .unwrap();
        assert_eq!(bytes(&source), before);
        guard.check_clear(&source).unwrap();
    }
}

/// Locks are per project and automatically released even when an earlier writer failed.
#[test]
fn write_lock_rejects_another_writer_until_the_owner_drops() {
    let (directory, source, _) = fixture();
    let guard = Guard::acquire(&directory.0).unwrap();
    assert!(matches!(Guard::acquire(&directory.0), Err(EditError::Busy)));
    drop(guard);
    Guard::acquire(&directory.0)
        .unwrap()
        .check_clear(&source)
        .unwrap();
}

/// A kill during staging or atomic replacement leaves temporary bytes inside the owned journal only.
#[test]
fn recovery_discards_interrupted_private_temporary_files() {
    let (directory, source, plan) = fixture();
    let before = bytes(&source);
    let guard = Guard::acquire(&directory.0).unwrap();
    let (_journal, staging) = prepare(&guard, &source, &plan);
    fs::write(staging.join("publish.tmp"), "incomplete candidate").unwrap();
    fs::write(staging.join("finish.tmp"), "incomplete marker").unwrap();
    let status = guard.recovery_status(&source).unwrap();
    guard
        .recover(&source, status.snapshot.as_deref().unwrap())
        .unwrap();
    assert_eq!(bytes(&source), before);
    guard.check_clear(&source).unwrap();
}

/// A half-written preparation has no ready marker, so it can be discarded without reading invalid JSON.
#[test]
fn recovery_discards_preparation_before_the_ready_marker() {
    let (directory, source, _) = fixture();
    let before = bytes(&source);
    let guard = Guard::acquire(&directory.0).unwrap();
    let staging = guard.directory.join("rename");
    fs::create_dir(&staging).unwrap();
    fs::write(staging.join("journal.json"), "{\"schemaVersion\":").unwrap();
    fs::write(staging.join("0.before"), "partial bytes").unwrap();
    let status = guard.recovery_status(&source).unwrap();
    guard
        .recover(&source, status.snapshot.as_deref().unwrap())
        .unwrap();
    assert_eq!(bytes(&source), before);
}

/// An inherited/cloned handle must not extend the metadata transaction beyond its owning guard.
#[test]
fn dropping_the_guard_releases_an_inherited_lock_handle() {
    let (directory, source, _) = fixture();
    let guard = Guard::acquire(&directory.0).unwrap();
    let inherited = guard.lock.try_clone().unwrap();
    drop(guard);
    Guard::acquire(&directory.0)
        .unwrap()
        .check_clear(&source)
        .unwrap();
    drop(inherited);
}

/// A write failure after an earlier replacement restores all changed source bytes automatically.
#[test]
fn publication_rolls_back_after_a_member_becomes_read_only() {
    for failed in 1..3 {
        let (directory, source, plan) = fixture();
        let before = bytes(&source);
        let guard = Guard::acquire(&directory.0).unwrap();
        let (journal, staging) = prepare(&guard, &source, &plan);
        let path = source.join(&journal.files[failed].path);
        let mut permissions = fs::metadata(&path).unwrap().permissions();
        permissions.set_readonly(true);
        fs::set_permissions(&path, permissions).unwrap();
        assert!(matches!(
            Guard::commit(&journal, &staging),
            Err(EditError::ReadOnly)
        ));
        assert_eq!(bytes(&source), before);
        guard.check_clear(&source).unwrap();
    }
}

/// A dependency added after the last member write must roll all published bytes back together.
#[test]
fn linked_property_postcheck_rolls_back_and_preserves_external_edits() {
    for external in [false, true] {
        let (directory, source, plan) = fixture();
        let before = bytes(&source);
        let guard = Guard::acquire(&directory.0).unwrap();
        let (journal, staging) = prepare(
            &guard,
            &source,
            &RenamePlan {
                moves: Vec::new(),
                ..plan
            },
        );
        let result = Guard::commit_checked(&journal, &staging, || {
            if external {
                fs::write(source.join("Configuration.xml"), "external").unwrap();
            }
            Err(EditError::Conflict)
        });
        if external {
            assert!(matches!(result, Err(EditError::RecoveryRequired)));
            assert_eq!(
                fs::read_to_string(source.join("Configuration.xml")).unwrap(),
                "external"
            );
            assert!(guard.recovery_status(&source).unwrap().pending);
        } else {
            assert!(matches!(result, Err(EditError::Conflict)));
            assert_eq!(bytes(&source), before);
            assert!(!staging.exists());
        }
    }
}
