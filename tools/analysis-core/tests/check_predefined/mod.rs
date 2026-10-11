//! Batch predefined data consume current preserved resources and report failures without invented YAML ranges.

use super::*;
use serde_json::json;

/// Native predefined data failures keep BSL findings and explicit incomplete coverage independent of locale.
#[test]
fn current_saved_predefined_data_and_resource_failures_have_locale_independent_reports() {
    let (directory, root) = support::project(GOOD, false);
    support::predefined_project::add_predefined(&root, &directory.0.join("native"));
    let source = root.join("src-with-predefined");
    let resource = source.join("Catalogs/Items/Ext/Predefined.xml");
    let saved = fs::read(&resource).unwrap();
    let mut previous = None;
    for lang in ["ru-RU", "en-US"] {
        let value = report(
            &run(&root, lang, &["--profile", "syntax", "--format", "json"]),
            0,
        );
        assert!(
            value["data"]["report"]["metadata_issues"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            value["data"]["report"]["coverage"]["metadata_models_complete"],
            true
        );
        if let Some(previous) = &previous {
            assert_eq!(&value, previous);
        }
        previous = Some(value);
    }
    for invalid in [Some("<Other/>"), None] {
        if let Some(text) = invalid {
            fs::write(&resource, text).unwrap();
        } else {
            fs::remove_file(&resource).unwrap();
        }
        let mut previous = None;
        for lang in ["ru-RU", "en-US"] {
            let value = report(
                &run(&root, lang, &["--profile", "syntax", "--format", "json"]),
                2,
            );
            let issues = value["data"]["report"]["metadata_issues"]
                .as_array()
                .unwrap();
            assert_eq!(issues.len(), 1);
            assert_eq!(issues[0]["source"], "Catalogs/Items/object.yaml");
            assert_eq!(
                issues[0]["location"],
                json!(["Catalogs/Items/Ext/Predefined.xml"])
            );
            assert_eq!(
                issues[0]["code"],
                if invalid.is_some() {
                    "analysis_predefined_data_invalid"
                } else {
                    "source_file_missing"
                }
            );
            assert!(issues[0]["range"].is_null());
            assert_eq!(issues[0]["findings"], json!([]));
            assert_eq!(value["data"]["report"]["coverage"]["syntax_complete"], true);
            let human = run(&root, lang, &["--profile", "syntax"]);
            assert_eq!(human.status.code(), Some(2));
            assert!(human.stderr.is_empty());
            let text = String::from_utf8(human.stdout).unwrap();
            assert!(text.contains(if lang == "ru-RU" {
                "Проверка метаданных неполна"
            } else {
                "Metadata check is incomplete"
            }));
            assert!(text.contains(issues[0]["code"].as_str().unwrap()));
            if let Some(previous) = &previous {
                assert_eq!(&value, previous);
            }
            previous = Some(value);
        }
        fs::write(&resource, &saved).unwrap();
    }
    assert_eq!(fs::read(resource).unwrap(), saved);
}
