//! Real checker processes retain findings and incomplete project coverage across native artifact types.

use eska::project_core::ProjectType;

use super::support::TestDirectory;
use super::*;
#[path = "../support/local_syntax_project.rs"]
mod fixture;

/// Scope admission is identical in RU/EN; semantic refusal and original CF behavior stay separate.
#[test]
fn local_syntax_profiles_check_all_four_types_without_claiming_semantic_completeness() {
    for kind in fixture::TYPES {
        let (_directory, root, module) = fixture::project(kind, fixture::BAD, false);
        let mut values = Vec::new();
        for lang in ["ru-RU", "en-US"] {
            let expected = if kind == ProjectType::Configuration {
                1
            } else {
                2
            };
            let value = report(
                &run(&root, lang, &["--profile", "syntax", "--format", "json"]),
                expected,
            );
            let checked = &value["data"]["report"];
            assert_eq!(
                checked["analysis_scope"],
                if kind == ProjectType::Configuration {
                    "canonical_configuration"
                } else {
                    "local_syntax"
                }
            );
            assert_eq!(checked["coverage"]["syntax_complete"], true);
            assert_eq!(checked["coverage"]["checked_modules"], 1);
            assert_eq!(
                checked["coverage"]["metadata_models_complete"],
                kind == ProjectType::Configuration
            );
            assert!(
                checked["coverage"]
                    .get("semantic_profile_complete")
                    .is_none()
            );
            assert_eq!(checked["files"][0]["source"], module.as_str());
            let findings = checked["files"][0]["findings"].as_array().unwrap();
            assert!(!findings.is_empty());
            assert!(findings.iter().all(|item| item["code"] == "ParseError"));
            if kind != ProjectType::Configuration {
                assert!(
                    checked["metadata_issues"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .all(|issue| issue["code"] == "analysis_context_required")
                );
                let human = run(&root, lang, &["--profile", "syntax"]);
                assert_eq!(human.status.code(), Some(2));
                let text = String::from_utf8(human.stdout).unwrap();
                assert!(text.contains(if lang == "ru-RU" {
                    "локальный синтаксис BSL"
                } else {
                    "local BSL syntax"
                }));
                let semantic = run(&root, lang, &["--profile", "semantic", "--format", "json"]);
                assert_eq!(semantic.status.code(), Some(3));
                let refusal: Value = serde_json::from_slice(&semantic.stdout).unwrap();
                assert_eq!(
                    refusal["error"]["code"],
                    if kind == ProjectType::Extension {
                        "analysis_scope_unsupported"
                    } else {
                        "analysis_configuration_context_required"
                    }
                );
            }
            values.push(value);
        }
        assert_eq!(values[0], values[1]);
        assert_eq!(
            fs::read(root.join("src").join(module.as_path())).unwrap(),
            fixture::BAD.as_bytes()
        );
    }
}

/// Binary, bounded-out and unvisited sources never become a clean empty syntax result.
#[test]
fn local_syntax_binary_deadline_and_quota_failures_keep_explicit_coverage() {
    for kind in fixture::TYPES {
        for case in ["binary", "deadline", "quota"] {
            let (_directory, root, _module) =
                fixture::project(kind, fixture::GOOD, case == "binary");
            let mut args = vec!["--profile", "syntax", "--format", "json"];
            match case {
                "deadline" => args.extend(["--timeout-seconds", "0"]),
                "quota" => args.extend(["--max-retained-source-bytes", "1"]),
                _ => {}
            }
            let value = report(&run(&root, "en-US", &args), 2);
            let checked = &value["data"]["report"];
            assert_eq!(checked["coverage"]["syntax_complete"], false);
            assert_eq!(checked["coverage"]["checked_modules"], 0);
            match case {
                "binary" => {
                    assert_eq!(checked["coverage"]["binary_modules"], 1);
                    assert_eq!(checked["files"][0]["state"]["kind"], "binary");
                }
                "deadline" => assert_eq!(checked["stop"], "deadline_exceeded"),
                _ => {
                    assert_eq!(checked["coverage"]["unavailable_modules"], 1);
                    assert_eq!(checked["resources"]["total_source_bytes"], 0);
                }
            }
        }
    }
}
