//! The project tool cannot read or prepare a source while a selected import guard is present.

use super::*;

/// Human guidance is translated while both process operations retain one stable JSON failure.
#[test]
fn pending_source_import_blocks_inspect_and_prepare_in_both_locales() {
    let test = TestDir::new();
    native(&test.0);
    let pin = import(&test.0, "project", "sample");
    let root = test.0.join("project");
    let settings = fs::read(root.join("eska.toml")).unwrap();
    let source = fs::read(root.join("src/configuration.yaml")).unwrap();
    fs::write(root.join(".eska-project-source-import.json"), "{}").unwrap();
    for args in [
        vec!["inspect"],
        vec!["prepare", "--accepted-base-sha256", &pin],
    ] {
        let mut reports = Vec::new();
        for (locale, phrase) in [
            ("ru", "Импорт исходников прерван"),
            ("en", "Source import was interrupted"),
        ] {
            let human = tool(&root, locale, &args);
            assert_eq!(human.status.code(), Some(1), "{human:?}");
            assert!(human.stdout.is_empty());
            assert!(String::from_utf8_lossy(&human.stderr).contains(phrase));
            let mut json = args.clone();
            json.extend(["--format", "json"]);
            let output = tool(&root, locale, &json);
            assert_eq!(output.status.code(), Some(1), "{output:?}");
            assert!(String::from_utf8_lossy(&output.stderr).contains(phrase));
            let report: Value = serde_json::from_slice(&output.stdout).unwrap();
            assert_eq!(report["schema_version"], 1);
            assert_eq!(report["error"]["code"], "source_import_incomplete");
            reports.push(report);
        }
        assert_eq!(reports[0], reports[1]);
    }
    assert_eq!(fs::read(root.join("eska.toml")).unwrap(), settings);
    assert_eq!(
        fs::read(root.join("src/configuration.yaml")).unwrap(),
        source
    );
    assert!(!root.join(".eska/generations").exists());
    fs::remove_file(root.join(".eska-project-source-import.json")).unwrap();
    success(&root, "ru", &["inspect", "--format", "json"]);
}

/// Capturing explicitly supplied source bytes emits an ancestry-only pin and never changes support.
#[test]
fn captures_an_explicit_ancestor_with_localized_help_and_stable_output() {
    let test = TestDir::new();
    native(&test.0);
    let pin = import(&test.0, "project", "sample");
    for (locale, phrase) in [("ru", "предка"), ("en", "ancestry")] {
        let output = tool(&test.0, locale, &["capture-ancestor", "--help"]);
        assert_eq!(output.status.code(), Some(0));
        assert!(String::from_utf8_lossy(&output.stdout).contains(phrase));
        let destination = format!("ancestor-{locale}");
        let value = success(
            &test.0,
            locale,
            &[
                "capture-ancestor",
                "project/src",
                &destination,
                "--format",
                "json",
            ],
        );
        assert_eq!(value["data"]["support_approved"], false);
        let reference = eska::project_core::source_format::ImportAncestorRef::from_sha256(
            value["data"]["ancestor"]["sha256"].as_str().unwrap(),
        )
        .unwrap();
        eska::project_core::source_format::ImportAncestor::open(
            &test.0.join(destination),
            &reference,
        )
        .unwrap();
        let accepted = eska::project_core::source_format::AcceptedRef {
            sha256: pin.clone(),
        };
        eska::project_core::source_format::AcceptedBase::open(
            &test.0.join("project/.eska/accepted"),
            &accepted,
        )
        .unwrap();
    }
}

/// Both explicit actions recover a real guarded journal after original staging has disappeared.
#[test]
fn recovery_process_resolves_a_guard_without_discovery_or_platform_access() {
    use eska::project_core::{
        discovery::discover,
        metadata_model::ProjectScope,
        source_format::{
            AcceptedRef, ImportAncestor, PROJECT_SOURCE_IMPORT_RECEIPT, capture_import_ancestor,
            prepare_project_source_import,
        },
    };
    for (locale, action) in [("ru", "complete"), ("en", "roll-back")] {
        let test = TestDir::new();
        native(&test.0);
        let pin = import(&test.0, "project", "sample");
        let root = test.0.join("project");
        let before = fs::read(root.join("src/configuration.yaml")).unwrap();
        let ancestor_pin =
            capture_import_ancestor(&root.join("src"), &test.0.join("ancestor")).unwrap();
        let ancestor = ImportAncestor::open(&test.0.join("ancestor"), &ancestor_pin).unwrap();
        ancestor.restore(&test.0.join("incoming-source")).unwrap();
        let path = test.0.join("incoming-source/configuration.yaml");
        let contents = fs::read_to_string(&path).unwrap();
        fs::write(path, contents.replace("Accepted", "Incoming")).unwrap();
        let incoming_pin =
            capture_import_ancestor(&test.0.join("incoming-source"), &test.0.join("incoming"))
                .unwrap();
        let incoming = ImportAncestor::open(&test.0.join("incoming"), &incoming_pin).unwrap();
        let project = discover(&root).unwrap();
        let plan = prepare_project_source_import(
            &project,
            &ProjectScope::Standalone,
            &AcceptedRef { sha256: pin },
            &ancestor,
            &incoming,
        )
        .unwrap();
        let reference = plan.reference().sha256().to_owned();
        let error = plan
            .publish_cancellable(&|| root.join(PROJECT_SOURCE_IMPORT_RECEIPT).exists())
            .unwrap_err();
        assert_eq!(error.code, "source_read_cancelled");
        drop(plan);
        fs::remove_dir_all(test.0.join("incoming")).unwrap();
        let wrong = tool(
            &root,
            locale,
            &[
                "recover-import",
                "--action",
                action,
                "--receipt-sha256",
                &"f".repeat(64),
                "--format",
                "json",
            ],
        );
        assert_eq!(wrong.status.code(), Some(1));
        assert!(root.join(PROJECT_SOURCE_IMPORT_RECEIPT).exists());
        let output = success(
            &root,
            locale,
            &[
                "recover-import",
                "--action",
                action,
                "--receipt-sha256",
                &reference,
                "--format",
                "json",
            ],
        );
        assert_eq!(output["data"]["action"], action);
        assert!(!root.join(PROJECT_SOURCE_IMPORT_RECEIPT).exists());
        if action == "complete" {
            assert!(
                fs::read_to_string(root.join("src/configuration.yaml"))
                    .unwrap()
                    .contains("Incoming")
            );
        } else {
            assert_eq!(
                fs::read(root.join("src/configuration.yaml")).unwrap(),
                before
            );
        }
        success(
            &root,
            locale,
            &[
                "recover-import",
                "--action",
                action,
                "--receipt-sha256",
                &reference,
                "--format",
                "json",
            ],
        );
    }
}
