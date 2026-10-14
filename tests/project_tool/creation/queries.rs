//! Query-layout creation is a real offline process for all four native types, in both UI languages.

#[path = "../../fixtures/eska-format/query_creation.rs"]
mod fixture;

use super::*;
use eska::project_core::source_format::{
    AcceptedBase, AcceptedRef, FormPropertyName, FormPropertyValue, ImportAncestorRef,
    NativeQueryDocument, SourceReader, project_import_ancestor,
};

/// Query creation reports separate support/ancestry digests and exact project/source coordinates.
fn assert_import_data(data: &Value, project: &Path) {
    assert_eq!(data["source_format"], "eska");
    assert_eq!(data["source_layout"], "queries");
    assert_eq!(data["accepted_base"], ".eska/accepted");
    assert_eq!(data["name"], "sample");
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
            "name",
            "root",
            "source",
            "source_format",
            "source_layout"
        ]
    );
    assert_eq!(data["root"], project.to_str().unwrap());
    assert_eq!(data["source"], project.join("src").to_str().unwrap());
}

/// Candidate digests and the stable import envelope do not depend on the selected UI language.
#[test]
fn query_import_inspect_and_prepare_cover_four_profiles_on_ru_and_en() {
    for kind in fixture::TYPES {
        let test = TestDir::new();
        let native = fixture::native(&test.0, kind);
        let original = fixture::snapshot(&native.input);
        let mut pins = Vec::new();
        let mut sources = Vec::new();
        for (locale, destination) in [("ru", "project ru"), ("en", "project en")] {
            let imported = success(
                &test.0,
                locale,
                &[
                    "import",
                    "native",
                    destination,
                    "--name",
                    "sample",
                    "--layout",
                    "queries",
                    "--platform-version",
                    "8.3.27.2325",
                    "--format",
                    "json",
                ],
            );
            assert_eq!(imported["command"], "import");
            let data = &imported["data"];
            let pin = data["accepted_base_sha256"].as_str().unwrap();
            let ancestor =
                ImportAncestorRef::from_sha256(data["ancestor_sha256"].as_str().unwrap()).unwrap();
            pins.push(pin.to_owned());
            let project = test.0.join(destination);
            assert_import_data(data, &project);
            let inspected = success(&project, locale, &["inspect", "--format", "json"]);
            assert_eq!(inspected["data"]["type"], kind);
            assert_eq!(inspected["data"]["metadata"]["name"], "NativeName");
            assert_eq!(inspected["data"]["metadata"]["uuid"], fixture::ROOT_UUID);
            assert_eq!(inspected["data"]["platform_version"], "8.3.27.2325");
            let reader = SourceReader::open(&project.join("src")).unwrap();
            assert_eq!(reader.configuration().path.as_str(), native.root_source);
            assert_eq!(reader.query_paths().count(), 2);
            assert_eq!(reader.form_dialog_paths().count(), 1);
            let form = reader
                .read_owner(
                    &eska::project_core::source_format::SourcePath::parse(
                        native.form_source.to_str().unwrap(),
                    )
                    .unwrap(),
                )
                .unwrap();
            assert!(form.source.form.is_some());
            assert!(!project.join("src").join(&native.dialog).exists());
            let prepared = success(
                &project,
                locale,
                &["prepare", "--accepted-base-sha256", pin, "--format", "json"],
            );
            assert_eq!(prepared["data"]["baseline"], pin);
            assert_eq!(prepared["data"]["exclusions"], serde_json::json!([]));
            let xml = Path::new(prepared["data"]["xml"].as_str().unwrap());
            for path in [&native.dialog, &native.schema, &native.module] {
                assert_eq!(
                    fs::read(xml.join(path)).unwrap(),
                    fs::read(native.input.join(path)).unwrap()
                );
            }
            assert!(xml.join(&native.root).exists());
            assert_eq!(fs::read(xml.join("opaque.bin")).unwrap(), [0, 255, 10, 13]);
            assert!(xml.join("Empty").is_dir());
            assert_eq!(
                fs::read(project.join("src").join(native.module_source)).unwrap(),
                fs::read(native.input.join(&native.module)).unwrap()
            );
            AcceptedBase::open(
                &project.join(".eska/accepted"),
                &AcceptedRef { sha256: pin.into() },
            )
            .unwrap();
            sources.push(fixture::snapshot(&project.join("src")));
            let configured = eska::project_core::discovery::discover(&project).unwrap();
            let initial = project_import_ancestor(
                &configured,
                &eska::project_core::metadata_model::ProjectScope::Standalone,
                &ancestor,
            )
            .unwrap();
            let restored = test.0.join(format!("restored {locale}"));
            initial.restore(&restored).unwrap();
            assert_eq!(
                fixture::snapshot(&restored),
                sources.last().unwrap().clone()
            );
            assert!(!project.join(".git").exists());
            assert!(!project.join("build").exists());
        }
        assert_eq!(pins[0], pins[1]);
        assert_eq!(sources[0], sources[1]);
        assert_eq!(fixture::snapshot(&native.input), original);
    }
}

