//! Queries migration preserves raw source ancestry without promoting local edits or dropping history.

use crate::query_creation_fixture as fixture;
#[path = "query_migration/workspace.rs"]
mod workspace;

use super::*;
use eska::project_core::{
    discovery::discover,
    metadata_model::ProjectScope,
    source_format::{
        FormPropertyName, FormPropertyValue, ImportAncestor, ImportAncestorRef, SourceReader,
        capture_import_ancestor, prepare_project_source_import, project_import_ancestor,
    },
};

/// The new Queries report is explicit while the existing readable report retains its old schema.
fn assert_report(data: &Value, version: u32, pin: &str) {
    assert_eq!(data["source_layout"], "queries");
    assert_eq!(data["source_layout_version"], version);
    assert_eq!(data["previous_baseline"], pin);
    assert_eq!(data["baseline"], pin);
    assert_eq!(data["layout_changed"], false);
    assert_eq!(data["ancestry_preserved"], true);
    assert_eq!(data["ci_pin_updated"], false);
    assert_eq!(data["effective_git_policy_verified"], false);
    assert_eq!(data["source_changes"], serde_json::json!([]));
    assert_eq!(data["accepted_changes"], serde_json::json!([]));
    assert_eq!(
        data.as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        [
            "accepted_changes",
            "ancestry_preserved",
            "baseline",
            "ci_pin_updated",
            "copied_project_files",
            "descriptor_schema_version",
            "destination",
            "effective_git_policy_verified",
            "layout_changed",
            "previous_baseline",
            "publication",
            "root",
            "source_changes",
            "source_layout",
            "source_layout_version",
            "written",
        ]
    );
}

/// Saved forms and queries remain working edits; the copied raw ancestor keeps its pristine bytes.
fn save_working_edits(project: &Path, native: &fixture::Native) -> u32 {
    let source = project.join("src");
    let reader = SourceReader::open(&source).unwrap();
    let version = reader.manifest().schema_version;
    assert_eq!(reader.configuration().path.as_str(), native.root_source);
    assert_eq!(reader.query_paths().count(), 2);
    let form_path = reader.form_dialog_paths().next().unwrap();
    assert_eq!(form_path.as_path(), native.form_source);
    let mut form = reader.read_owner(form_path).unwrap().source;
    form.form
        .as_mut()
        .unwrap()
        .properties
        .insert(FormPropertyName::ReadOnly, FormPropertyValue::Bool(true));
    fs::write(source.join(form_path.as_path()), form.to_yaml().unwrap()).unwrap();
    fs::write(
        source.join(reader.query_paths().next().unwrap().as_path()),
        b"SELECT 42;\r\n",
    )
    .unwrap();
    fs::write(
        source.join(native.module_source),
        b"\xef\xbb\xbf// local\r\n",
    )
    .unwrap();
    assert!(native.input.join(&native.form).is_file());
    version
}

/// Each native profile retains exact working/base/history/settings bytes and an independently held pin.
#[test]
fn queries_noop_migration_covers_all_four_profiles_in_both_locales() {
    for kind in fixture::TYPES {
        let test = TestDir::new();
        let native = fixture::native(&test.0, kind);
        let imported = success(
            &test.0,
            "en",
            &[
                "import", "native", "old", "--name", "sample", "--layout", "queries", "--format",
                "json",
            ],
        );
        let pin = imported["data"]["accepted_base_sha256"].as_str().unwrap();
        let ancestor =
            ImportAncestorRef::from_sha256(imported["data"]["ancestor_sha256"].as_str().unwrap())
                .unwrap();
        let old = test.0.join("old");
        let version = save_working_edits(&old, &native);
        let before = fixture::snapshot(&old);
        let args = [
            "migrate",
            "../new",
            "--layout",
            "queries",
            "--accepted-base-sha256",
            pin,
            "--format",
            "json",
        ];
        let ru = success(&old, "ru", &args);
        assert_eq!(ru, success(&old, "en", &args));
        for (locale, phrase) in [
            ("ru", "План проекта Queries"),
            ("en", "Queries project preview"),
        ] {
            let human = tool(&old, locale, &args[..6]);
            assert_eq!(human.status.code(), Some(0));
            assert!(human.stderr.is_empty());
            assert!(String::from_utf8_lossy(&human.stdout).contains(phrase));
        }
        assert_report(&ru["data"], version, pin);
        assert_eq!(ru["data"]["written"], false);
        assert_eq!(ru["data"]["publication"], Value::Null);
        assert!(!test.0.join("new").exists());
        let written = success(&old, "en", &[&args[..], &["--write"]].concat());
        assert_report(&written["data"], version, pin);
        assert_eq!(written["data"]["publication"], "created");
        let new = test.0.join("new");
        assert_eq!(fixture::snapshot(&new), before);
        assert_eq!(fixture::snapshot(&old), before);
        let copied = discover(&new).unwrap();
        let raw = project_import_ancestor(&copied, &ProjectScope::Standalone, &ancestor).unwrap();
        raw.restore(&test.0.join("restored raw")).unwrap();
        assert_eq!(
            fs::read(test.0.join("restored raw").join(native.module_source)).unwrap(),
            fs::read(native.input.join(&native.module)).unwrap()
        );
        for (locale, phrase) in [
            ("ru", "Проект Queries записан"),
            ("en", "Queries project published"),
        ] {
            let repeat = success(&old, locale, &[&args[..], &["--write"]].concat());
            assert_eq!(repeat["data"]["publication"], "unchanged");
            let human = tool(&old, locale, &[&args[..6], &["--write"]].concat());
            assert_eq!(human.status.code(), Some(0));
            assert!(human.stderr.is_empty());
            assert!(String::from_utf8_lossy(&human.stdout).contains(phrase));
        }
        assert_prepared(&new, pin, &native);
    }
}

