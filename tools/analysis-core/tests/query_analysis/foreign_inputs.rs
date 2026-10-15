//! Declaration pages qualify foreign synchronization without reading current owner or module payloads.

use super::*;

const METHOD: &str = "eska/analysis/inputs";

/// One rooted page always cites the caller's exact host generation.
fn page(client: &mut Client, snapshot: &Value, root: &Value, offset: usize, limit: usize) -> Value {
    client.ask(&request(
        json!(41),
        METHOD,
        json!({"snapshot":snapshot,"root":root,"offset":offset,"limit":limit}),
    ))
}

/// Both locales enumerate only visible foreign registered inputs, even if their saved payload vanished.
#[test]
fn visible_foreign_input_pages_are_exact_lazy_and_locale_independent() {
    let directory = TestDirectory::new();
    let (project, dependency, catalog) = fixture::composition(&directory.0);
    let path = dependency.join("src").join(catalog.as_path());
    let saved = fs::read(&path).unwrap();
    let mut variants = Vec::new();
    for lang in ["ru-RU", "en-US"] {
        let (mut client, init) = start(&project, lang, true, true, &[]);
        let eska = &init["result"]["capabilities"]["experimental"]["eska"];
        assert_eq!(eska["inputDeclarations"]["version"], 1);
        assert_eq!(eska["inputDeclarations"]["method"], METHOD);
        let snapshot = &eska["snapshot"];
        let composition = &eska["composition"];
        fs::remove_file(&path).unwrap();
        let mut inputs = Vec::new();
        for root in composition["roots"].as_array().unwrap() {
            if root["visible"] == true && root["root"] != composition["selected"] {
                let mut offset = 0;
                loop {
                    let response = page(&mut client, snapshot, &root["root"], offset, 1);
                    let result = &response["result"];
                    assert!(response.get("error").is_none(), "{response}");
                    assert_eq!(&result["snapshot"], snapshot);
                    assert_eq!(result["root"], root["root"]);
                    let total = usize::try_from(result["total"].as_u64().unwrap()).unwrap();
                    let entries = result["inputs"].as_array().unwrap();
                    assert!(entries.len() <= 1);
                    for entry in entries {
                        assert_eq!(entry["root"], root["root"]);
                        let path = url::Url::parse(entry["uri"].as_str().unwrap())
                            .unwrap()
                            .to_file_path()
                            .unwrap();
                        assert_eq!(
                            path,
                            std::path::Path::new(root["source_root"].as_str().unwrap())
                                .join(entry["source"].as_str().unwrap())
                        );
                        assert!(
                            ["eska-source", "bsl", "sdbl", "xml"]
                                .contains(&entry["languageId"].as_str().unwrap())
                        );
                        inputs.push(entry.clone());
                    }
                    offset += entries.len();
                    if offset == total {
                        break;
                    }
                    assert!(!entries.is_empty());
                }
            } else {
                let refused = page(&mut client, snapshot, &root["root"], 0, 1);
                assert_eq!(
                    refused["error"]["data"]["kind"],
                    "analysis_root_not_visible"
                );
            }
        }
        assert!(
            inputs
                .iter()
                .any(|entry| entry["source"] == catalog.as_str())
        );
        variants.push(inputs);
        fs::write(&path, &saved).unwrap();
        client.finish();
    }
    assert_eq!(variants[0], variants[1]);
}

/// Snapshot/shape/page bounds and the common request deadline are checked before any source read.
#[test]
fn foreign_input_pages_keep_snapshot_bounds_and_deadline_guards() {
    let directory = TestDirectory::new();
    let (project, _, _) = fixture::composition(&directory.0);
    for extra in [vec![], vec!["--timeout-seconds", "0"]] {
        let (mut client, init) = start(&project, "en-US", true, true, &extra);
        let eska = &init["result"]["capabilities"]["experimental"]["eska"];
        let snapshot = &eska["snapshot"];
        let root = &eska["composition"]["roots"]
            .as_array()
            .unwrap()
            .iter()
            .find(|root| root["visible"] == true && root["root"] != eska["composition"]["selected"])
            .unwrap()["root"];
        let response = page(&mut client, snapshot, root, 0, 1);
        if extra.is_empty() {
            assert!(response.get("error").is_none(), "{response}");
            for limit in [0, 201] {
                assert_eq!(
                    page(&mut client, snapshot, root, 0, limit)["error"]["data"]["kind"],
                    "page_limit_invalid"
                );
            }
            let mut stale = snapshot.clone();
            stale["revision"] = json!("999999");
            assert_eq!(
                page(&mut client, &stale, root, 0, 1)["error"]["data"]["kind"],
                "stale_revision"
            );
        } else {
            assert_eq!(
                response["error"]["data"]["kind"],
                "analysis_deadline_exceeded"
            );
        }
        client.finish();
    }
}