/// Freeze reaches the unified form and its plain query; exclusions never rewrite the working source.
#[test]
fn query_creation_freeze_blocks_saved_edits_and_restores_native_bytes_in_both_locales() {
    for kind in fixture::TYPES {
        let test = TestDir::new();
        let native = fixture::native(&test.0, kind);
        let imported = success(
            &test.0,
            "ru",
            &[
                "import",
                "native",
                "project",
                "--name",
                "sample",
                "--layout",
                "queries",
                "--freeze",
                fixture::FORM_UUID,
                "--format",
                "json",
            ],
        );
        let pin = imported["data"]["accepted_base_sha256"].as_str().unwrap();
        let project = test.0.join("project");
        let reader = SourceReader::open(&project.join("src")).unwrap();
        let form_path = eska::project_core::source_format::SourcePath::parse(
            native.form_source.to_str().unwrap(),
        )
        .unwrap();
        let mut form = reader.read_owner(&form_path).unwrap().source;
        form.form
            .as_mut()
            .unwrap()
            .properties
            .insert(FormPropertyName::ReadOnly, FormPropertyValue::Bool(true));
        fs::write(
            project.join("src").join(&native.form_source),
            form.to_yaml().unwrap(),
        )
        .unwrap();
        let query = reader
            .query_paths()
            .find(|path| path.as_str().contains("/Main/Queries/"))
            .unwrap();
        fs::write(project.join("src").join(query.as_path()), "SELECT 42;\r\n").unwrap();
        let working = fixture::snapshot(&project.join("src"));
        let mut errors = Vec::new();
        for locale in ["ru", "en"] {
            let output = tool(
                &project,
                locale,
                &["prepare", "--accepted-base-sha256", pin, "--format", "json"],
            );
            assert_eq!(output.status.code(), Some(1), "{output:?}");
            assert!(!output.stderr.is_empty());
            let error: Value = serde_json::from_slice(&output.stdout).unwrap();
            assert_eq!(error["error"]["code"], "source_changes_excluded");
            assert!(
                error["error"]["diagnostics"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .all(|diagnostic| diagnostic["reason"] == "eska_frozen")
            );
            errors.push(error);
        }
        assert_eq!(errors[0], errors[1]);
        assert!(!project.join(".eska/generations").exists());
        let restored = success(
            &project,
            "en",
            &[
                "prepare",
                "--accepted-base-sha256",
                pin,
                "--allow-exclusions",
                "--format",
                "json",
            ],
        );
        let xml = Path::new(restored["data"]["xml"].as_str().unwrap());
        assert_eq!(
            fs::read(xml.join(&native.dialog)).unwrap(),
            fs::read(native.input.join(&native.dialog)).unwrap()
        );
        let schema = fs::read_to_string(xml.join(&native.schema)).unwrap();
        assert_eq!(
            NativeQueryDocument::from_xml(&schema).unwrap().queries()[0].text,
            "SELECT 2;\r\n// native query"
        );
        assert_eq!(fixture::snapshot(&project.join("src")), working);
        AcceptedBase::open(
            &project.join(".eska/accepted"),
            &AcceptedRef { sha256: pin.into() },
        )
        .unwrap();
    }
}

/// Every failure is locale-free and removes only an operation-owned incomplete project.
#[test]
fn incomplete_or_unverified_query_creation_preserves_inputs_and_foreign_destinations() {
    for kind in fixture::TYPES {
        for failure in [
            "native",
            "descriptor",
            "form",
            "query",
            "freeze",
            "existing",
        ] {
            let test = TestDir::new();
            let native = fixture::native(&test.0, kind);
            match failure {
                "native" => {
                    let file = native.input.join(&native.root);
                    fs::write(
                        &file,
                        fs::read_to_string(&file).unwrap().replace("2.20", "9.99"),
                    )
                    .unwrap();
                }
                "descriptor" => fs::remove_file(native.input.join(&native.form)).unwrap(),
                "form" => fs::write(native.input.join(&native.dialog), "<Form/>").unwrap(),
                "query" => fs::write(
                    native.input.join(&native.schema),
                    "<DataCompositionSchema/>",
                )
                .unwrap(),
                _ => {}
            }
            let original = fixture::snapshot(&native.input);
            let destination = test.0.join("refused");
            if failure == "existing" {
                fs::create_dir(&destination).unwrap();
                fs::write(destination.join("sentinel"), b"user bytes").unwrap();
            }
            let mut args = vec![
                "import", "native", "refused", "--name", "sample", "--layout", "queries",
                "--format", "json",
            ];
            if failure == "freeze" {
                args.extend(["--freeze", "90000000-0000-4000-8000-000000000001"]);
            }
            let mut reports = Vec::new();
            for locale in ["ru", "en"] {
                let output = tool(&test.0, locale, &args);
                assert_eq!(output.status.code(), Some(1), "{output:?}");
                assert!(!output.stderr.is_empty());
                let error: Value = serde_json::from_slice(&output.stdout).unwrap();
                assert_eq!(error["status"], "error");
                assert_eq!(
                    error["error"]["code"],
                    match failure {
                        "native" => "source_profile_unsupported",
                        "descriptor" => "source_descriptor_missing",
                        "form" => "source_form_profile_unverified",
                        "query" => "source_query_profile_unverified",
                        "freeze" => "source_freeze_object_unknown",
                        "existing" => "source_destination_exists",
                        _ => unreachable!(),
                    }
                );
                reports.push(error);
                if failure == "existing" {
                    assert_eq!(
                        fs::read(destination.join("sentinel")).unwrap(),
                        b"user bytes"
                    );
                } else {
                    assert!(!destination.exists());
                }
                assert_eq!(fixture::snapshot(&native.input), original);
            }
            assert_eq!(reports[0], reports[1]);
        }
    }
}

/// The explicit layout remains usable without TTY and reports successful creation in either language.
#[test]
fn human_query_creation_is_localized_without_requiring_platform_or_git() {
    for kind in fixture::TYPES {
        let test = TestDir::new();
        fixture::native(&test.0, kind);
        for (locale, destination, phrase) in [
            ("ru", "human ru", "Проект создан"),
            ("en", "human en", "Project created"),
        ] {
            let output = tool(
                &test.0,
                locale,
                &[
                    "import",
                    "native",
                    destination,
                    "--name",
                    "sample",
                    "--layout",
                    "queries",
                ],
            );
            assert_eq!(output.status.code(), Some(0), "{output:?}");
            assert!(output.stderr.is_empty());
            assert!(String::from_utf8_lossy(&output.stdout).contains(phrase));
            assert!(String::from_utf8_lossy(&output.stdout).contains("--ancestor-sha256"));
            assert!(test.0.join(destination).join("eska.toml").exists());
        }
    }
}
