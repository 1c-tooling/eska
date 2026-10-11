//! Metadata pulls share current YAML state without inheriting BSL configuration failures.

mod guards;

use super::*;
use url::Url;

const OWNER: &str = "CommonModules/Tools/object.yaml";

/// Both standard and explicit ESKA pulls use the existing provider identifier and exact document URI.
fn pull(root: &Path, source: &str, snap: &Value) -> Value {
    request(
        json!(20),
        "textDocument/diagnostic",
        json!({"textDocument":{"uri":Url::from_file_path(root.join("src").join(source)).unwrap().as_str()},
        "identifier":"eska-syntax","eska":{"snapshot":snap}}),
    )
}

/// Input events acknowledge one current full document; no helper edits saved files.
fn edit(root: &Path, source: &str, version: i32, text: &str, open: bool) -> Value {
    let uri = Url::from_file_path(root.join("src").join(source)).unwrap();
    if open {
        json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":uri.as_str(),"languageId":"eska-source","version":version,"text":text}}})
    } else {
        json!({"jsonrpc":"2.0","method":"textDocument/didChange","params":{"textDocument":{"uri":uri.as_str(),"version":version},"contentChanges":[{"text":text}]}})
    }
}

/// Independently count non-BMP/CRLF fixture prefixes without using the production coordinate converter.
fn position(text: &str, byte: usize) -> Value {
    let prefix = &text[..byte];
    let last = prefix.rsplit('\n').next().unwrap().trim_end_matches('\r');
    json!({"line":prefix.bytes().filter(|&byte|byte==b'\n').count(),"character":last.encode_utf16().count()})
}

