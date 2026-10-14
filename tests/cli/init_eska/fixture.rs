//! Existing Queries inputs, private companions and byte snapshots all belong to one playground claim.

pub(super) use super::super::new_eska::fixture::Fixture;
use eska::project_core::{
    ProjectName, ProjectType,
    creation::{self, ImportedProject, NewProjectOptions},
    native_version::ProjectVersion,
};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    process::Output,
};

pub(super) type Snapshot = (BTreeMap<PathBuf, Vec<u8>>, BTreeSet<PathBuf>);

/// The existing factory generates valid authoring; initialization tests never fabricate project controls.
pub(super) fn seed(fixture: &Fixture, kind: ProjectType, version: &str) -> ImportedProject {
    creation::new_project(
        &fixture.0.0.join("seed"),
        ProjectName::parse("seed".into()).unwrap(),
        &NewProjectOptions::new(kind, ProjectVersion::parse(version).unwrap())
            .with_metadata_name("Объект".into())
            .unwrap(),
    )
    .unwrap()
}

/// Source-only copying models unmanaged input without recreating support or ancestry from a working tree.
pub(super) fn unmanaged(
    fixture: &Fixture,
    seed: &ImportedProject,
    name: &str,
    source: &str,
) -> PathBuf {
    let root = fixture.0.0.join(name);
    let destination = root.join(source);
    let (files, directories) = snapshot(seed.project.source());
    fs::create_dir_all(&destination).unwrap();
    for directory in directories {
        fs::create_dir_all(destination.join(directory)).unwrap();
    }
    for (path, bytes) in files {
        fs::write(destination.join(path), bytes).unwrap();
    }
    fs::write(root.join("user.bin"), [0, 255, 13, 10]).unwrap();
    fs::write(root.join(".gitignore"), "# User ignore\r\nprivate/\r\n").unwrap();
    root
}

/// Exact files and empty directories expose accidental rewriting or cleanup of old inputs.
pub(super) fn snapshot(root: &Path) -> Snapshot {
    let mut files = BTreeMap::new();
    let mut directories = BTreeSet::new();
    let mut pending = vec![PathBuf::new()];
    while let Some(relative) = pending.pop() {
        for entry in fs::read_dir(root.join(&relative)).unwrap() {
            let entry = entry.unwrap();
            let path = relative.join(entry.file_name());
            if entry.file_type().unwrap().is_dir() {
                directories.insert(path.clone());
                pending.push(path);
            } else {
                files.insert(path, fs::read(entry.path()).unwrap());
            }
        }
    }
    (files, directories)
}

/// Parent output contains only its final complete result, never an intermediate direct-tool success.
pub(super) fn success(output: &Output) -> Value {
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert!(output.stderr.is_empty(), "{output:?}");
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["schema_version"], 1);
    assert_eq!(report["tool"], "eska");
    assert_eq!(report["command"], "init");
    assert_eq!(report["status"], "success");
    report["data"].clone()
}
