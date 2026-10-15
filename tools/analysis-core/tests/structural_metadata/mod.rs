//! Framed metadata pulls expose structural readiness without canonical or DCS/query completeness.

use super::{Client, fs, initialize, notification, request, support};
use eska::project_core::source_format::SourceReader;
use serde_json::{Value, json};
use std::path::Path;

#[path = "../support/structural_project.rs"]
mod fixture;

const TEMPLATE: &str = "Reports/Demo/Templates/Schema/template.yaml";

/// Every URI refers to the sole current authoring input in this test's selected source.
fn uri(root: &Path, source: &str) -> String {
    url::Url::from_file_path(root.join("src").join(source))
        .unwrap()
        .to_string()
}

/// A single semantic process retains the same metadata checks and saved/editor state machine.
fn start(root: &Path, lang: &str) -> Client {
    let mut client = Client::with_options(root, lang, &["--profile", "semantic"]);
    assert!(client.ask(&initialize()).get("error").is_none());
    client.send(&notification("initialized"));
    client
}

/// Structural fields and current coordinates use metadata diagnostics independently of BSL rules.
fn pull(root: &Path, source: &str) -> Value {
    request(
        json!(20),
        "textDocument/diagnostic",
        json!({"textDocument":{"uri":uri(root, source)}}),
    )
}

/// Dirty invalid fields cannot reuse saved structure; watch respects editor authority and close repairs.
#[test]
fn structural_descriptors_lsp_dirty_watch_close_preserve_current_incomplete_state() {
    let directory = support::TestDirectory::new();
    let root = fixture::project(&directory.0);
    fixture::settings(&root);
    let saved = fs::read_to_string(root.join("src").join(TEMPLATE)).unwrap();
    let dirty = saved.replace("DataCompositionSchema", "FutureTemplateType");
    let mut machine = Vec::new();
    for lang in ["ru-RU", "en-US"] {
        let mut client = start(&root, lang);
        let clean = client.ask(&pull(&root, TEMPLATE));
        assert_eq!(
            clean["result"]["eska"]["coverage"]["metadata_model_complete"],
            true
        );
        assert_eq!(
            clean["result"]["eska"]["coverage"]["semantic_complete"],
            false
        );
        assert_eq!(clean["result"]["items"], json!([]));
        let opened = client.ask(&json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":uri(&root,TEMPLATE),"languageId":"eska-source","version":3,"text":dirty}}}));
        let invalid = client.ask(&pull(&root, TEMPLATE));
        assert_eq!(
            invalid["result"]["eska"]["coverage"]["metadata_model_complete"],
            false
        );
        let item = &invalid["result"]["items"][0];
        assert_eq!(item["code"], "analysis_adapter_invalid");
        assert_eq!(item["data"]["issue"]["kind"], "unsupported_property_value");
        assert_eq!(item["data"]["issue"]["name"], "TemplateType");
        let begin = usize::try_from(item["data"]["start_byte"].as_u64().unwrap()).unwrap();
        let end = usize::try_from(item["data"]["end_byte"].as_u64().unwrap()).unwrap();
        assert!(dirty[begin..end].contains("FutureTemplateType"));
        let mut value = item.clone();
        value.as_object_mut().unwrap().remove("message");
        machine.push(value);
        let watched = client.ask(&json!({"jsonrpc":"2.0","method":"workspace/didChangeWatchedFiles","params":{"changes":[{"uri":uri(&root,TEMPLATE),"type":2}]}}));
        assert_eq!(watched["params"]["snapshot"], opened["params"]["snapshot"]);
        assert_eq!(client.ask(&pull(&root, TEMPLATE)), invalid);
        let closed = client.ask(&json!({"jsonrpc":"2.0","method":"textDocument/didClose","params":{"textDocument":{"uri":uri(&root,TEMPLATE)}}}));
        assert_eq!(closed["method"], "eska/inputChanged");
        let repaired = client.ask(&pull(&root, TEMPLATE));
        assert_eq!(
            repaired["result"]["eska"]["coverage"]["metadata_model_complete"],
            true
        );
        assert_eq!(repaired["result"]["items"], json!([]));
        assert_eq!(
            repaired["result"]["eska"]["coverage"]["semantic_complete"],
            false
        );
        client.finish();
    }
    assert_eq!(machine[0], machine[1]);
    assert_eq!(
        fs::read_to_string(root.join("src").join(TEMPLATE)).unwrap(),
        saved
    );
}

/// BSL profile completeness is explicitly distinct from unadmitted DCS/query URI and consumer checks.
#[test]
fn structural_descriptors_lsp_semantic_bsl_coverage_does_not_admit_query_inputs() {
    let directory = support::TestDirectory::new();
    let root = fixture::project(&directory.0);
    fixture::settings(&root);
    let source = root.join("src");
    let reader = SourceReader::open(&source).unwrap();
    let query = reader.query_paths().next().unwrap().clone();
    let module = reader.module_paths().next().unwrap().clone();
    fs::write(
        source.join(query.as_path()),
        "SELECT Missing.Field FROM Catalog.Unknown AS Missing",
    )
    .unwrap();
    fs::write(
        source.join("Reports/Demo/Templates/Schema/Template.xml"),
        b"invalid reference XML",
    )
    .unwrap();
    for lang in ["ru-RU", "en-US"] {
        let mut client = start(&root, lang);
        let descriptor = client.ask(&pull(&root, TEMPLATE));
        assert_eq!(
            descriptor["result"]["eska"]["coverage"]["metadata_model_complete"],
            true
        );
        assert_eq!(
            descriptor["result"]["eska"]["coverage"]["semantic_complete"],
            false
        );
        let body = client.ask(&pull(&root, module.as_str()));
        assert_eq!(
            body["result"]["eska"]["coverage"]["semantic_profile_complete"],
            true
        );
        assert_eq!(
            body["result"]["eska"]["coverage"]["semantic_complete"],
            false
        );
        for (path, expected) in [
            (query.as_str(), "query_syntax_not_negotiated"),
            (
                "Reports/Demo/Templates/Schema/Template.xml",
                "unknown_source",
            ),
        ] {
            let refusal = client.ask(&pull(&root, path));
            assert_eq!(refusal["error"]["data"]["kind"], expected);
        }
        client.finish();
    }
}
