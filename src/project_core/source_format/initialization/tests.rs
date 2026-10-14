//! Attachment checks include unchanged user evidence, stale plans and partial-publication rollback.

use sha2::Digest;
use std::{collections::BTreeMap, fs};

use crate::{
    project_core::{
        ProjectName, ProjectType,
        creation::{NewProjectOptions, new_project},
        discovery::discover,
        metadata_model::ProjectScope,
        native_version::ProjectVersion,
    },
    test_support::TestDir,
};

use super::*;
use crate::project_core::source_format::{
    ImportAncestor, capture_import_ancestor, open_initial_project_import_ancestor,
    prepare_project_source_import,
};

const TYPES: [ProjectType; 4] = [
    ProjectType::Configuration,
    ProjectType::Extension,
    ProjectType::Processing,
    ProjectType::Report,
];

/// Produce independently owned complete sources; subsequent attachment operates on an unmanaged root.
fn fixture(test: &TestDir, kind: ProjectType) -> crate::project_core::creation::ImportedProject {
    new_project(
        &test.0.join("project"),
        ProjectName::parse("demo".into()).unwrap(),
        &NewProjectOptions::new(kind, ProjectVersion::parse("1.2.3.4").unwrap()),
    )
    .unwrap()
}

/// Compare exact user/service file content without relying on derivative native revisions.
fn bytes(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    let mut result = BTreeMap::new();
    let mut pending = vec![root.to_owned()];
    while let Some(parent) = pending.pop() {
        for entry in fs::read_dir(parent).unwrap() {
            let entry = entry.unwrap();
            if entry.file_type().unwrap().is_dir() {
                pending.push(entry.path());
            } else {
                result.insert(
                    entry.path().strip_prefix(root).unwrap().to_owned(),
                    fs::read(entry.path()).unwrap(),
                );
            }
        }
    }
    result
}

/// Reuse a portable label while testing the source format's own complete attachment boundary.
fn plan(root: &Path, pin: Option<&AcceptedRef>) -> Result<ProjectInitializationPlan, Error> {
    prepare_project_initialization(
        root,
        SourcePath::parse("src").unwrap(),
        ProjectName::parse("attached".into()).unwrap(),
        None,
        pin,
    )
}

/// Explicit unmanaged-source attachment creates two candidate authorities without touching user files/Git.
#[test]
fn all_profiles_attach_raw_sources_and_preserve_existing_user_state() {
    for kind in TYPES {
        let test = TestDir::new();
        let created = fixture(&test, kind);
        let root = created.project.root();
        fs::remove_file(root.join("eska.toml")).unwrap();
        fs::remove_dir_all(root.join(".eska")).unwrap();
        fs::write(
            root.join(".gitattributes"),
            b"# User policy\r\n*.txt text\r\n",
        )
        .unwrap();
        fs::write(root.join(".gitignore"), b"# User ignore\r\n/private/\r\n").unwrap();
        fs::create_dir(root.join(".git")).unwrap();
        fs::write(root.join(".git/HEAD"), b"user repository bytes\r\n").unwrap();
        fs::write(root.join("user-note.txt"), b"untracked user text\r\n").unwrap();
        let source = bytes(created.project.source());
        let prepared = plan(root, None).unwrap();
        let initialized = prepared.publish().unwrap();
        prepared
            .verify_published(
                &initialized.accepted,
                initialized.initial_ancestor.as_ref().unwrap(),
                &initialized.manifest_sha256,
            )
            .unwrap();
        assert_eq!(initialized.project.configuration().project_type(), kind);
        assert_eq!(discover(root).unwrap(), initialized.project);
        assert_eq!(
            initialized
                .project
                .configuration()
                .version_scheme()
                .unwrap()
                .as_str(),
            "x.y.z.build"
        );
        assert_eq!(source, bytes(initialized.project.source()));
        assert_eq!(
            fs::read(root.join(".git/HEAD")).unwrap(),
            b"user repository bytes\r\n"
        );
        assert_eq!(
            fs::read(root.join(".gitignore")).unwrap(),
            b"# User ignore\r\n/private/\r\n"
        );
        assert_eq!(
            fs::read(root.join("user-note.txt")).unwrap(),
            b"untracked user text\r\n"
        );
        assert!(
            fs::read(root.join(".gitattributes"))
                .unwrap()
                .starts_with(b"# User policy\r\n*.txt text\r\n")
        );
        let reader = SourceReader::open(initialized.project.source()).unwrap();
        AcceptedBase::open(&root.join(ACCEPTED), &initialized.accepted)
            .unwrap()
            .validate_source_root(&reader)
            .unwrap();
        let ancestor = initialized.initial_ancestor.unwrap();
        assert_ne!(ancestor.sha256(), initialized.accepted.sha256);
        open_initial_project_import_ancestor(&initialized.project, &ancestor).unwrap();
        assert_eq!(
            initialized.manifest_sha256,
            files::hex(&sha2::Sha256::digest(
                fs::read(root.join("eska.toml")).unwrap()
            ))
        );
    }
}

