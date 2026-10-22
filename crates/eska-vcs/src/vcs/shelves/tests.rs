//! A duplicated descriptor models the lock lifetime inherited by a concurrent Unix fork.

use super::*;

/// The session owns the mutation lock even if another descriptor still refers to its open file.
#[test]
fn session_drop_releases_lock_before_a_duplicated_descriptor_closes() {
    let root = crate::test_support::TestDir::new();
    gix::init(&root.0).unwrap();
    let repository = Repository::discover(&root.0).unwrap();
    let session = Session::acquire(&repository).unwrap();
    let duplicated = session.lock.try_clone().unwrap();
    drop(session);
    let next = Session::acquire(&repository).expect("the completed session released its lock");
    assert!(duplicated.metadata().unwrap().is_file());
    drop(next);
    drop(duplicated);
}
