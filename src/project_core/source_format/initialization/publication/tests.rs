//! Managed phase boundaries exercise ownership without timing races or production fault injection.

use super::*;
use crate::{
    project_core::{
        ProjectName, ProjectType,
        creation::{NewProjectOptions, new_project},
        native_version::ProjectVersion,
        source_format::{
            SourcePath, accept_source,
            files::Inventory,
            layout::{
                self,
                migration::publication::{self as storage, View},
            },
            synchronization::stage_project_import_ancestor,
        },
    },
    test_support::TestDir,
};
use std::{collections::BTreeMap, path::PathBuf};

/// Every test begins with its own verified source and retains only authoring and existing user policy.
fn unmanaged(test: &TestDir) -> ImportedProject {
    let created = new_project(
        &test.0.join("project"),
        ProjectName::parse("demo".into()).unwrap(),
        &NewProjectOptions::new(ProjectType::Report, ProjectVersion::parse("0.1.0").unwrap()),
    )
    .unwrap();
    fs::remove_file(created.project.root().join("eska.toml")).unwrap();
    fs::remove_dir_all(created.project.root().join(".eska")).unwrap();
    created
}

/// Prepare the real public operation while retaining the private phase handles for controlled edits.
fn prepared(created: &ImportedProject) -> ProjectInitializationPlan {
    super::super::super::prepare_project_initialization(
        created.project.root(),
        SourcePath::parse("src").unwrap(),
        ProjectName::parse("attached".into()).unwrap(),
        None,
        None,
    )
    .unwrap()
}

/// Ownership has no authority until the production publisher claims an absent public store.
fn owned(stores: Vec<CandidateStore>) -> Owned {
    Owned {
        service_parent: false,
        directories: stores,
        ignore: None,
        attributes: false,
    }
}

/// Locate only this test's exclusively claimed stage without exposing production paths as an API.
fn stage_path(root: &Path) -> PathBuf {
    fs::read_dir(root)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .find(|path| {
            path.file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .starts_with(".eska-init-")
        })
        .unwrap()
}

/// Compare all ordinary source/store bytes, including foreign entries that cleanup must retain.
fn snapshot(root: &Path) -> Inventory {
    Inventory::read(root).unwrap()
}

/// An ancestry producer failure cleans private state while simultaneous public user stores remain intact.
#[test]
fn producer_failure_never_cleans_public_candidate_names() {
    let test = TestDir::new();
    let created = unmanaged(&test);
    let plan = prepared(&created);
    let stage = Stage::claim(&plan).unwrap();
    let private = stage_path(&plan.root);
    accept_source(plan.project.source(), &private.join("accepted"), &[]).unwrap();
    fs::create_dir_all(plan.root.join(ACCEPTED)).unwrap();
    fs::create_dir_all(plan.root.join(PROJECT_SOURCE_IMPORT_DIRECTORY)).unwrap();
    fs::write(
        plan.root.join(ACCEPTED).join("user.txt"),
        b"independent accepted bytes",
    )
    .unwrap();
    fs::write(
        plan.root
            .join(PROJECT_SOURCE_IMPORT_DIRECTORY)
            .join("user.txt"),
        b"independent history bytes",
    )
    .unwrap();
    let public = snapshot(&plan.root.join(".eska"));
    fs::rename(
        plan.project.source().join("ObjectModule.bsl"),
        plan.project.source().join("SavedUserModule.bsl"),
    )
    .unwrap();
    let source = snapshot(plan.project.source());
    assert!(stage_project_import_ancestor(&plan.project, &private.join("source-import")).is_err());
    assert!(!private.join("source-import").exists());
    assert!(private.join("accepted").is_dir());
    stage.remove().unwrap();
    assert_eq!(snapshot(&plan.root.join(".eska")), public);
    assert_eq!(snapshot(plan.project.source()), source);
    assert!(!plan.root.join("eska.toml").exists());
    assert!(!private.exists());
}

