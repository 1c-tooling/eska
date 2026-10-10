//! Metadata JSON and human results preserve current causes without changing CI completion status.

use super::*;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::fmt::Write;

use support::metadata::{OWNER, text as current_text};

/// Count the verbatim prefix independently of the production source map.
fn position(text: &str, byte: usize) -> Value {
    let prefix = &text[..byte];
    json!({"line":prefix.bytes().filter(|&byte| byte == b'\n').count(),"character":prefix.rsplit('\n').next().unwrap().encode_utf16().count()})
}

/// Detailed findings match current hashes and UTF-16 spans; machine output has no translated message.
#[test]
fn metadata_causes_and_coordinates_are_stable_in_both_locales() {
    for parse_issue in [false, true] {
        let (_directory, root) = support::project(GOOD, false);
        let text = current_text(&root, parse_issue);
        let file = root.join("src").join(OWNER);
        fs::write(&file, &text).unwrap();
        let mut previous = None;
        for lang in ["en-US", "ru-RU"] {
            let value = report(
                &run(&root, lang, &["--profile", "syntax", "--format", "json"]),
                2,
            );
            assert_eq!(value["data"]["position_encoding"], "unicode-scalar");
            assert_eq!(value["data"]["metadata_position_encoding"], "utf-16");
            let batch = &value["data"]["report"];
            assert_eq!(batch["coverage"]["syntax_complete"], true);
            assert_eq!(batch["coverage"]["metadata_models_complete"], false);
            let issue = &batch["metadata_issues"][0];
            assert_eq!(issue["source"], OWNER);
            assert_eq!(
                issue["source_sha256"],
                Sha256::digest(text.as_bytes())
                    .iter()
                    .fold(String::new(), |mut hash, byte| {
                        write!(hash, "{byte:02x}").unwrap();
                        hash
                    })
            );
            assert_eq!(issue["code"], "metadata_projection_invalid");
            let items = issue["findings"].as_array().unwrap();
            assert_eq!(items.len(), 1);
            let finding = &items[0];
            let (code, cause, severity, needle, message) = if parse_issue {
                (
                    "analysis_metadata_parse_issue",
                    json!({"kind":"parse_issue","cause":{"UnknownKind":{"namespace":"urn:future","name":"Child"}}}),
                    2,
                    "future:Child",
                    if lang == "ru-RU" {
                        "неизвестный"
                    } else {
                        "unknown"
                    },
                )
            } else {
                (
                    "analysis_adapter_invalid",
                    json!({"kind":"invalid_property","name":"Server"}),
                    1,
                    "Future",
                    if lang == "ru-RU" {
                        "некорректное"
                    } else {
                        "invalid"
                    },
                )
            };
            assert_eq!(finding["code"], code);
            assert_eq!(finding["severity"], severity);
            assert_eq!(finding["data"]["issue"], cause);
            assert_eq!(finding["data"]["native_path"], "CommonModules/Tools.xml");
            let start = usize::try_from(finding["data"]["start_byte"].as_u64().unwrap()).unwrap();
            let end = usize::try_from(finding["data"]["end_byte"].as_u64().unwrap()).unwrap();
            let prefix = text[..start].rsplit('\n').next().unwrap();
            assert_eq!(prefix.encode_utf16().count(), prefix.chars().count() + 1);
            assert!(text[start..end].contains(needle));
            assert_eq!(
                finding["range"],
                json!({"start":position(&text,start),"end":position(&text,end)})
            );
            let human = run(&root, lang, &["--profile", "syntax"]);
            assert_eq!(human.status.code(), Some(2));
            assert!(human.stderr.is_empty());
            let human = String::from_utf8(human.stdout).unwrap();
            assert!(human.contains(message), "{human}");
            assert!(human.contains(code) && human.contains(&format!("{OWNER}:2:")));
            if let Some(previous) = &previous {
                assert_eq!(&value, previous);
            }
            previous = Some(value);
            assert_eq!(fs::read_to_string(&file).unwrap(), text);
        }
    }
}

/// Refused current YAML stays incomplete with no fabricated span; correction clears the previous metadata issue.
#[test]
fn unavailable_metadata_has_null_findings_and_correction_clears_report() {
    let (_directory, root) = support::project(GOOD, false);
    let file = root.join("src").join(OWNER);
    let saved = fs::read(&file).unwrap();
    for missing in [false, true] {
        if missing {
            fs::remove_file(&file).unwrap();
        } else {
            fs::write(&file, "broken: [\n").unwrap();
        }
        let mut previous = None;
        for lang in ["en-US", "ru-RU"] {
            let value = report(
                &run(&root, lang, &["--profile", "syntax", "--format", "json"]),
                2,
            );
            let issue = &value["data"]["report"]["metadata_issues"][0];
            assert_eq!(issue["source"], OWNER);
            assert!(
                issue.as_object().unwrap().contains_key("findings") && issue["findings"].is_null()
            );
            assert!(issue["range"].is_null());
            assert_eq!(
                issue["code"],
                if missing {
                    "source_file_missing"
                } else {
                    "source_yaml_invalid"
                }
            );
            let human = run(&root, lang, &["--profile", "syntax"]);
            assert_eq!(human.status.code(), Some(2));
            let text = String::from_utf8(human.stdout).unwrap();
            assert!(text.contains(if lang == "ru-RU" {
                "Проверка метаданных неполна"
            } else {
                "Metadata check is incomplete"
            }));
            assert!(text.contains(OWNER) && text.contains(issue["code"].as_str().unwrap()));
            if let Some(previous) = &previous {
                assert_eq!(&value, previous);
            }
            previous = Some(value);
        }
        fs::write(&file, &saved).unwrap();
        let value = report(
            &run(&root, "en-US", &["--profile", "syntax", "--format", "json"]),
            0,
        );
        assert_eq!(value["data"]["report"]["metadata_issues"], json!([]));
    }
}
