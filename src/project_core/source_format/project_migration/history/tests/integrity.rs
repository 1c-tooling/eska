//! Copied history must remain a valid raw snapshot of this independently identified project.

use super::*;

/// Malformed state and edited immutable bytes never become a migration-owned output.
#[test]
fn invalid_active_history_is_refused_before_output() {
    for corruption in ["state", "missing state", "blob"] {
        let test = TestDir::new();
        let imported = creation::import_query_project(
            &native(&test),
            &test.0.join("old"),
            ProjectName::parse("sample".into()).unwrap(),
            None,
            &[],
        )
        .unwrap();
        let service = imported
            .project
            .root()
            .join(PROJECT_SOURCE_IMPORT_DIRECTORY);
        match corruption {
            "state" => fs::write(service.join("state.json"), "{}").unwrap(),
            "missing state" => fs::remove_file(service.join("state.json")).unwrap(),
            "blob" => {
                let inventory = Inventory::read(&service.join("initial")).unwrap();
                let blob = inventory
                    .files
                    .keys()
                    .find(|path| path.as_str().starts_with("blobs/"))
                    .unwrap();
                fs::write(service.join("initial").join(blob.as_path()), b"edited").unwrap();
            }
            _ => unreachable!(),
        }
        let before = Inventory::read(imported.project.root()).unwrap();
        let destination = test.0.join("refused");
        assert!(
            prepare_query_project(
                &imported.project,
                &ProjectScope::Standalone,
                &imported.accepted,
                &destination,
            )
            .is_err()
        );
        assert!(!destination.exists());
        assert_eq!(Inventory::read(imported.project.root()).unwrap(), before);
    }
}

/// A self-consistent foreign raw store cannot borrow the independently approved root identity.
#[test]
fn valid_history_from_another_root_is_refused_before_output() {
    let test = TestDir::new();
    let imported = creation::import_query_project(
        &native(&test),
        &test.0.join("old"),
        ProjectName::parse("sample".into()).unwrap(),
        None,
        &[],
    )
    .unwrap();
    let other = test.0.join("other source");
    let initial = project_import_ancestor(
        &imported.project,
        &ProjectScope::Standalone,
        imported.initial_ancestor.as_ref().unwrap(),
    )
    .unwrap();
    initial.restore(&other).unwrap();
    let owner = other.join("configuration.yaml");
    fs::write(
        &owner,
        fs::read_to_string(&owner).unwrap().replace(
            "10000000-0000-4000-8000-000000000001",
            "10000000-0000-4000-8000-000000000009",
        ),
    )
    .unwrap();
    let manifest = other.join("source.json");
    fs::write(
        &manifest,
        fs::read_to_string(&manifest).unwrap().replace(
            "10000000-0000-4000-8000-000000000001",
            "10000000-0000-4000-8000-000000000009",
        ),
    )
    .unwrap();
    let service = imported
        .project
        .root()
        .join(PROJECT_SOURCE_IMPORT_DIRECTORY);
    fs::remove_dir_all(service.join("initial")).unwrap();
    let reference = capture_import_ancestor(&other, &service.join("initial")).unwrap();
    fs::write(
        service.join("state.json"),
        serde_json::json!({
            "schema_version":2,
            "origin":"initial",
            "source":"src",
            "ancestor":{"sha256":reference.sha256()}
        })
        .to_string(),
    )
    .unwrap();
    let destination = test.0.join("refused");
    assert_eq!(
        prepare_query_project(
            &imported.project,
            &ProjectScope::Standalone,
            &imported.accepted,
            &destination,
        )
        .unwrap_err()
        .code,
        "source_identity_mismatch"
    );
    assert!(!destination.exists());
}

/// Same native identity does not make a map 3 ancestor compatible with a map 5 authoring source.
#[test]
fn history_schema_mismatch_is_refused_without_repackaging_the_raw_ancestor() {
    let test = TestDir::new();
    let input = native(&test);
    let name = ProjectName::parse("sample".into()).unwrap();
    let modern =
        creation::import_query_project(&input, &test.0.join("modern"), name.clone(), None, &[])
            .unwrap();
    let legacy =
        creation::import_readable_project(&input, &test.0.join("legacy"), name, None, &[]).unwrap();
    let service = modern.project.root().join(PROJECT_SOURCE_IMPORT_DIRECTORY);
    fs::remove_dir_all(service.join("initial")).unwrap();
    let reference =
        capture_import_ancestor(legacy.project.source(), &service.join("initial")).unwrap();
    fs::write(
        service.join("state.json"),
        serde_json::json!({
            "schema_version":2,
            "origin":"initial",
            "source":"src",
            "ancestor":{"sha256":reference.sha256()}
        })
        .to_string(),
    )
    .unwrap();
    let before = Inventory::read(&service).unwrap();
    let destination = test.0.join("refused");
    assert_eq!(
        prepare_query_project(
            &modern.project,
            &ProjectScope::Standalone,
            &modern.accepted,
            &destination,
        )
        .unwrap_err()
        .code,
        "source_import_profile_mismatch"
    );
    assert!(!destination.exists());
    assert_eq!(Inventory::read(&service).unwrap(), before);
}

/// A symlink inside an old raw archive cannot redirect the supposedly exact migration copy.
#[cfg(unix)]
#[test]
fn aliased_history_is_refused_before_output() {
    let test = TestDir::new();
    let imported = creation::import_query_project(
        &native(&test),
        &test.0.join("old"),
        ProjectName::parse("sample".into()).unwrap(),
        None,
        &[],
    )
    .unwrap();
    let service = imported
        .project
        .root()
        .join(PROJECT_SOURCE_IMPORT_DIRECTORY);
    std::os::unix::fs::symlink(imported.project.source(), service.join("alias")).unwrap();
    let destination = test.0.join("refused");
    assert_eq!(
        prepare_query_project(
            &imported.project,
            &ProjectScope::Standalone,
            &imported.accepted,
            &destination,
        )
        .unwrap_err()
        .code,
        "source_entry_unsupported"
    );
    assert!(!destination.exists());
}