/// A producer's immutable fingerprint cannot be replaced by a later private-stage inventory.
#[test]
fn changed_stage_after_production_refuses_publication_without_claiming_store() {
    let test = TestDir::new();
    let created = unmanaged(&test);
    let plan = prepared(&created);
    let stage = Stage::claim(&plan).unwrap();
    let candidates = stage.prepare(&plan).unwrap();
    let mut owned = owned(candidates.stores);
    fs::write(
        stage_path(&plan.root).join("accepted/accepted.json"),
        b"changed producer result",
    )
    .unwrap();
    let error = plan.publish_candidates(&mut owned).unwrap_err();
    assert_eq!(error.code, "source_input_changed");
    assert!(owned.directories.iter().all(|store| !store.claimed));
    assert_eq!(
        owned.rollback(&plan, error).err().unwrap().code,
        "source_input_changed"
    );
    stage.remove().unwrap();
    assert!(!plan.root.join(".eska").exists());
    assert!(!plan.root.join("eska.toml").exists());
}

/// Cleanup cannot follow an existing root replaced by an alias into another user's directory.
#[cfg(unix)]
#[test]
fn private_stage_cleanup_refuses_a_redirected_project_root() {
    let test = TestDir::new();
    let created = unmanaged(&test);
    let plan = prepared(&created);
    let stage = Stage::claim(&plan).unwrap();
    let private = stage_path(&plan.root);
    let original = test.0.join("original-project");
    let foreign = test.0.join("foreign-root");
    fs::rename(&plan.root, &original).unwrap();
    fs::create_dir(&foreign).unwrap();
    let redirected = foreign.join(private.file_name().unwrap());
    fs::create_dir(&redirected).unwrap();
    fs::write(redirected.join("user.txt"), b"independent foreign bytes").unwrap();
    std::os::unix::fs::symlink(&foreign, &plan.root).unwrap();
    let before = snapshot(&foreign);
    let error = stage.remove().unwrap_err();
    assert_eq!(error.code, "source_cleanup_failed");
    assert_eq!(snapshot(&foreign), before);
    assert!(original.join(private.file_name().unwrap()).is_dir());
    assert!(original.join("src/ObjectModule.bsl").is_file());
}

/// Foreign entries and edited bytes added after production never become output ownership authority.
#[test]
fn late_public_store_edits_are_retained_and_block_manifest_publication() {
    for change in ["edited", "deleted", "foreign", "directory"] {
        let test = TestDir::new();
        let created = unmanaged(&test);
        let plan = prepared(&created);
        let source = snapshot(plan.project.source());
        let stage = Stage::claim(&plan).unwrap();
        let candidates = stage.prepare(&plan).unwrap();
        let mut owned = owned(candidates.stores);
        plan.publish_candidates(&mut owned).unwrap();
        let directory = plan.root.join(ACCEPTED);
        match change {
            "deleted" => fs::remove_file(directory.join("accepted.json")).unwrap(),
            "edited" => {
                fs::write(directory.join("accepted.json"), b"intervening user bytes").unwrap();
            }
            "foreign" => {
                fs::write(directory.join("foreign.txt"), b"independent user entry").unwrap();
            }
            _ => fs::create_dir(directory.join("foreign-empty")).unwrap(),
        }
        let intervening = snapshot(&directory);
        let error = plan
            .finish_controls(&mut owned, candidates.accepted, candidates.ancestor)
            .err()
            .unwrap();
        assert_eq!(error.code, "source_input_changed");
        let error = owned.rollback(&plan, error).err().unwrap();
        assert_eq!(error.code, "source_cleanup_failed");
        assert_eq!(snapshot(&directory), intervening);
        assert!(!plan.root.join(PROJECT_SOURCE_IMPORT_DIRECTORY).exists());
        assert!(!plan.root.join("eska.toml").exists());
        assert_eq!(snapshot(plan.project.source()), source);
        stage.remove().unwrap();
    }
}

