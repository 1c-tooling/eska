//! Independent handles and processes coordinate through a lock path that is never removed.

use super::*;
use crate::test_support::TestDir;

/// Reopening the stable path must contend even within one process and retain its inode on drop.
#[test]
fn independent_handles_contend_and_drop_allows_reacquisition() {
    let test = TestDir::new();
    let first = acquire(&test.0).unwrap();
    let path = test.0.join(".eska/source-write.lock");
    let before = fs::metadata(&path).unwrap();
    assert_eq!(acquire(&test.0).unwrap_err().code, "source_write_busy");
    let root = test.0.clone();
    assert_eq!(
        std::thread::spawn(move || acquire(&root).unwrap_err().code)
            .join()
            .unwrap(),
        "source_write_busy"
    );
    drop(first);
    assert!(path.is_file());
    let second = acquire(&test.0).unwrap();
    assert_eq!(fs::read(&path).unwrap(), b"");
    assert_eq!(
        fs::metadata(&path).unwrap().modified().unwrap(),
        before.modified().unwrap()
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        assert_eq!(fs::metadata(&path).unwrap().ino(), before.ino());
    }
    drop(second);
    assert!(path.is_file());
}

/// Explicit unlock prevents an inherited or duplicated descriptor from extending the interval.
#[test]
fn dropping_guard_unlocks_even_while_a_duplicate_file_handle_remains_open() {
    let test = TestDir::new();
    let first = acquire(&test.0).unwrap();
    let duplicate = first.file.try_clone().unwrap();
    drop(first);
    let _next = acquire(&test.0).unwrap();
    drop(duplicate);
    assert_eq!(acquire(&test.0).unwrap_err().code, "source_write_busy");
}

/// A child opens its own handle instead of inheriting the parent's lock guard.
#[test]
fn another_process_observes_the_same_busy_lock() {
    let test = TestDir::new();
    let _guard = acquire(&test.0).unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "project_core::source_format::source_write::tests::process_probe",
            "--nocapture",
            "--test-threads=1",
        ])
        .env("ESKA_SOURCE_LOCK_PROBE_ROOT", &test.0)
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    assert!(String::from_utf8_lossy(&output.stdout).contains("source_write_busy"));
}

/// This exact subprocess entry runs only with the parent test's task-local directory.
#[test]
fn process_probe() {
    let Some(root) = std::env::var_os("ESKA_SOURCE_LOCK_PROBE_ROOT") else {
        return;
    };
    assert_eq!(
        acquire(Path::new(&root)).unwrap_err().code,
        "source_write_busy"
    );
    println!("source_write_busy");
}

/// Existing root, service and file aliases never redirect even an empty lock-file creation.
#[cfg(unix)]
#[test]
fn root_service_and_lock_aliases_and_nonregular_files_are_refused() {
    use std::os::unix::fs::symlink;

    let test = TestDir::new();
    let project = test.0.join("project");
    let foreign = test.0.join("foreign");
    fs::create_dir(&project).unwrap();
    fs::create_dir(&foreign).unwrap();
    fs::write(foreign.join("sentinel"), "untouched").unwrap();
    let alias = test.0.join("alias");
    symlink(&project, &alias).unwrap();
    assert_eq!(acquire(&alias).unwrap_err().code, "source_edit_unsafe_path");
    symlink(&foreign, project.join(".eska")).unwrap();
    assert_eq!(
        acquire(&project).unwrap_err().code,
        "source_edit_unsafe_path"
    );
    assert!(!foreign.join("source-write.lock").exists());
    fs::remove_file(project.join(".eska")).unwrap();
    fs::create_dir(project.join(".eska")).unwrap();
    let lock = project.join(".eska/source-write.lock");
    symlink(foreign.join("sentinel"), &lock).unwrap();
    assert_eq!(
        acquire(&project).unwrap_err().code,
        "source_edit_unsafe_path"
    );
    assert_eq!(fs::read(foreign.join("sentinel")).unwrap(), b"untouched");
    fs::remove_file(&lock).unwrap();
    fs::create_dir(&lock).unwrap();
    assert_eq!(
        acquire(&project).unwrap_err().code,
        "source_edit_unsafe_path"
    );
}