/// Local module edits never advance an existing accepted pin or replace raw initial/published ancestry.
#[test]
fn initial_and_published_history_are_preserved_under_independent_support_pin() {
    for published in [false, true] {
        for kind in TYPES {
            let test = TestDir::new();
            let created = fixture(&test, kind);
            if published {
                let ancestor = open_initial_project_import_ancestor(
                    &created.project,
                    created.initial_ancestor.as_ref().unwrap(),
                )
                .unwrap();
                let reference =
                    capture_import_ancestor(created.project.source(), &test.0.join("incoming"))
                        .unwrap();
                let incoming = ImportAncestor::open(&test.0.join("incoming"), &reference).unwrap();
                prepare_project_source_import(
                    &created.project,
                    &ProjectScope::Standalone,
                    &created.accepted,
                    &ancestor,
                    &incoming,
                )
                .unwrap()
                .publish()
                .unwrap();
            }
            let root = created.project.root();
            let accepted = bytes(&root.join(ACCEPTED));
            let history = bytes(&root.join(PROJECT_SOURCE_IMPORT_DIRECTORY));
            if matches!(kind, ProjectType::Processing | ProjectType::Report) {
                let module = created.project.source().join("ObjectModule.bsl");
                let mut body = fs::read(&module).unwrap();
                body.extend_from_slice(b"// Saved user comment.\r\n");
                fs::write(module, body).unwrap();
            }
            fs::remove_file(root.join("eska.toml")).unwrap();
            assert_eq!(
                plan(root, None).err().unwrap().code,
                "accepted_base_pin_required"
            );
            let source = bytes(created.project.source());
            let prepared = plan(root, Some(&created.accepted)).unwrap();
            let initialized = prepared.publish().unwrap();
            prepared
                .verify_published(
                    &initialized.accepted,
                    initialized.initial_ancestor.as_ref().unwrap(),
                    &initialized.manifest_sha256,
                )
                .unwrap();
            assert_eq!(initialized.accepted, created.accepted);
            assert_eq!(accepted, bytes(&root.join(ACCEPTED)));
            assert_eq!(history, bytes(&root.join(PROJECT_SOURCE_IMPORT_DIRECTORY)));
            assert_eq!(source, bytes(initialized.project.source()));
            let ancestor = initialized.initial_ancestor.as_ref().unwrap();
            super::super::project_import_ancestor(
                &initialized.project,
                &ProjectScope::Standalone,
                ancestor,
            )
            .unwrap();
        }
    }
}

/// Missing history receives a candidate of actual edited source while pinned support stays unchanged.
#[test]
fn existing_support_without_history_creates_only_unapproved_current_source_ancestry() {
    let test = TestDir::new();
    let created = fixture(&test, ProjectType::Report);
    let root = created.project.root();
    fs::remove_file(root.join("eska.toml")).unwrap();
    fs::remove_dir_all(root.join(PROJECT_SOURCE_IMPORT_DIRECTORY)).unwrap();
    let module = created.project.source().join("ObjectModule.bsl");
    let mut body = fs::read(&module).unwrap();
    body.extend_from_slice(b"// Independent saved user edit.\r\n");
    fs::write(module, body).unwrap();
    let source = Inventory::read(created.project.source()).unwrap();
    let support = Inventory::read(&root.join(ACCEPTED)).unwrap();
    let prepared = plan(root, Some(&created.accepted)).unwrap();
    assert!(!prepared.accepted_base_created());
    assert!(!prepared.ancestry_preserved());
    let initialized = prepared.publish().unwrap();
    assert_eq!(initialized.accepted, created.accepted);
    assert_eq!(Inventory::read(&root.join(ACCEPTED)).unwrap(), support);
    assert_eq!(
        Inventory::read(initialized.project.source()).unwrap(),
        source
    );
    let ancestor = initialized.initial_ancestor.as_ref().unwrap();
    assert_eq!(
        open_initial_project_import_ancestor(&initialized.project, ancestor)
            .unwrap()
            .revision(),
        source.revision,
    );
    prepared
        .verify_published(
            &initialized.accepted,
            ancestor,
            &initialized.manifest_sha256,
        )
        .unwrap();
}

/// Stale source, historical evidence or settings refuse publication before attachment writes.
#[test]
fn stale_inputs_and_concurrent_manifest_are_retained_without_new_publication() {
    for change in ["source", "history", "manifest"] {
        let test = TestDir::new();
        let created = fixture(&test, ProjectType::Report);
        let root = created.project.root();
        fs::remove_file(root.join("eska.toml")).unwrap();
        let prepared = plan(root, Some(&created.accepted)).unwrap();
        let path = match change {
            "source" => created.project.source().join("ObjectModule.bsl"),
            "history" => root
                .join(PROJECT_SOURCE_IMPORT_DIRECTORY)
                .join("state.json"),
            _ => root.join("eska.toml"),
        };
        fs::write(&path, b"Concurrent user edit").unwrap();
        let before = bytes(root);
        assert!(prepared.publish().is_err());
        assert_eq!(before, bytes(root));
    }
}