/// Machine findings retain exact current byte ranges, source authority and domain details in both locales.
fn adapter_item(response: &Value, text: &str, lang: &str) -> Value {
    assert!(response.get("error").is_none(), "{response}");
    let result = &response["result"];
    assert_eq!(result["eska"]["profile"], "metadata");
    assert_eq!(
        result["eska"]["coverage"]["metadata_document_complete"],
        true
    );
    assert_eq!(result["eska"]["coverage"]["metadata_model_complete"], false);
    assert_eq!(result["eska"]["coverage"]["semantic_complete"], false);
    assert!(result["eska"]["config_sha256"].is_null());
    let items = result["items"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    let item = &items[0];
    assert_eq!(item["severity"], 1);
    assert_eq!(item["code"], "analysis_adapter_invalid");
    assert_eq!(
        item["data"]["issue"],
        json!({"kind":"invalid_property","name":"Server"})
    );
    let start = usize::try_from(item["data"]["start_byte"].as_u64().unwrap()).unwrap();
    let end = usize::try_from(item["data"]["end_byte"].as_u64().unwrap()).unwrap();
    assert!(text[start..end].contains("Future"));
    assert_eq!(item["range"]["start"], position(text, start));
    assert_eq!(item["range"]["end"], position(text, end));
    let message = item["message"].as_str().unwrap();
    assert!(message.contains("Server"));
    assert!(message.contains(if lang == "ru-RU" {
        "некорректное"
    } else {
        "invalid"
    }));
    let mut machine = item.clone();
    machine.as_object_mut().unwrap().remove("message");
    machine
}

/// Pulls do not reread changed saved bytes while the document is open, and unavailable close cannot reuse old findings.
#[test]
fn metadata_diagnostics_follow_editor_correction_invalid_and_missing_close() {
    let (_directory, root) = support::project("// Saved caller.\n", false);
    let file = root.join("src").join(OWNER);
    let saved = fs::read_to_string(&file).unwrap();
    let draft = format!(
        "# Я😀 current diagnostics\r\n{}",
        support::property::server_value(&saved, "Future")
    );
    assert_ne!(draft, saved);
    let mut previous = None;
    for lang in ["en-US", "ru-RU"] {
        let mut client = Client::with_options(&root, lang, &[]);
        client.ask(&initialize());
        client.send(&notification("initialized"));
        let ready = client.ask(&pull(&root, OWNER, &snapshot("0")));
        assert_eq!(ready["result"]["items"], json!([]));
        assert_eq!(ready["result"]["eska"]["snapshot"], snapshot("1"));
        let opened = client.ask(&edit(&root, OWNER, 7, &draft, true));
        let snap = &opened["params"]["snapshot"];
        let response = client.ask(&pull(&root, OWNER, snap));
        let first = adapter_item(&response, &draft, lang);
        assert_eq!(
            response["result"]["eska"]["source_sha256"],
            opened["params"]["input"]["state"]["source_sha256"]
        );
        assert_eq!(
            first["data"]["start_byte"],
            opened["params"]["input"]["state"]["model_issue"]["range"]["start"]
        );
        fs::remove_file(&file).unwrap();
        assert_eq!(client.ask(&pull(&root, OWNER, snap)), response);
        let changed = client.ask(&edit(&root, OWNER, 8, &saved, false));
        let corrected = client.ask(&pull(&root, OWNER, &changed["params"]["snapshot"]));
        assert_eq!(corrected["result"]["items"], json!([]));
        assert_eq!(
            corrected["result"]["eska"]["coverage"]["metadata_model_complete"],
            true
        );
        let changed = client.ask(&edit(&root, OWNER, 9, "broken: [\n", false));
        let invalid = client.ask(&pull(&root, OWNER, &changed["params"]["snapshot"]));
        assert_eq!(
            invalid["error"]["data"]["kind"],
            "metadata_input_unavailable"
        );
        let closed=client.ask(&json!({"jsonrpc":"2.0","method":"textDocument/didClose","params":{"textDocument":{"uri":Url::from_file_path(&file).unwrap().as_str()}}}));
        let missing = client.ask(&pull(&root, OWNER, &closed["params"]["snapshot"]));
        assert_eq!(
            missing["error"]["data"]["details"]["input"]["state"]["input"]["failure"]["code"],
            "source_file_missing"
        );
        fs::write(&file, &saved).unwrap();
        assert_eq!(
            client.ask(&pull(&root, OWNER, &closed["params"]["snapshot"])),
            missing
        );
        let refreshed = client.ask(&request(
            json!(21),
            "eska/metadata/refresh",
            json!({"snapshot":closed["params"]["snapshot"],"source":OWNER}),
        ));
        assert_eq!(
            client.ask(&pull(&root, OWNER, &refreshed["result"]["snapshot"]))["result"]["items"],
            json!([])
        );
        if let Some(previous) = &previous {
            assert_eq!(&first, previous);
        }
        previous = Some(first);
        assert_eq!(fs::read_to_string(&file).unwrap(), saved);
        client.finish();
    }
}

/// Preserved unknown children produce one localized parse finding; incomplete projection is not duplicated.
#[test]
fn metadata_parse_diagnostics_use_captured_child_spans_and_standard_pulls() {
    use eska::project_core::source_format::{
        Child, Namespace, Section, SourcePath, SourceReader, XmlFragment,
    };
    let (_directory, root) = support::project("// Saved caller.\n", false);
    let reader = SourceReader::open(&root.join("src")).unwrap();
    let mut source = reader
        .read_owner(&SourcePath::parse(OWNER).unwrap())
        .unwrap()
        .source;
    source
        .document
        .descriptor
        .object
        .sections
        .push(Section::Children(vec![Child::Preserved {
            fragment: XmlFragment {
                xml: "<future:Child/>".into(),
                namespaces: vec![Namespace {
                    prefix: Some("future".into()),
                    uri: "urn:future".into(),
                }],
            },
        }]));
    let text = format!(
        "# Я😀 parse issue\r\n{}",
        serde_json::to_string(&source).unwrap()
    );
    fs::write(root.join("src").join(OWNER), &text).unwrap();
    let mut previous = None;
    for lang in ["en-US", "ru-RU"] {
        let mut client = Client::with_options(&root, lang, &[]);
        client.ask(&initialize());
        client.send(&notification("initialized"));
        let mut standard = pull(&root, OWNER, &snapshot("0"));
        standard["params"].as_object_mut().unwrap().remove("eska");
        standard["params"]["previousResultId"] = json!("stale-report");
        let response = client.ask(&standard);
        let result = &response["result"];
        assert_eq!(result["kind"], "full");
        assert_eq!(result["items"].as_array().unwrap().len(), 1);
        assert_eq!(
            result["eska"]["coverage"]["metadata_document_complete"],
            false
        );
        let item = &result["items"][0];
        assert_eq!(item["severity"], 2);
        assert_eq!(item["code"], "analysis_metadata_parse_issue");
        assert_eq!(
            item["data"]["issue"]["cause"],
            json!({"UnknownKind":{"namespace":"urn:future","name":"Child"}})
        );
        let start = usize::try_from(item["data"]["start_byte"].as_u64().unwrap()).unwrap();
        let end = usize::try_from(item["data"]["end_byte"].as_u64().unwrap()).unwrap();
        assert!(text[start..end].contains("future:Child"));
        assert_eq!(item["range"]["start"], position(&text, start));
        assert_eq!(item["range"]["end"], position(&text, end));
        let mut machine = item.clone();
        machine.as_object_mut().unwrap().remove("message");
        if let Some(previous) = &previous {
            assert_eq!(&machine, previous);
        }
        previous = Some(machine);
        client.finish();
    }
}
