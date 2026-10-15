//! Optional reports preserve legacy pulls and publish actual current-resource consumer locations.

use super::*;
use eska::project_core::source_format::{SourcePath, SourceReader};

/// Select the validated physical XML resource and add one real dataset binding before analysis.
fn bound_resource(project: &fixture::Project) -> (SourcePath, String) {
    let reader = SourceReader::open(&project.root.join("src")).unwrap();
    let query = reader.declared_query(&project.source).unwrap();
    let resource = reader
        .manifest()
        .files
        .iter()
        .find(|file| &file.path == query.native_resource)
        .unwrap()
        .source_path()
        .clone();
    let path = project.root.join("src").join(resource.as_path());
    let original = fs::read_to_string(&path).unwrap();
    let text = original.replace("</name>", "</name><!-- Я😀 -->\r\n<field xsi:type=\"DataSetFieldField\"><dataPath>Published</dataPath><field>Value</field></field>");
    assert_ne!(text, original);
    fs::write(path, &text).unwrap();
    fs::write(
        project.root.join("src").join(project.source.as_path()),
        "SELECT 1 AS Renamed",
    )
    .unwrap();
    (resource, text)
}

/// Reports share the existing contextual identifier, timer and document input writer.
fn start_consumers(project: &fixture::Project, lang: &str, extra: &[&str]) -> Client {
    let mut client = Client::with_options(&project.root, lang, extra);
    let mut init = initialize();
    init["params"]["initializationOptions"]["eska"] = json!({
        "protocolVersion":"0.1","queryAnalysis":true,"queryConsumers":true
    });
    let result = client.ask(&init);
    assert!(result.get("error").is_none(), "{result}");
    let capability = &result["result"]["capabilities"]["experimental"]["eska"]["queryConsumers"];
    assert_eq!(capability["version"], 1);
    assert_eq!(capability["enabled"], true);
    assert_eq!(capability["consumerComplete"], false);
    client.send(&notification("initialized"));
    client
}

/// Source hashes and native-to-physical locations remain tied to the response's current generation.
fn checked(client: &mut Client, project: &fixture::Project) -> Value {
    let full = pull(
        client,
        &uri(project, &project.source),
        Some("eska-query-analysis"),
    );
    assert!(full.get("error").is_none(), "{full}");
    let status = &full["result"]["eska"]["consumers"];
    assert_eq!(status["kind"], "checked", "{full}");
    assert_eq!(
        status["report"]["context"]["revision"].to_string(),
        full["result"]["eska"]["snapshot"]["revision"]
            .as_str()
            .unwrap()
    );
    assert_eq!(full["result"]["eska"]["resources"]["module_text_bytes"], 0);
    assert_eq!(status["report"]["coverage"]["parameters_complete"], false);
    assert_eq!(status["report"]["coverage"]["consumer_complete"], false);
    status["report"].clone()
}

/// Only human finding messages differ; all source stamps and coordinate facts remain machine data.
fn machine_report(mut report: Value) -> Value {
    for finding in report["findings"].as_array_mut().unwrap() {
        finding.as_object_mut().unwrap().remove("message");
    }
    report
}

/// Current XML changes can repair or invalidate bindings while query text remains untouched.
fn current_resource_case(
    project: &fixture::Project,
    resource: &SourcePath,
    saved: &str,
    lang: &str,
) -> Value {
    let mut client = start_consumers(project, lang, &[]);
    let first = checked(&mut client, project);
    let finding = &first["findings"][0];
    assert_eq!(finding["code"], "query_consumer_output_field_missing");
    assert_eq!(finding["location"]["source"], resource.as_str());
    assert_eq!(finding["location"]["authoring"]["precision"], "exact");
    let start = saved.find("<field>Value</field>").unwrap();
    let prefix = &saved[..start];
    assert_eq!(
        finding["location"]["authoring"]["range"]["start"]["line"],
        prefix.bytes().filter(|&byte| byte == b'\n').count()
    );
    assert_eq!(
        finding["location"]["authoring"]["range"]["start"]["character"],
        prefix.rsplit('\n').next().unwrap().encode_utf16().count()
    );
    let resource_uri = uri(project, resource);
    let repair = saved.replace("<field>Value</field>", "<field>Renamed</field>");
    let ack = client.ask(
        &json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{
            "textDocument":{"uri":resource_uri,"languageId":"xml","version":7,"text":repair}
        }}),
    );
    assert_eq!(ack["method"], "eska/inputChanged");
    let repaired = checked(&mut client, project);
    assert_eq!(repaired["findings"], json!([]));
    assert_eq!(
        repaired["context"]["resource"]["origin"],
        json!({"kind":"overlay","document_version":7})
    );
    assert_eq!(repaired["context"]["query"], first["context"]["query"]);
    let invalid = client.ask(&json!({"jsonrpc":"2.0","method":"textDocument/didChange","params":{
        "textDocument":{"uri":resource_uri,"version":8},"contentChanges":[{"text":"<invalid current"}]
    }}));
    assert_eq!(invalid["method"], "eska/inputChanged");
    let full = pull(
        &mut client,
        &uri(project, &project.source),
        Some("eska-query-analysis"),
    );
    assert_eq!(
        full["result"]["eska"]["consumers"]["kind"], "unavailable",
        "{full}"
    );
    assert!(full["result"]["eska"]["consumers"].get("report").is_none());
    let close = client.ask(&json!({"jsonrpc":"2.0","method":"textDocument/didClose","params":{"textDocument":{"uri":resource_uri}}}));
    assert_eq!(close["method"], "eska/inputChanged");
    let restored = checked(&mut client, project);
    assert_eq!(restored["findings"], first["findings"]);
    client.finish();
    machine_report(first)
}

