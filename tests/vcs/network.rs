use eska::vcs::{
    network::{self, CapabilityGap, FetchBackend, FetchError},
    repository::Repository,
};

use super::support::{commit, git, repository};
use crate::support::TestDir;

/// Build a local bare remote and attach it to the fixture repository.
fn attach_local_remote(root: &std::path::Path) -> TestDir {
    let remote = TestDir::new();
    git(
        &remote.0,
        &["init", "--bare", "--initial-branch=main", "--template="],
    );
    git(
        root,
        &[
            "remote",
            "add",
            "origin",
            remote.0.to_str().expect("UTF-8 remote path"),
        ],
    );
    remote
}

#[test]
fn configured_transport_fetches_through_gix() {
    let root = repository();
    commit(&root.0, "base");
    let remote = attach_local_remote(&root.0);
    git(&root.0, &["push", "origin", "main"]);
    let repository = Repository::discover(&root.0).unwrap();

    let outcome = network::fetch(&repository, "origin").expect("gix fetch");

    assert_eq!(outcome.backend, FetchBackend::Gix);
    drop(remote);
}

#[test]
fn ordinary_gix_failure_is_not_retried_with_system_git() {
    let root = repository();
    commit(&root.0, "base");
    let missing = root.0.join("missing.git");
    git(
        &root.0,
        &[
            "remote",
            "add",
            "origin",
            missing.to_str().expect("UTF-8 remote path"),
        ],
    );
    let repository = Repository::discover(&root.0).unwrap();

    let error = network::fetch(&repository, "origin").expect_err("missing remote");

    assert!(!matches!(error, FetchError::SystemGit { .. }));
}

#[test]
fn remote_helper_failure_retains_the_structured_fallback_reason() {
    let root = repository();
    commit(&root.0, "base");
    git(
        &root.0,
        &["remote", "add", "origin", "eska-test::repository"],
    );
    let repository = Repository::discover(&root.0).unwrap();

    let error = network::fetch(&repository, "origin").expect_err("missing remote helper");

    assert!(matches!(
        error,
        FetchError::SystemGit {
            reason: CapabilityGap::RemoteHelper { scheme },
            ..
        } if scheme == "eska-test"
    ));
}