/// The publisher's exclusive claim flag does not mistake concurrent identical output for its creation.
#[test]
fn an_identical_public_store_is_not_claimed_or_deleted() {
    let test = TestDir::new();
    let created = unmanaged(&test);
    let plan = prepared(&created);
    let stage = Stage::claim(&plan).unwrap();
    let candidates = stage.prepare(&plan).unwrap();
    let mut owned = owned(candidates.stores);
    fs::create_dir(plan.root.join(".eska")).unwrap();
    let input = stage_path(&plan.root).join("accepted");
    let inventory = &owned.directories[0].inventory;
    let generated = BTreeMap::new();
    storage::publish(
        View {
            root: &input,
            input: inventory,
            output: inventory,
            text: &generated,
            ready: "accepted.json",
            renamed: None,
        },
        &plan.root.join(ACCEPTED),
    )
    .unwrap();
    let before = snapshot(&plan.root.join(ACCEPTED));
    let error = owned.directories[0].publish(&plan).unwrap_err();
    assert_eq!(error.code, "source_destination_exists");
    assert!(!owned.directories[0].claimed);
    assert_eq!(
        owned.rollback(&plan, error).err().unwrap().code,
        "source_destination_exists"
    );
    assert_eq!(snapshot(&plan.root.join(ACCEPTED)), before);
    stage.remove().unwrap();
}

/// Partial rollback removes exact produced subsets but retains any foreign receipt/payload/directory.
#[test]
fn interrupted_publication_cleanup_requires_exact_generated_bytes() {
    for change in [
        "empty",
        "subset",
        "temporary",
        "foreign-temporary",
        "receipt",
        "file",
        "directory",
    ] {
        let test = TestDir::new();
        let created = unmanaged(&test);
        let plan = prepared(&created);
        let source = snapshot(plan.project.source());
        let stage = Stage::claim(&plan).unwrap();
        let candidates = stage.prepare(&plan).unwrap();
        let mut owned = owned(candidates.stores);
        fs::create_dir(plan.root.join(".eska")).unwrap();
        owned.service_parent = true;
        let directory = plan.root.join(ACCEPTED);
        let input = stage_path(&plan.root).join("accepted");
        let inventory = &owned.directories[0].inventory;
        let generated = BTreeMap::new();
        let view = View {
            root: &input,
            input: inventory,
            output: inventory,
            text: &generated,
            ready: "accepted.json",
            renamed: None,
        };
        if change == "empty" {
            fs::create_dir(&directory).unwrap();
        } else {
            storage::interrupt(view, &directory);
            fs::create_dir(directory.join("blobs")).unwrap();
            let entry = inventory
                .files
                .values()
                .find(|entry| entry.path.as_str().starts_with("blobs/"))
                .unwrap();
            files::copy_to(&input, &directory.join(entry.path.as_path()), entry).unwrap();
            match change {
                "temporary" => {
                    files::copy_to(&input, &directory.join(layout::PENDING_WRITE), entry).unwrap();
                }
                "foreign-temporary" => fs::write(
                    directory.join(layout::PENDING_WRITE),
                    b"foreign temporary bytes",
                )
                .unwrap(),
                "receipt" => fs::write(
                    directory.join(layout::PENDING_RECEIPT),
                    b"foreign receipt bytes",
                )
                .unwrap(),
                "file" => fs::write(
                    directory.join(entry.path.as_path()),
                    b"intervening payload edit",
                )
                .unwrap(),
                "directory" => fs::create_dir(directory.join("foreign-directory")).unwrap(),
                _ => (),
            }
        }
        owned.directories[0].claimed = true;
        let before = snapshot(&directory);
        let error = owned
            .rollback(&plan, Error::root("source_io_error"))
            .err()
            .unwrap();
        if matches!(change, "empty" | "subset" | "temporary") {
            assert_eq!(error.code, "source_io_error", "{change}");
            assert!(!plan.root.join(".eska").exists(), "{change}");
        } else {
            assert_eq!(error.code, "source_cleanup_failed", "{change}");
            assert_eq!(snapshot(&directory), before, "{change}");
        }
        assert_eq!(snapshot(plan.project.source()), source);
        assert!(!plan.root.join("eska.toml").exists());
        stage.remove().unwrap();
    }
}
