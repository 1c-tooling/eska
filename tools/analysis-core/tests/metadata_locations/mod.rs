//! Current metadata locations are strict, read-only and independent of BSL projection readiness.

use sha2::{Digest, Sha256};
use std::fmt::Write as _;

use super::*;

const OWNER: &str = "CommonModules/Tools/object.yaml";
const UUID: &str = "20000000-0000-4000-8000-000000000001";
const NAMESPACE: &str = "http://v8.1c.ru/8.3/MDClasses";

/// A UUID is selected within an explicit current owner; native property names are qualified.
fn locate(revision: &str, source: &str, uuid: &str, property: Option<&str>) -> Value {
    let mut params = json!({"snapshot":snapshot(revision),"source":source,"uuid":uuid});
    if let Some(name) = property {
        params["property"] = json!({"namespace":NAMESPACE,"name":name});
    }
    request(json!("место"), "eska/metadata/locate", params)
}

/// Read one declared YAML; loading belongs to a separate explicit request rather than locate.
fn read(client: &mut Client, revision: &str, source: &str) -> Value {
    let response = client.ask(&request(
        json!("чтение"),
        "eska/metadata/read",
        json!({"snapshot":snapshot(revision),"source":source}),
    ));
    assert!(response.get("result").is_some(), "{response}");
    response
}

/// A standard range counts UTF-16 units of the actual LF/CRLF fixture, not UTF-8 bytes.
fn position(text: &str, byte: usize) -> Value {
    let prefix = &text[..byte];
    let start = prefix.rfind('\n').map_or(0, |index| index + 1);
    json!({"line":prefix.bytes().filter(|&value|value==b'\n').count(),
        "character":prefix[start..].encode_utf16().count()})
}

/// Source fingerprints are computed independently, including every original line-ending byte.
fn hash(text: &str) -> String {
    let mut output = String::with_capacity(64);
    for byte in Sha256::digest(text.as_bytes()) {
        write!(&mut output, "{byte:02x}").unwrap();
    }
    output
}

/// Both URI and coordinates must address the physical YAML and exactly the parsed input hash.
fn verify(result: &Value, file: &Path, text: &str) {
    let start = usize::try_from(result["byte_range"]["start"].as_u64().unwrap()).unwrap();
    let end = usize::try_from(result["byte_range"]["end"].as_u64().unwrap()).unwrap();
    assert!(start < end);
    assert_eq!(
        result["location"]["uri"],
        url::Url::from_file_path(file).unwrap().as_str()
    );
    assert_eq!(result["location"]["range"]["start"], position(text, start));
    assert_eq!(result["location"]["range"]["end"], position(text, end));
    assert_eq!(result["source_sha256"], hash(text));
}

/// Initialization declares the actual UTF-16 contract without depending on language settings.
fn start(root: &Path, lang: &str, options: &[&str]) -> Client {
    let mut client = Client::with_options(root, lang, options);
    let init = client.ask(&initialize());
    assert_eq!(
        init["result"]["capabilities"]["experimental"]["eska"]["metadata"]["locate"],
        json!({"method":"eska/metadata/locate","scope":"current-ready-owner","positionEncoding":"utf-16","lazyRead":false})
    );
    client.send(&notification("initialized"));
    client
}

/// Deleted disk and invalid editor input cannot cause a stale or reread location to escape.
#[test]
fn metadata_locations_follow_saved_and_editor_bytes_without_disk_reads_in_both_locales() {
    let (_directory, root) = support::project("// Body.\n", false);
    let file = root.join("src").join(OWNER);
    let saved = fs::read_to_string(&file).unwrap();
    let uri = url::Url::from_file_path(&file).unwrap();
    let mut previous = None;
    for lang in ["en-US", "ru-RU"] {
        let mut client = start(&root, lang, &[]);
        let pending = client.ask(&locate("0", OWNER, UUID, None));
        assert_eq!(pending["error"]["data"]["kind"], "metadata_input_pending");
        assert_eq!(
            pending["error"]["data"]["details"]["snapshot"],
            snapshot("0")
        );
        let loaded = read(&mut client, "0", OWNER);
        let first = client.ask(&locate("1", OWNER, UUID, Some("Name")));
        verify(&first["result"], &file, &saved);
        assert_eq!(
            first["result"]["byte_range"],
            loaded["result"]["metadata"]["documents"][0]["objects"][0]["properties"][0]["range"]
        );
        assert_eq!(first["result"]["resources"], loaded["result"]["resources"]);
        fs::remove_file(&file).unwrap();
        assert_eq!(client.ask(&locate("1", OWNER, UUID, Some("Name"))), first);
        fs::write(&file, &saved).unwrap();
        let draft = format!(
            "# Я😀 Tools Name UUID are comment text\r\n{}",
            saved.replace('\n', "\r\n")
        );
        let opened = client.ask(&json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{
            "textDocument":{"uri":uri.as_str(),"languageId":"eska-source","version":1,"text":draft}}}));
        let current = client.ask(&locate("2", OWNER, UUID, Some("Name")));
        verify(&current["result"], &file, &draft);
        assert_eq!(
            current["result"]["origin"],
            json!({"kind":"overlay","document_version":1})
        );
        assert_eq!(
            current["result"]["resources"],
            opened["params"]["resources"]
        );
        assert_ne!(
            current["result"]["source_sha256"],
            first["result"]["source_sha256"]
        );
        let invalid = client.ask(&json!({"jsonrpc":"2.0","method":"textDocument/didChange","params":{
            "textDocument":{"uri":uri.as_str(),"version":2},"contentChanges":[{"text":format!("\u{feff}{draft}")} ]}}));
        assert_eq!(invalid["method"], "eska/inputChanged");
        let unavailable = client.ask(&locate("3", OWNER, UUID, Some("Name")));
        assert_eq!(
            unavailable["error"]["data"]["kind"],
            "metadata_input_unavailable"
        );
        assert_eq!(
            unavailable["error"]["data"]["details"]["failure"]["code"],
            "source_yaml_bom_forbidden"
        );
        client.ask(&json!({"jsonrpc":"2.0","method":"textDocument/didClose","params":{"textDocument":{"uri":uri.as_str()}}}));
        let restored = client.ask(&locate("4", OWNER, UUID, Some("Name")));
        verify(&restored["result"], &file, &saved);
        assert_eq!(restored["result"]["origin"], json!({"kind":"saved"}));
        assert_eq!(restored["result"]["location"], first["result"]["location"]);
        let trace = vec![
            pending,
            loaded,
            first,
            current,
            invalid,
            unavailable,
            restored,
        ];
        if let Some(previous) = &previous {
            assert_eq!(&trace, previous);
        }
        previous = Some(trace);
        assert_eq!(fs::read_to_string(&file).unwrap(), saved);
        client.finish();
    }
}

