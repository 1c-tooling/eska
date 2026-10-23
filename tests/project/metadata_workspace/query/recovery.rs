use std::{fs, sync::Arc};

use serde_json::{Value, json};

use crate::{
    backend::project::{
        metadata_edit::snapshot,
        query::{
            MAX_SNAPSHOT_BYTES,
            recovery::{Error, Part, Record, Store},
        },
    },
    support::TestDir,
};

/// Synthetic IDs are canonical v4 values without introducing a random-number dependency.
fn id(index: usize) -> String {
    format!("00000000-0000-4000-8000-{index:012x}")
}

/// Recovery binds an exact dirty owner independently of its base and current query draft.
fn metadata(index: usize, owner: &str) -> Value {
    json!({"schemaVersion":1,"id":id(index),"projectKey":"opaque member/context",
        "node":{"kind":"object","objectId":"opaque owner"},
        "source":{"value":"Ext/Template.xml","encoding":"utf-8"},
        "descriptor":{"value":"Template.xml","encoding":"utf-8"},
        "descriptorSnapshot":"a".repeat(64),"diskSnapshot":"b".repeat(64),
        "ownerSnapshot":snapshot(owner),"ownerVersion":"9007199254740993",
        "contextFingerprint":"c".repeat(64),"application":"app",
        "identity":{"kind":"dataSet","path":["Union","Query"]},"writable":true})
}

/// These texts are deliberately not parsed as source or treated as current write authorization.
fn record(index: usize) -> Record {
    let owner = "\u{feff}<query>SELECT 1</query>\r\n<!-- unsaved 😀 -->";
    Record::new(
        metadata(index, owner),
        [owner.into(), "SELECT 1".into(), "SELECT 2\nFROM T".into()],
    )
    .unwrap()
}

/// A cold backend recovers all dirty bytes, the application binding and the independent draft.
#[test]
fn backend_recovery_round_trips_without_writing_a_project_source() {
    let directory = TestDir::new();
    let path = directory.0.join("private");
    let store = Store::new(path.clone()).unwrap();
    assert!(store.load().unwrap().records.is_empty());
    assert!(!path.exists());
    let value = record(1);
    let acknowledged = store.save(value.clone(), None).unwrap();
    let loaded = Store::new(path.clone()).unwrap().load().unwrap();
    assert_eq!(loaded.damaged, [] as [String; 0]);
    assert!(!loaded.limited);
    assert_eq!(loaded.records.len(), 1);
    assert_eq!(loaded.records[0].revision, acknowledged.revision);
    for part in [Part::Metadata, Part::Owner, Part::Base, Part::Draft] {
        assert_eq!(
            loaded.records[0].record.part(part).unwrap(),
            value.part(part).unwrap()
        );
    }
    let persisted = fs::read(path.join(format!("{}.json", id(1)))).unwrap();
    assert_eq!(
        store
            .save(value, Some(&acknowledged.revision))
            .unwrap()
            .revision,
        acknowledged.revision
    );
    assert_eq!(
        fs::read(path.join(format!("{}.json", id(1)))).unwrap(),
        persisted
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(path.join(format!("{}.json", id(1))))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }
}

/// The original TypeScript JSON/base64 format does not need a lossy migration or deletion.
#[test]
fn backend_recovery_reads_existing_v1_files_with_their_exact_revision() {
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    let directory = TestDir::new();
    let value = record(2);
    let mut legacy = value.metadata().clone();
    legacy["storageVersion"] = json!(1);
    for (name, part) in [
        ("ownerText", Part::Owner),
        ("base", Part::Base),
        ("draft", Part::Draft),
    ] {
        legacy[name] = json!(STANDARD.encode(value.part(part).unwrap().as_bytes()));
    }
    let text = serde_json::to_string_pretty(&legacy).unwrap();
    fs::write(directory.0.join(format!("{}.json", value.id())), &text).unwrap();
    let store = Store::new(directory.0.clone()).unwrap();
    let loaded = store.load().unwrap();
    assert_eq!(loaded.records[0].revision, snapshot(&text));
    assert_eq!(
        loaded.records[0].record.part(Part::Owner).unwrap(),
        value.part(Part::Owner).unwrap()
    );
    assert_eq!(
        fs::read_to_string(directory.0.join(format!("{}.json", value.id()))).unwrap(),
        text
    );
}

