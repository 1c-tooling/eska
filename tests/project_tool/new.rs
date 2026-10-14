//! Fresh minimal creation exposes one authoring source and separate candidate pins without TTY.

use super::*;
use eska::project_core::{
    discovery::discover,
    metadata_model::ProjectScope,
    native_version,
    source_format::{
        AcceptedBase, AcceptedRef, ImportAncestorRef, PropertyName, PropertyValue, SourceReader,
        project_import_ancestor,
    },
};
use sha2::{Digest, Sha256};
use std::fmt::Write;

const TYPES: [&str; 4] = ["configuration", "extension", "processing", "report"];

/// Assert the complete locale-independent creation contract against the initially published file.
fn assert_new_protocol(data: &Value, kind: &str, root: &Path) {
    assert_eq!(
        data.as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        [
            "accepted_base",
            "accepted_base_sha256",
            "ancestor_sha256",
            "initial_version",
            "manifest_sha256",
            "metadata_name",
            "name",
            "root",
            "source",
            "source_format",
            "source_layout",
            "type",
            "version_scheme",
        ]
    );
    assert_eq!(data["type"], kind);
    assert_eq!(data["metadata_name"], "Project_2_project_label");
    assert_eq!(data["initial_version"], "0.1.0");
    assert_eq!(data["version_scheme"], "x.y.z");
    assert_eq!(data["source_format"], "eska");
    assert_eq!(data["source_layout"], "queries");
    assert_eq!(data["name"], "2-project-label");
    let digest = Sha256::digest(fs::read(root.join("eska.toml")).unwrap())
        .iter()
        .fold(String::with_capacity(64), |mut value, byte| {
            write!(value, "{byte:02x}").unwrap();
            value
        });
    assert_eq!(data["manifest_sha256"], digest);
    assert_eq!(data["root"], root.to_str().unwrap());
    assert_eq!(data["source"], root.join("src").to_str().unwrap());
}

/// Both locales publish the same new-command schema; fresh identities keep their own independent pins.
#[test]
fn new_minimal_profiles_are_inspectable_and_prepare_under_their_original_pins() {
    for kind in TYPES {
        for locale in ["ru", "en"] {
            let test = TestDir::new();
            let created = success(
                &test.0,
                locale,
                &[
                    "new",
                    "project",
                    "--name",
                    "2-project-label",
                    "--type",
                    kind,
                    "--format",
                    "json",
                ],
            );
            assert_eq!(created["command"], "new");
            let data = &created["data"];
            let root = test.0.join("project");
            assert_new_protocol(data, kind, &root);
            let pin = data["accepted_base_sha256"].as_str().unwrap();
            let ancestor = data["ancestor_sha256"].as_str().unwrap();
            assert_ne!(pin, ancestor);
            let project = discover(&root).unwrap();
            AcceptedBase::open(
                &root.join(".eska/accepted"),
                &AcceptedRef { sha256: pin.into() },
            )
            .unwrap();
            project_import_ancestor(
                &project,
                &ProjectScope::Standalone,
                &ImportAncestorRef::from_sha256(ancestor).unwrap(),
            )
            .unwrap();
            let reader = SourceReader::open(&root.join("src")).unwrap();
            if matches!(kind, "processing" | "report") {
                assert_eq!(
                    native_version::inspect(&project)
                        .unwrap()
                        .version
                        .to_string(),
                    "0.1.0"
                );
            } else {
                let owner = reader.read_owner(&reader.configuration().path).unwrap();
                assert_eq!(
                    owner
                        .source
                        .document
                        .descriptor
                        .object
                        .property(PropertyName::Version),
                    Some(&PropertyValue::Text("0.1.0".into()))
                );
            }
            assert_eq!(reader.owner_paths().count(), 1);
            assert_eq!(
                reader.configuration().declarations[0].name,
                "Project_2_project_label"
            );
            let inspected = success(&root, locale, &["inspect", "--format", "json"]);
            assert_eq!(inspected["data"]["type"], kind);
            let prepared = success(
                &root,
                locale,
                &["prepare", "--accepted-base-sha256", pin, "--format", "json"],
            );
            assert_eq!(prepared["data"]["baseline"], pin);
            assert_eq!(prepared["data"]["exclusions"], serde_json::json!([]));
            assert!(!root.join(".git").exists());
            assert!(!root.join("build").exists());
            assert!(!test.0.read_dir().unwrap().any(|entry| {
                entry
                    .unwrap()
                    .file_name()
                    .to_string_lossy()
                    .starts_with(".eska-new-")
            }));
        }
    }
}

