//! Opt-in real-source acceptance never writes into the supplied configuration or extensions.

use eska_project_core::formatting::{Language, Options, protocol::Request};
use std::{fs, path::PathBuf};

/// A copied corpus inside the owned playground must retain tokens and stabilize after one pass.
#[test]
#[ignore = "requires ESKA_FMT_CORPUS pointing to an owned T121 corpus JSON inside ESKA_TEST_ROOT"]
fn real_sources_preserve_tokens_and_are_idempotent() {
    let path = PathBuf::from(std::env::var_os("ESKA_FMT_CORPUS").expect("corpus input"))
        .canonicalize()
        .unwrap();
    let root = PathBuf::from(std::env::var_os("ESKA_TEST_ROOT").expect("isolated root"))
        .canonicalize()
        .unwrap();
    assert!(path.starts_with(root));
    let owner: serde_json::Value =
        serde_json::from_slice(&fs::read(path.parent().unwrap().join("owner.json")).unwrap())
            .unwrap();
    assert_eq!(owner["task"], "T121");
    let document: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    let samples = document["samples"].as_array().unwrap();
    assert!(!samples.is_empty() && samples.len() <= 128);
    let mut results = Vec::new();
    let mut failed = 0;
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    for (index, sample) in samples.iter().enumerate() {
        let source = sample["source"].as_str().unwrap();
        let start = std::time::Instant::now();
        let request = Request {
            schema_version: 1,
            language: Language::Bsl,
            text: source.into(),
            options: Options::default(),
            range: None,
        };
        let result = match eska_fmt::format(&request) {
            Ok(response) => {
                let mut after = source.to_owned();
                for edit in response.edits.iter().rev() {
                    after.replace_range(edit.range.start..edit.range.end, &edit.text);
                }
                let request = Request {
                    text: after,
                    ..request
                };
                match eska_fmt::format(&request) {
                    Ok(repeat) if repeat.edits.is_empty() => {
                        serde_json::json!({"status":"passed","changed":!response.edits.is_empty()})
                    }
                    Ok(repeat) => {
                        failed += 1;
                        let first = path
                            .parent()
                            .unwrap()
                            .join(format!("corpus-{nonce}-{index}-first.bsl"));
                        let second = path
                            .parent()
                            .unwrap()
                            .join(format!("corpus-{nonce}-{index}-second.bsl"));
                        fs::write(&first, &request.text).unwrap();
                        let mut after = request.text.clone();
                        for edit in repeat.edits.iter().rev() {
                            after.replace_range(edit.range.start..edit.range.end, &edit.text);
                        }
                        fs::write(&second, after).unwrap();
                        serde_json::json!({"status":"not_idempotent","first":first,"second":second})
                    }
                    Err(error) => {
                        failed += 1;
                        serde_json::json!({"status":"repeat_error","error":error})
                    }
                }
            }
            Err(error) => {
                failed += 1;
                serde_json::json!({"status":"rejected","error":error})
            }
        };
        results.push(serde_json::json!({"name":sample["name"],"bytes":source.len(),"seconds":start.elapsed().as_secs_f64(),"result":result}));
    }
    let report = serde_json::json!({"task":"T121","complete":failed==0,"count":samples.len(),"failed":failed,"results":results});
    let output = path
        .parent()
        .unwrap()
        .join(format!("corpus-result-{nonce}.json"));
    fs::write(&output, serde_json::to_vec_pretty(&report).unwrap()).unwrap();
    println!("{}", output.display());
    assert_eq!(failed, 0, "inspect {}", output.display());
}