/// A second IDE window cannot overwrite, remove or resurrect another window's acknowledged revision.
#[test]
fn backend_recovery_compares_revisions_across_independent_store_instances() {
    let directory = TestDir::new();
    let first = Store::new(directory.0.clone()).unwrap();
    let second = Store::new(directory.0.clone()).unwrap();
    let original = first.save(record(3), None).unwrap();
    let mut texts = [Part::Owner, Part::Base, Part::Draft]
        .map(|part| Arc::from(original.record.part(part).unwrap().as_ref()));
    texts[2] = "SELECT 99".into();
    let updated = first
        .save(
            Record::new(original.record.metadata().clone(), texts).unwrap(),
            Some(&original.revision),
        )
        .unwrap();
    assert!(matches!(
        second.save(record(3), Some(&original.revision)),
        Err(Error::Conflict)
    ));
    assert!(matches!(
        second.remove(&id(3), &original.revision),
        Err(Error::Conflict)
    ));
    assert!(matches!(second.save(record(3), None), Err(Error::Conflict)));
    assert_eq!(
        second.load().unwrap().records[0]
            .record
            .part(Part::Draft)
            .unwrap()
            .as_ref(),
        "SELECT 99"
    );
    assert!(second.remove(&id(3), &updated.revision).unwrap());
    assert!(matches!(
        first.save(record(3), Some(&updated.revision)),
        Err(Error::Conflict)
    ));
    assert!(!second.remove(&id(3), &updated.revision).unwrap());
}

/// Corrupt data and unrelated manual copies remain available for explicit recovery.
#[test]
fn backend_recovery_preserves_damaged_records_and_unrelated_files() {
    let directory = TestDir::new();
    fs::write(
        directory.0.join(format!("{}.json", id(4))),
        "{bad checkpoint",
    )
    .unwrap();
    fs::write(directory.0.join("manual-copy.json"), "keep").unwrap();
    let store = Store::new(directory.0.clone()).unwrap();
    let result = store.load().unwrap();
    assert!(result.records.is_empty());
    assert_eq!(result.damaged, [id(4)]);
    assert!(matches!(store.save(record(4), None), Err(Error::Conflict)));
    store.save(record(5), None).unwrap();
    assert_eq!(
        fs::read_to_string(directory.0.join(format!("{}.json", id(4)))).unwrap(),
        "{bad checkpoint"
    );
    assert_eq!(
        fs::read_to_string(directory.0.join("manual-copy.json")).unwrap(),
        "keep"
    );
}

/// Every private binding is checked by ESKA even if an adapter omits its own input validation.
#[test]
fn backend_recovery_rejects_malformed_bindings_and_mismatching_owner_hashes() {
    let value = record(6);
    let texts = [Part::Owner, Part::Base, Part::Draft]
        .map(|part| Arc::from(value.part(part).unwrap().as_ref()));
    for replacement in [
        json!({"id":"../source.xml"}),
        json!({"schemaVersion":2}),
        json!({"ownerVersion":"0"}),
        json!({"ownerVersion":"18446744073709551616"}),
        json!({"ownerSnapshot":"f".repeat(64)}),
        json!({"application":""}),
        json!({"source":null}),
        json!({"identity":{"kind":"dataSet","path":[]}}),
        json!({"extra":1}),
    ] {
        let mut binding = value.metadata().clone();
        binding
            .as_object_mut()
            .unwrap()
            .extend(replacement.as_object().unwrap().clone());
        assert!(Record::new(binding, texts.clone()).is_err());
    }
    assert!(matches!(
        Record::new(
            value.metadata().clone(),
            ["changed owner".into(), texts[1].clone(), texts[2].clone()]
        ),
        Err(Error::InvalidRecord)
    ));
    let mut metadata = value.metadata().clone();
    metadata["projectKey"] = json!("x".repeat(1_048_577));
    assert!(matches!(
        Record::new(metadata, texts),
        Err(Error::ResourceLimit)
    ));
}

