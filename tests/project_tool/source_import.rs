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
