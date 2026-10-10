//! The real wire reports native estimates separately from source quotas and unavailable RSS.

use super::*;

const METHOD: &str = "eska/analysis/memory";
const MODULE: &str = "CommonModules/Tools/Ext/Module.bsl";
const BODY: &str =
    "\u{feff}Функция Пример(Аргумент)\r\nЯ=\"😀\";Возврат Аргумент;\r\nКонецФункции\r\n";

/// Exact public keys distinguish absent heap hooks, incomplete estimates and a missing RSS reader.
fn verify(report: &Value, revision: &str, source_bytes: usize) {
    assert_eq!(report["schema_version"], 1);
    assert_eq!(report["snapshot"], snapshot(revision));
    assert_eq!(report["heap_estimate_complete"], false);
    assert_eq!(report["process_rss_bytes"], Value::Null);
    assert_eq!(report["resources"]["module_text_bytes"], source_bytes);
    assert!(report["kernel"].get("revision").is_none());
    let tables = report["kernel"]["tables"].as_array().unwrap();
    assert!(!tables.is_empty());
    assert!(
        tables
            .windows(2)
            .all(|pair| pair[0]["name"].as_str() <= pair[1]["name"].as_str())
    );
    assert!(tables.iter().all(|table| {
        table
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>()
            == [
                "entries",
                "field_stack_bytes",
                "heap_bytes",
                "metadata_bytes",
                "name",
            ]
            && table["entries"].is_u64()
            && table["metadata_bytes"].is_u64()
            && table["field_stack_bytes"].is_u64()
            && (table["heap_bytes"].is_null() || table["heap_bytes"].is_u64())
    }));
    assert!(tables.iter().any(|table| table["heap_bytes"].is_null()));
    assert!(tables.iter().any(|table| table["heap_bytes"] == 0));
    assert_eq!(
        report["kernel"]["tables_without_heap_hook"],
        tables
            .iter()
            .filter(|table| table["heap_bytes"].is_null())
            .count()
    );
    assert_eq!(
        report["kernel"]["live_tables_without_heap_hook"],
        tables
            .iter()
            .filter(|table| table["entries"].as_u64().unwrap() > 0 && table["heap_bytes"].is_null())
            .count()
    );
    assert_eq!(
        report["kernel"]["summed_heap_estimates"],
        tables
            .iter()
            .filter_map(|table| table["heap_bytes"].as_u64())
            .sum::<u64>()
    );
}

/// Two cold samples are identical, remain locale-independent and do not warm source or queries.
#[test]
fn kernel_memory_sampling_preserves_cold_state_and_machine_schema_in_both_locales() {
    let (_directory, root) = support::project(BODY, false);
    let mut previous = None;
    for locale in ["en-US", "ru-RU"] {
        let mut client = Client::with_options(&root, locale, &[]);
        let initialized = client.ask(&initialize());
        let capability = &initialized["result"]["capabilities"]["experimental"]["eska"];
        assert_eq!(
            capability["kernelMemory"],
            json!({"method":METHOD,"scope":"current-session","lazyRead":false,"heapEstimateComplete":false,"processRss":false})
        );
        assert!(
            !capability["languageMethods"]
                .as_array()
                .unwrap()
                .contains(&json!(METHOD))
        );
        assert!(
            !capability["cancellableMethods"]
                .as_array()
                .unwrap()
                .contains(&json!(METHOD))
        );
        client.send(&notification("initialized"));
        let before = client.ask(&request(
            json!("state"),
            "eska/metadata/owners",
            json!({"snapshot":snapshot("0"),"limit":1}),
        ));
        let first = client.ask(&request(
            json!("memory"),
            METHOD,
            json!({"snapshot":snapshot("0")}),
        ));
        let second = client.ask(&request(
            json!("memory"),
            METHOD,
            json!({"snapshot":snapshot("0")}),
        ));
        assert_eq!(first, second);
        verify(&first["result"], "0", 0);
        assert_eq!(first["result"]["resources"], before["result"]["resources"]);
        let after = client.ask(&request(
            json!("state"),
            "eska/metadata/owners",
            json!({"snapshot":snapshot("0"),"limit":1}),
        ));
        assert_eq!(before, after);
        let trace = (initialized, first, after);
        if let Some(old) = &previous {
            assert_eq!(&trace, old);
        } else {
            previous = Some(trace);
        }
        client.finish();
    }
}