/// Explicit native names and four-part widths stay exact while human output retains ordinary localization.
#[test]
fn new_explicit_names_versions_and_human_output_work_in_both_locales() {
    for (locale, phrase) in [("ru", "Проект создан"), ("en", "Project created")] {
        let test = TestDir::new();
        let created = success(
            &test.0,
            locale,
            &[
                "new",
                "json-project",
                "--name",
                "label",
                "--type",
                "report",
                "--metadata-name",
                "Новый_Отчет",
                "--initial-version",
                "01.02.003.0004",
                "--platform-version",
                "8.3.27.2325",
                "--format",
                "json",
            ],
        );
        assert_eq!(created["data"]["metadata_name"], "Новый_Отчет");
        assert_eq!(created["data"]["initial_version"], "01.02.003.0004");
        assert_eq!(created["data"]["version_scheme"], "x.y.z.build");
        let output = tool(
            &test.0,
            locale,
            &["new", "human-project", "--name", "label"],
        );
        assert_eq!(output.status.code(), Some(0), "{output:?}");
        assert!(output.stderr.is_empty());
        assert!(String::from_utf8_lossy(&output.stdout).contains(phrase));
        let reader = SourceReader::open(&test.0.join("human-project/src")).unwrap();
        assert_eq!(reader.project_type().unwrap().as_str(), "configuration");
    }
}

/// Invalid semantic selections fail without leaving staging, authority stores or a partial destination.
#[test]
fn new_invalid_names_versions_and_existing_destinations_are_locale_free_and_preserved() {
    for (option, value, code) in [
        ("--name", "Bad Label", "project_name_invalid"),
        (
            "--metadata-name",
            "Bad Name",
            "source_metadata_name_invalid",
        ),
        (
            "--metadata-name",
            "../Outside",
            "source_metadata_name_invalid",
        ),
        ("--metadata-name", "CON", "source_metadata_name_invalid"),
        ("--initial-version", "1.2", "source_initial_version_invalid"),
        (
            "--initial-version",
            "01.2.3",
            "source_initial_version_invalid",
        ),
        (
            "--initial-version",
            "1.2.3-beta",
            "source_initial_version_invalid",
        ),
    ] {
        let test = TestDir::new();
        let mut previous = None;
        for locale in ["ru", "en"] {
            let mut args = vec!["new", "failed"];
            if option != "--name" {
                args.extend(["--name", "label"]);
            }
            args.extend([option, value, "--format", "json"]);
            let output = tool(&test.0, locale, &args);
            assert_eq!(output.status.code(), Some(1), "{output:?}");
            assert!(!output.stderr.is_empty());
            let report: Value = serde_json::from_slice(&output.stdout).unwrap();
            assert_eq!(report["error"]["code"], code);
            if let Some(previous) = previous {
                assert_eq!(report, previous);
            }
            previous = Some(report);
            assert!(!test.0.join("failed").exists());
            assert_eq!(test.0.read_dir().unwrap().count(), 0);
        }
    }
    let test = TestDir::new();
    let root = test.0.join("occupied");
    fs::create_dir(&root).unwrap();
    fs::write(root.join("sentinel"), "user bytes").unwrap();
    for locale in ["ru", "en"] {
        let output = tool(
            &test.0,
            locale,
            &["new", "occupied", "--name", "label", "--format", "json"],
        );
        assert_eq!(output.status.code(), Some(1));
        assert_eq!(
            serde_json::from_slice::<Value>(&output.stdout).unwrap()["error"]["code"],
            "source_destination_exists"
        );
        assert_eq!(fs::read(root.join("sentinel")).unwrap(), b"user bytes");
    }
}

/// Relative output uses the advertised project base, while absolute destinations stay exact.
#[test]
fn new_relative_destination_uses_project_dir_and_absolute_destination_is_preserved() {
    let test = TestDir::new();
    let base = test.0.join("base folder");
    fs::create_dir(&base).unwrap();
    let created = success(
        &test.0,
        "en",
        &[
            "--project-dir",
            "base folder",
            "new",
            "child",
            "--name",
            "label",
            "--format",
            "json",
        ],
    );
    assert_eq!(
        created["data"]["root"],
        base.join("child").to_str().unwrap()
    );
    assert!(!test.0.join("child").exists());
    let absolute = test.0.join("absolute project");
    let created = success(
        &test.0,
        "ru",
        &[
            "--project-dir",
            "base folder",
            "new",
            absolute.to_str().unwrap(),
            "--name",
            "label",
            "--format",
            "json",
        ],
    );
    assert_eq!(created["data"]["root"], absolute.to_str().unwrap());
}