/// The copied project exports local authoring through the ordinary guarded preparation boundary.
fn assert_prepared(project: &Path, pin: &str, native: &fixture::Native) {
    let prepared = success(
        project,
        "en",
        &["prepare", "--accepted-base-sha256", pin, "--format", "json"],
    );
    let xml = Path::new(prepared["data"]["xml"].as_str().unwrap());
    assert!(xml.join(&native.root).is_file());
    assert_eq!(
        fs::read(xml.join(&native.module)).unwrap(),
        b"\xef\xbb\xbf// local\r\n"
    );
    let form = fs::read_to_string(xml.join(&native.dialog)).unwrap();
    let schema = fs::read_to_string(xml.join(&native.schema)).unwrap();
    assert!(form.contains("<ReadOnly>true</ReadOnly>"));
    assert!(form.contains("SELECT 42;") || schema.contains("SELECT 42;"));
}

/// A historical readable project cannot silently lose or invent ancestry during query conversion.
#[test]
fn layout_changing_history_refusal_is_localized_and_json_stable() {
    let test = TestDir::new();
    native(&test.0);
    let imported = success(
        &test.0,
        "en",
        &[
            "import", "native", "old", "--name", "sample", "--layout", "readable", "--format",
            "json",
        ],
    );
    let pin = imported["data"]["accepted_base_sha256"].as_str().unwrap();
    let old = test.0.join("old");
    let project = discover(&old).unwrap();
    let store = test.0.join("confirmed raw");
    let reference = capture_import_ancestor(project.source(), &store).unwrap();
    let ancestor = ImportAncestor::open(&store, &reference).unwrap();
    prepare_project_source_import(
        &project,
        &ProjectScope::Standalone,
        &eska::project_core::source_format::AcceptedRef { sha256: pin.into() },
        &ancestor,
        &ancestor,
    )
    .unwrap()
    .publish()
    .unwrap();
    let before = fixture::snapshot(&old);
    let args = [
        "migrate",
        "../refused",
        "--layout",
        "queries",
        "--accepted-base-sha256",
        pin,
        "--write",
    ];
    let mut errors = Vec::new();
    for (locale, phrase) in [("ru", "историю"), ("en", "history")] {
        let human = tool(&old, locale, &args);
        assert_eq!(human.status.code(), Some(1));
        assert!(human.stdout.is_empty());
        assert!(String::from_utf8_lossy(&human.stderr).contains(phrase));
        let output = tool(&old, locale, &[&args[..], &["--format", "json"]].concat());
        assert_eq!(output.status.code(), Some(1));
        assert!(String::from_utf8_lossy(&output.stderr).contains(phrase));
        let json: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(
            json["error"]["code"],
            "source_migration_ancestry_unsupported"
        );
        errors.push(json);
    }
    assert_eq!(errors[0], errors[1]);
    assert!(!test.0.join("refused").exists());
    assert_eq!(fixture::snapshot(&old), before);
}

/// Migrate layout choices have their own defaults and localized help rather than import family help.
#[test]
fn query_migration_help_and_invalid_layout_work_in_both_locales() {
    let test = TestDir::new();
    for (locale, phrase) in [
        ("ru", "readable (по умолчанию)"),
        ("en", "readable (default)"),
    ] {
        let output = tool(&test.0, locale, &["migrate", "--help"]);
        assert_eq!(output.status.code(), Some(0));
        assert!(output.stderr.is_empty());
        let help = String::from_utf8_lossy(&output.stdout);
        assert!(help.contains("--layout"));
        assert!(help.contains(phrase));
        assert!(help.contains("queries"));
        let invalid = tool(
            &test.0,
            locale,
            &[
                "migrate", "refused", "--layout", "family", "--format", "json",
            ],
        );
        assert_eq!(invalid.status.code(), Some(1));
        let json: Value = serde_json::from_slice(&invalid.stdout).unwrap();
        assert_eq!(json["error"]["code"], "arguments_invalid");
        assert!(!test.0.join("refused").exists());
    }
}

/// Selecting Queries adds fields only to its report; default readable JSON keeps its original keys.
#[test]
fn readable_migration_default_keeps_its_json_contract_and_does_not_bootstrap_history() {
    let test = TestDir::new();
    native(&test.0);
    let pin = import(&test.0, "old", "sample");
    let old = test.0.join("old");
    let args = [
        "migrate",
        "../new",
        "--accepted-base-sha256",
        &pin,
        "--format",
        "json",
    ];
    let preview = success(&old, "ru", &args);
    assert_eq!(
        preview,
        success(&old, "en", &[&args[..], &["--layout", "readable"]].concat())
    );
    let data = &preview["data"];
    assert_eq!(data["source_layout"], "readable");
    assert_eq!(data["source_layout_version"], 3);
    assert_eq!(data["descriptor_schema_version"], 2);
    assert_eq!(
        data.as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        [
            "accepted_changes",
            "baseline",
            "ci_pin_updated",
            "copied_project_files",
            "descriptor_schema_version",
            "destination",
            "effective_git_policy_verified",
            "layout_changed",
            "previous_baseline",
            "publication",
            "root",
            "source_changes",
            "source_layout",
            "source_layout_version",
            "written",
        ]
    );
    success(&old, "en", &[&args[..], &["--write"]].concat());
    assert!(!test.0.join("new/.eska/source-import").exists());
}