/// Wrong identity, `QName`, source and wire fields cannot change the current session revision.
#[test]
fn metadata_locations_validate_snapshot_identity_property_and_strict_wire_parameters() {
    let (_directory, root) = support::project("// Body.\n", false);
    let mut previous = None;
    for lang in ["en-US", "ru-RU"] {
        let mut client = start(&root, lang, &[]);
        read(&mut client, "0", OWNER);
        let mut trace = Vec::new();
        let stale = client.ask(&locate("0", OWNER, UUID, None));
        assert_eq!(stale["error"]["code"], -32801);
        trace.push(stale);
        for (query, kind) in [
            (
                locate("1", OWNER, "00000000-0000-4000-8000-000000000000", None),
                "metadata_object_not_found",
            ),
            (
                locate("1", OWNER, UUID, Some("Missing")),
                "metadata_property_not_found",
            ),
        ] {
            let result = client.ask(&query);
            assert_eq!(result["error"]["code"], -32602);
            assert_eq!(result["error"]["data"]["kind"], kind);
            trace.push(result);
        }
        let mut foreign = locate("1", OWNER, UUID, Some("Name"));
        foreign["params"]["property"]["namespace"] = json!("urn:foreign");
        let foreign = client.ask(&foreign);
        assert_eq!(
            foreign["error"]["data"]["kind"],
            "metadata_property_not_found"
        );
        trace.push(foreign);
        let mut extra = locate("1", OWNER, UUID, Some("Name"));
        extra["params"]["property"]["extra"] = json!(true);
        for query in [
            extra,
            locate("1", "../object.yaml", UUID, None),
            locate("1", OWNER, "invalid", None),
            locate("1", "CommonModules/Unknown/object.yaml", UUID, None),
        ] {
            let result = client.ask(&query);
            assert_eq!(result["error"]["code"], -32602);
            trace.push(result);
        }
        let current = client.ask(&locate("1", OWNER, UUID, None));
        assert_eq!(current["result"]["snapshot"], snapshot("1"));
        assert_eq!(current["result"]["resources"]["module_text_bytes"], 0);
        trace.push(current);
        if let Some(previous) = &previous {
            assert_eq!(&trace, previous);
        }
        previous = Some(trace);
        client.finish();
    }
}

/// Inline objects point to the physical owner's YAML even with invalid BSL settings and zero time.
#[test]
fn inline_metadata_location_is_independent_of_language_analysis_and_platform_artifacts() {
    let directory = support::TestDirectory::new();
    let root = support::inline_command_project(&directory.0, "// Body.\n");
    fs::write(root.join("bsl-analyzer.toml"), "[broken\n").unwrap();
    let source = "Catalogs/Sample/object.yaml";
    let uuid = "40000000-0000-4000-8000-000000000001";
    let file = root.join("src-with-command").join(source);
    let saved = fs::read_to_string(&file).unwrap();
    let mut previous = None;
    for lang in ["en-US", "ru-RU"] {
        let mut client = start(&root, lang, &["--timeout-seconds", "0"]);
        read(&mut client, "0", source);
        let object = client.ask(&locate("1", source, uuid, None));
        let property = client.ask(&locate("1", source, uuid, Some("Name")));
        for result in [&object["result"], &property["result"]] {
            verify(result, &file, &saved);
            assert_eq!(result["native_path"], "Catalogs/Sample.xml");
            assert_eq!(result["uuid"], uuid);
            assert_eq!(result["resources"]["module_text_bytes"], 0);
        }
        let trace = [object, property];
        if let Some(previous) = &previous {
            assert_eq!(&trace, previous);
        }
        previous = Some(trace);
        assert_eq!(fs::read_to_string(&file).unwrap(), saved);
        assert!(!root.join(".eska").exists());
        client.finish();
    }
}