/// Quotas apply across windows and include the complete owner plus both query revisions.
#[test]
fn backend_recovery_bounds_record_count_and_keeps_previous_checkpoints_on_failure() {
    let directory = TestDir::new();
    let store = Store::new(directory.0.clone()).unwrap();
    for index in 0..16 {
        store.save(record(index), None).unwrap();
    }
    assert!(matches!(
        Store::new(directory.0.clone())
            .unwrap()
            .save(record(17), None),
        Err(Error::ResourceLimit)
    ));
    assert_eq!(store.load().unwrap().records.len(), 16);
    assert!(fs::read_dir(&directory.0).unwrap().all(|entry| {
        !entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .ends_with(".tmp")
    }));
}

/// Base64 bounds control-character overhead while preserving full-size unfinished drafts.
#[test]
fn backend_recovery_bounds_logical_bytes_independently_of_json_expansion() {
    let directory = TestDir::new();
    let store = Store::new(directory.0.clone()).unwrap();
    let text: Arc<str> = "\0".repeat(MAX_SNAPSHOT_BYTES).into();
    let stored = store
        .save(
            Record::new(
                metadata(18, &text),
                [text.clone(), text.clone(), text.clone()],
            )
            .unwrap(),
            None,
        )
        .unwrap();
    let too_much = Record::new(metadata(19, &text), [text.clone(), "".into(), "x".into()]).unwrap();
    assert!(matches!(
        store.save(too_much, None),
        Err(Error::ResourceLimit)
    ));
    let loaded = store.load().unwrap();
    assert_eq!(loaded.records.len(), 1);
    assert_eq!(loaded.records[0].revision, stored.revision);
    assert_eq!(
        loaded.records[0].record.part(Part::Draft).unwrap().as_ref(),
        text.as_ref()
    );
    let oversized: Arc<str> = "x".repeat(MAX_SNAPSHOT_BYTES + 1).into();
    assert!(matches!(
        Record::new(metadata(20, ""), ["".into(), "".into(), oversized]),
        Err(Error::ResourceLimit)
    ));
}

/// The OS lock is temporary ownership of storage, independent of persistent support restrictions.
#[test]
fn backend_recovery_store_lock_releases_with_its_handle() {
    let directory = TestDir::new();
    let lock = fs::File::create_new(directory.0.join(".eska-query-recovery.lock")).unwrap();
    lock.try_lock().unwrap();
    let store = Store::new(directory.0.clone()).unwrap();
    assert!(matches!(store.save(record(21), None), Err(Error::Busy)));
    drop(lock);
    assert!(store.save(record(21), None).is_ok());
}

/// Aliased directories, records and lock files cannot redirect checkpoint writes.
#[cfg(unix)]
#[test]
fn backend_recovery_rejects_symlinked_storage_components() {
    use std::os::unix::fs::symlink;
    let directory = TestDir::new();
    fs::create_dir(directory.0.join("actual")).unwrap();
    symlink(directory.0.join("actual"), directory.0.join("alias")).unwrap();
    let store = Store::new(directory.0.join("alias/new-private")).unwrap();
    assert!(matches!(
        store.save(record(22), None),
        Err(Error::UnsafePath)
    ));
    assert!(!directory.0.join("actual/new-private").exists());
    let store = Store::new(directory.0.clone()).unwrap();
    fs::write(directory.0.join("manual-copy"), "keep").unwrap();
    symlink(
        directory.0.join("manual-copy"),
        directory.0.join(format!("{}.json", id(22))),
    )
    .unwrap();
    assert_eq!(store.load().unwrap().damaged, [id(22)]);
    symlink(
        directory.0.join("manual-copy"),
        directory.0.join(".eska-query-recovery.lock"),
    )
    .unwrap();
    assert!(matches!(
        store.save(record(23), None),
        Err(Error::UnsafePath)
    ));
    assert_eq!(
        fs::read_to_string(directory.0.join("manual-copy")).unwrap(),
        "keep"
    );
}