/// A late attribute conflict removes only new evidence and retains unrelated service and policy bytes.
#[test]
fn attribute_conflict_rolls_back_new_service_without_deleting_foreign_entries() {
    let test = TestDir::new();
    let created = fixture(&test, ProjectType::Report);
    let root = created.project.root();
    fs::remove_file(root.join("eska.toml")).unwrap();
    fs::remove_dir_all(root.join(".eska")).unwrap();
    fs::remove_file(root.join(".gitignore")).unwrap();
    fs::write(root.join(".gitattributes"), "# old\n").unwrap();
    fs::create_dir(root.join(".eska")).unwrap();
    fs::write(root.join(".eska/sentinel"), "user data").unwrap();
    let prepared = plan(root, None).unwrap();
    fs::write(root.join(".gitattributes"), "# new user policy\n").unwrap();
    let before = bytes(root);
    assert!(prepared.publish().is_err());
    assert_eq!(before, bytes(root));
    assert!(!root.join(".eska/accepted").exists());
    assert!(!root.join(PROJECT_SOURCE_IMPORT_DIRECTORY).exists());
}

/// Missing pins, malformed controls and symlinked input do not claim service data or settings.
#[test]
fn wrong_support_and_unsafe_service_are_explicit_read_only_refusals() {
    let test = TestDir::new();
    let created = fixture(&test, ProjectType::Report);
    let root = created.project.root();
    fs::remove_file(root.join("eska.toml")).unwrap();
    let wrong = AcceptedRef {
        sha256: "0".repeat(64),
    };
    let before = bytes(root);
    assert!(plan(root, Some(&wrong)).is_err());
    assert_eq!(before, bytes(root));
    fs::write(
        root.join(PROJECT_SOURCE_IMPORT_DIRECTORY)
            .join("state.json"),
        "{}",
    )
    .unwrap();
    let before = bytes(root);
    assert!(plan(root, Some(&created.accepted)).is_err());
    assert_eq!(before, bytes(root));
}

/// Parent onboarding cannot accept settings, source or evidence changed after the child acknowledgment.
#[test]
fn published_verification_rejects_changed_source_support_history_and_settings() {
    for change in ["source", "support", "history", "settings"] {
        let test = TestDir::new();
        let created = fixture(&test, ProjectType::Report);
        let root = created.project.root();
        fs::remove_file(root.join("eska.toml")).unwrap();
        let prepared = plan(root, Some(&created.accepted)).unwrap();
        let initialized = prepared.publish().unwrap();
        let ancestor = initialized.initial_ancestor.as_ref().unwrap();
        prepared
            .verify_published(
                &initialized.accepted,
                ancestor,
                &initialized.manifest_sha256,
            )
            .unwrap();
        let path = match change {
            "source" => root.join("src/ObjectModule.bsl"),
            "support" => root.join(ACCEPTED).join("accepted.json"),
            "history" => root
                .join(PROJECT_SOURCE_IMPORT_DIRECTORY)
                .join("state.json"),
            _ => root.join("eska.toml"),
        };
        fs::write(path, b"intervening caller bytes").unwrap();
        let before = bytes(root);
        assert!(
            prepared
                .verify_published(
                    &initialized.accepted,
                    ancestor,
                    &initialized.manifest_sha256
                )
                .is_err()
        );
        assert_eq!(before, bytes(root));
    }
}

/// An explicit nondefault source is retained and parent/sibling aliases cannot redirect initialization.
#[test]
fn explicit_source_directory_and_alias_refusals_keep_user_inputs() {
    let test = TestDir::new();
    let created = fixture(&test, ProjectType::Report);
    let root = created.project.root();
    fs::remove_file(root.join("eska.toml")).unwrap();
    fs::remove_dir_all(root.join(".eska")).unwrap();
    fs::rename(root.join("src"), root.join("source folder")).unwrap();
    let source = SourcePath::parse("source folder").unwrap();
    let prepared = prepare_project_initialization(
        root,
        source,
        ProjectName::parse("attached".into()).unwrap(),
        None,
        None,
    )
    .unwrap();
    let initialized = prepared.publish().unwrap();
    assert_eq!(initialized.project.source(), root.join("source folder"));
    prepared
        .verify_published(
            &initialized.accepted,
            initialized.initial_ancestor.as_ref().unwrap(),
            &initialized.manifest_sha256,
        )
        .unwrap();
    #[cfg(unix)]
    {
        let alias_test = TestDir::new();
        let aliased = fixture(&alias_test, ProjectType::Report);
        let alias_root = aliased.project.root();
        fs::remove_file(alias_root.join("eska.toml")).unwrap();
        fs::rename(
            alias_root.join(".eska"),
            alias_test.0.join("external-service"),
        )
        .unwrap();
        std::os::unix::fs::symlink(
            alias_test.0.join("external-service"),
            alias_root.join(".eska"),
        )
        .unwrap();
        assert!(plan(alias_root, Some(&aliased.accepted)).is_err());
        assert!(!alias_root.join("eska.toml").exists());
        assert!(alias_test.0.join("external-service/accepted").is_dir());
    }
}