/// Sampling retains overlay authority after saved deletion and reports released source separately.
#[test]
fn kernel_memory_reports_warm_overlay_and_close_without_lazy_reload_or_clock_change() {
    let (_directory, root) = support::project(BODY, false);
    let path = root.join("src").join(MODULE);
    let uri = url::Url::from_file_path(&path).unwrap();
    let mut previous_warm = None;
    for locale in ["en-US", "ru-RU"] {
        fs::write(&path, BODY).unwrap();
        let mut client = Client::with_options(&root, locale, &[]);
        client.ask(&initialize());
        client.send(&notification("initialized"));
        let opened = client.ask(
            &json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{
            "textDocument":{"uri":uri.as_str(),"version":1,"languageId":"bsl","text":BODY}}}),
        );
        let symbols = request(
            json!("symbols"),
            "textDocument/documentSymbol",
            json!({"textDocument":{"uri":uri.as_str()},"eska":{"snapshot":snapshot("1")}}),
        );
        let before = client.ask(&symbols);
        assert!(!before["result"].as_array().unwrap().is_empty());
        let warm = client.ask(&request(
            json!("memory"),
            METHOD,
            json!({"snapshot":snapshot("1")}),
        ));
        verify(&warm["result"], "1", BODY.len());
        assert_eq!(warm["result"]["resources"], opened["params"]["resources"]);
        if let Some(previous) = &previous_warm {
            assert_eq!(&warm, previous);
        } else {
            previous_warm = Some(warm.clone());
        }
        fs::remove_file(&path).unwrap();
        assert_eq!(
            client.ask(&request(
                json!("memory"),
                METHOD,
                json!({"snapshot":snapshot("1")})
            )),
            warm
        );
        assert_eq!(client.ask(&symbols), before);
        let closed = client.ask(&json!({"jsonrpc":"2.0","method":"textDocument/didClose","params":{"textDocument":{"uri":uri.as_str()}}}));
        let released = client.ask(&request(
            json!("memory"),
            METHOD,
            json!({"snapshot":snapshot("2")}),
        ));
        verify(&released["result"], "2", 0);
        assert_eq!(
            released["result"]["resources"],
            closed["params"]["resources"]
        );
        assert!(!path.exists());
        client.finish();
    }
}

/// Invalid guards never sample another clock; telemetry remains available with invalid BSL settings.
#[test]
fn kernel_memory_requires_current_snapshot_and_ignores_bsl_settings_and_query_timeout() {
    let (_directory, root) = support::project(BODY, false);
    fs::write(root.join("bsl-analyzer.toml"), "[invalid").unwrap();
    for locale in ["en-US", "ru-RU"] {
        let mut client = Client::with_options(&root, locale, &["--timeout-seconds", "0"]);
        let initialized = client.ask(&initialize());
        assert!(
            initialized["result"]["capabilities"]
                .get("completionProvider")
                .is_none()
        );
        client.send(&notification("initialized"));
        for params in [json!({}), json!({"snapshot":snapshot("0"),"source":MODULE})] {
            assert_eq!(
                client.ask(&request(json!("guard"), METHOD, params))["error"]["data"]["kind"],
                "rpc_invalid_params"
            );
        }
        assert_eq!(
            client.ask(&request(
                json!("guard"),
                METHOD,
                json!({"snapshot":snapshot("1")})
            ))["error"]["data"]["kind"],
            "stale_revision"
        );
        let foreign = client.ask(&request(
            json!("guard"),
            METHOD,
            json!({"snapshot":{"session_id":"other","revision":"0"}}),
        ));
        assert_eq!(foreign["error"]["data"]["kind"], "unknown_session");
        let current = client.ask(&request(
            json!("guard"),
            METHOD,
            json!({"snapshot":snapshot("0")}),
        ));
        verify(&current["result"], "0", 0);
        client.finish();
    }
}
