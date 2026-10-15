//! Cancellation is a pre-commit decision, including after durable temporary-file preparation.

use super::*;
use crate::test_support::TestDir;
use std::cell::Cell;

/// No filesystem input is touched when the operation starts already cancelled.
#[test]
fn early_cancel_precedes_filesystem_reads_or_creation() {
    let test = TestDir::new();
    let absent = test.0.join("absent");
    assert!(matches!(
        publish_snapshot_cancellable(&absent, Path::new("file"), "old", "new", &|| true),
        Err(PublishError::Cancelled)
    ));
    assert!(matches!(
        publish_new_cancellable(&absent, Path::new("file"), "new", &|| true),
        Err(PublishError::Cancelled)
    ));
    assert!(!absent.exists());
}

/// The last rename check runs after temporary bytes were written, but before their publication.
#[test]
fn cancel_at_final_rename_keeps_old_bytes_and_removes_owned_temporary() {
    let test = TestDir::new();
    let path = test.0.join("file");
    fs::write(&path, "old").unwrap();
    let checks = Cell::new(0);
    let cancelled = || {
        checks.set(checks.get() + 1);
        if checks.get() == 2 {
            let temporary = fs::read_dir(&test.0)
                .unwrap()
                .map(|entry| entry.unwrap().path())
                .find(|entry| entry != &path)
                .unwrap();
            assert_eq!(fs::read(temporary).unwrap(), b"new");
        }
        checks.get() == 2
    };
    assert!(matches!(
        publish_snapshot_cancellable(&test.0, Path::new("file"), "old", "new", &cancelled),
        Err(PublishError::Cancelled)
    ));
    assert_eq!(checks.get(), 2);
    assert_eq!(fs::read(&path).unwrap(), b"old");
    assert_eq!(fs::read_dir(&test.0).unwrap().count(), 1);
}

/// A cancelled no-overwrite creation leaves no destination or abandoned temporary file.
#[test]
fn cancel_at_final_hard_link_never_creates_the_destination() {
    let test = TestDir::new();
    let checks = Cell::new(0);
    assert!(matches!(
        publish_new_cancellable(&test.0, Path::new("file"), "new", &|| {
            checks.set(checks.get() + 1);
            if checks.get() == 2 {
                let temporary = fs::read_dir(&test.0)
                    .unwrap()
                    .next()
                    .unwrap()
                    .unwrap()
                    .path();
                assert_eq!(fs::read(temporary).unwrap(), b"new");
                assert!(!test.0.join("file").exists());
            }
            checks.get() == 2
        }),
        Err(PublishError::Cancelled)
    ));
    assert_eq!(checks.get(), 2);
    assert_eq!(fs::read_dir(&test.0).unwrap().count(), 0);
}

/// Even an exact no-op returns cancellation before its unchanged outcome and keeps its timestamp.
#[test]
fn noop_honors_final_cancellation_without_replacing_saved_bytes() {
    let test = TestDir::new();
    let path = test.0.join("file");
    fs::write(&path, "old").unwrap();
    let modified = fs::metadata(&path).unwrap().modified().unwrap();
    let checks = Cell::new(0);
    assert!(matches!(
        publish_snapshot_cancellable(&test.0, Path::new("file"), "old", "old", &|| {
            checks.set(checks.get() + 1);
            checks.get() == 2
        }),
        Err(PublishError::Cancelled)
    ));
    assert_eq!(checks.get(), 2);
    assert_eq!(fs::read(&path).unwrap(), b"old");
    assert_eq!(fs::metadata(path).unwrap().modified().unwrap(), modified);
}

/// A successful commit is never followed by another hook that could disguise it as cancelled.
#[test]
fn successful_rename_and_creation_have_no_post_commit_cancellation_check() {
    for existing in [false, true] {
        let test = TestDir::new();
        let path = test.0.join("file");
        let checks = Cell::new(0);
        let cancelled = || {
            checks.set(checks.get() + 1);
            checks.get() > 2
        };
        if existing {
            fs::write(&path, "old").unwrap();
            publish_snapshot_cancellable(&test.0, Path::new("file"), "old", "new", &cancelled)
                .unwrap();
        } else {
            publish_new_cancellable(&test.0, Path::new("file"), "new", &cancelled).unwrap();
        }
        assert_eq!(checks.get(), 2);
        assert_eq!(fs::read(path).unwrap(), b"new");
    }
}