/// CF and selected dependency/CFE composition share current storage semantics and RU/EN coordinates.
#[test]
fn consumer_reports_current_resource_and_locales_share_one_query_pull() {
    for composition in [false, true] {
        let directory = TestDirectory::new();
        let project = if composition {
            fixture::composition(&directory.0).0
        } else {
            fixture::project(&directory.0, "configuration")
        };
        let (resource, saved) = bound_resource(&project);
        let before = fixture::snapshot(&project);
        let ru = current_resource_case(&project, &resource, &saved, "ru-RU");
        let en = current_resource_case(&project, &resource, &saved, "en-US");
        assert_eq!(ru, en);
        assert_eq!(fixture::snapshot(&project), before);
    }
}

/// Legacy clients receive no additional payload; invalid negotiation and zero deadline precede reads.
#[test]
fn consumer_reports_require_independent_opt_in_and_existing_deadline_preflight() {
    let directory = TestDirectory::new();
    let project = fixture::project(&directory.0, "configuration");
    let (mut legacy, _) = start(&project, "en-US", true, true, &[]);
    let result = pull(
        &mut legacy,
        &uri(&project, &project.source),
        Some("eska-query-analysis"),
    );
    assert!(result["result"]["eska"].get("consumers").is_none());
    legacy.finish();
    for options in [
        json!({"queryAnalysis":true,"queryConsumers":"true"}),
        json!({"queryConsumers":true}),
    ] {
        let mut client = Client::with_options(&project.root, "en-US", &[]);
        let mut init = initialize();
        init["params"]["initializationOptions"]["eska"] = options;
        init["params"]["initializationOptions"]["eska"]["protocolVersion"] = json!("0.1");
        assert_eq!(
            client.ask(&init)["error"]["data"]["kind"],
            "rpc_invalid_params"
        );
        let valid = client.ask(&initialize());
        assert!(valid.get("error").is_none(), "{valid}");
        client.send(&notification("initialized"));
        client.finish();
    }
    let mut client = start_consumers(&project, "en-US", &["--timeout-seconds", "0"]);
    let result = pull(
        &mut client,
        &uri(&project, &project.source),
        Some("eska-query-analysis"),
    );
    assert_eq!(
        result["error"]["data"]["kind"],
        "analysis_deadline_exceeded"
    );
    client.finish();
}

/// Saved IDE and checker findings match, while human output uses each requested locale.
#[cfg(feature = "check-cli")]
#[test]
fn consumer_reports_saved_checker_and_ide_match_without_localizing_machine_output() {
    let directory = TestDirectory::new();
    let project = fixture::project(&directory.0, "configuration");
    bound_resource(&project);
    let before = fixture::snapshot(&project);
    let mut variants = Vec::new();
    for lang in ["ru-RU", "en-US"] {
        let mut client = start_consumers(&project, lang, &[]);
        let ide = machine_report(checked(&mut client, &project));
        client.finish();
        let output = std::process::Command::new(env!("CARGO_BIN_EXE_eska-check"))
            .current_dir(&project.root)
            .env("PATH", "")
            .args(["--profile", "semantic", "--format", "json", "--lang", lang])
            .output()
            .unwrap();
        assert_eq!(
            output.status.code(),
            Some(2),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(output.stderr.is_empty());
        let json: Value = serde_json::from_slice(&output.stdout).unwrap();
        let file = json["data"]["report"]["query_files"]
            .as_array()
            .unwrap()
            .iter()
            .find(|file| file["source"] == project.source.as_str())
            .unwrap();
        let batch = &file["consumers"]["report"];
        assert_eq!(batch["findings"], ide["findings"]);
        assert_eq!(batch["coverage"], ide["coverage"]);
        for source in ["query", "owner", "resource"] {
            assert_eq!(batch["context"][source], ide["context"][source]);
        }
        variants.push(file["consumers"].clone());
        let output = std::process::Command::new(env!("CARGO_BIN_EXE_eska-check"))
            .current_dir(&project.root)
            .env("PATH", "")
            .args(["--profile", "semantic", "--format", "human", "--lang", lang])
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(2));
        let text = String::from_utf8(output.stdout)
            .unwrap()
            .replace(['\u{2068}', '\u{2069}'], "");
        assert!(
            text.contains(if lang == "ru-RU" {
                "нет поля «Value»"
            } else {
                "no field \"Value\""
            }),
            "{text}"
        );
    }
    assert_eq!(variants[0], variants[1]);
    assert_eq!(fixture::snapshot(&project), before);
}
