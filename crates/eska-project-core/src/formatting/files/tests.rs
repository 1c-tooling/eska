//! Publication tests assert preservation of user edits and all-file preflight under real OS source leases.

use super::*;
use crate::test_support::TestDir;

/// Own canonical sources so conflict checks address the same identities as a real CLI invocation.
fn plan(directory: &TestDir, name: &str) -> WritePlan {
    let path = directory.0.join(name);
    fs::write(&path, "A=1;").unwrap();
    WritePlan {
        path,
        before: "A=1;".into(),
        after: "A = 1;\n".into(),
    }
}

/// A conflict in a later file preserves the earlier source and the intervening user edit.
#[test]
fn stale_plan_is_rejected_before_any_publication() {
    let directory = TestDir::new();
    let first = plan(&directory, "A.bsl");
    let second = plan(&directory, "B.bsl");
    fs::write(&second.path, "user edit").unwrap();
    let mut context = Context::open(&directory.0, &[], false).unwrap();
    let result = publish(&mut context, &[first, second], &AtomicBool::new(false));
    assert_eq!(result.error.unwrap().code, "source_conflict");
    assert_eq!(result.states, [State::NotApplied, State::NotApplied]);
    assert_eq!(
        fs::read_to_string(directory.0.join("A.bsl")).unwrap(),
        "A=1;"
    );
    assert_eq!(
        fs::read_to_string(directory.0.join("B.bsl")).unwrap(),
        "user edit"
    );
}

/// Partial OS acquisition releases its earlier handles, while a competing source stays busy until drop.
#[test]
fn shared_lease_failure_cannot_leave_a_file_locked() {
    let directory = TestDir::new();
    let first = plan(&directory, "A.bsl");
    let second = plan(&directory, "B.bsl");
    let owner = SourceLease::acquire(&directory.0, std::slice::from_ref(&second.path)).unwrap();
    assert!(
        SourceLease::acquire(&directory.0, &[first.path.clone(), second.path.clone()]).is_err()
    );
    SourceLease::check(&directory.0, &first.path).unwrap();
    let mut context = Context::open(&directory.0, &[], false).unwrap();
    let result = publish(&mut context, &[first, second], &AtomicBool::new(false));
    assert_eq!(result.error.unwrap().code, "source_busy");
    assert_eq!(result.states, [State::NotApplied, State::NotApplied]);
    drop(owner);
    SourceLease::check(&directory.0, &directory.0.join("B.bsl")).unwrap();
}

/// A successful batch retains source permissions and releases every shared write capability.
#[test]
fn atomic_replacements_release_their_owned_leases() {
    let directory = TestDir::new();
    let first = plan(&directory, "A.bsl");
    let second = plan(&directory, "B.bsl");
    let permissions = fs::metadata(&first.path).unwrap().permissions();
    let mut context = Context::open(&directory.0, &[], false).unwrap();
    let result = publish(&mut context, &[first, second], &AtomicBool::new(false));
    assert!(result.error.is_none());
    assert_eq!(result.states, [State::Applied, State::Applied]);
    for name in ["A.bsl", "B.bsl"] {
        let path = directory.0.join(name);
        assert_eq!(fs::read_to_string(&path).unwrap(), "A = 1;\n");
        SourceLease::check(&directory.0, &path).unwrap();
    }
    assert_eq!(
        fs::metadata(directory.0.join("A.bsl"))
            .unwrap()
            .permissions(),
        permissions
    );
}

/// Cancellation before preflight has no filesystem side effects, including coordination directories.
#[test]
fn cancellation_before_preflight_does_not_write_sources_or_lock_storage() {
    let directory = TestDir::new();
    let source = plan(&directory, "A.bsl");
    let mut context = Context::open(&directory.0, &[], false).unwrap();
    let result = publish(&mut context, &[source], &AtomicBool::new(true));
    assert_eq!(result.error.unwrap().code, "cancelled");
    assert!(!directory.0.join(".eska").exists());
    assert_eq!(
        fs::read_to_string(directory.0.join("A.bsl")).unwrap(),
        "A=1;"
    );
}

/// A configuration change during formatting invalidates the complete plan before publication.
#[test]
fn changed_style_manifest_rejects_an_obsolete_plan() {
    let directory = TestDir::new();
    fs::create_dir(directory.0.join("src")).unwrap();
    let manifest = directory.0.join("eska.toml");
    fs::write(&manifest, "[project]\ntype='configuration'\n").unwrap();
    let source = plan(&directory, "src/A.bsl");
    let mut context = Context::open(&directory.0, &[], false).unwrap();
    fs::write(
        &manifest,
        "[project]\ntype='configuration'\n[format.bsl]\ncolumn_limit=80\n",
    )
    .unwrap();
    let result = publish(&mut context, &[source], &AtomicBool::new(false));
    assert_eq!(result.error.unwrap().code, "config_conflict");
    assert_eq!(
        fs::read_to_string(directory.0.join("src/A.bsl")).unwrap(),
        "A=1;"
    );
}
