//! Signature help respects frozen settings and validates context before lazy reads or timers.

use super::*;

/// Unavailable call sources never return a successful null signature or previous saved bytes.
#[test]
fn unavailable_signature_sources_keep_explicit_input_errors_in_both_locales() {
    for (binary, missing, options) in [
        (true, false, vec![]),
        (false, true, vec![]),
        (false, false, vec!["--max-retained-source-bytes", "0"]),
    ] {
        let (_directory, root) = support::project(BODY, binary);
        let module = if binary {
            "CommonModules/Tools/Ext/Module.bin"
        } else {
            MODULE
        };
        if missing {
            fs::remove_file(root.join("src").join(module)).unwrap();
        }
        for lang in ["en-US", "ru-RU"] {
            let mut client = Client::with_options(&root, lang, &options);
            client.ask(&initialize());
            client.send(&notification("initialized"));
            let reply = client.ask(&pull(
                &root,
                module,
                "src",
                "0",
                json!({"line":0,"character":0}),
            ));
            assert!(reply.get("result").is_none(), "{reply}");
            assert_eq!(reply["error"]["data"]["kind"], "module_unavailable");
            client.finish();
        }
    }
}

/// Snapshot guards precede IO and expired/overflow timers keep the original source clock.
#[test]
fn signature_preflight_and_deadline_contract_remain_locale_independent() {
    let (_directory, root) = support::project(BODY, false);
    for (timeout, expected) in [
        ("0", "analysis_deadline_exceeded"),
        ("18446744073709551615", "analysis_timeout_out_of_range"),
    ] {
        let mut previous = None;
        for lang in ["en-US", "ru-RU"] {
            let mut client = Client::with_options(&root, lang, &["--timeout-seconds", timeout]);
            client.ask(&initialize());
            client.send(&notification("initialized"));
            let mut query = pull(&root, MODULE, "src", "5", json!({"line":0,"character":0}));
            assert_eq!(
                client.ask(&query)["error"]["data"]["kind"],
                "stale_revision"
            );
            query["params"]["eska"]["snapshot"] = json!({"session_id":"other","revision":"0"});
            assert_eq!(
                client.ask(&query)["error"]["data"]["kind"],
                "unknown_session"
            );
            query["params"]["eska"]["snapshot"] = snapshot("0");
            query["params"]["partialResultToken"] = json!(1);
            assert_eq!(
                client.ask(&query)["error"]["data"]["kind"],
                "rpc_invalid_params"
            );
            query["params"]
                .as_object_mut()
                .unwrap()
                .remove("partialResultToken");
            let error = client.ask(&query);
            assert_eq!(error["error"]["code"], -32000);
            assert_eq!(error["error"]["data"]["kind"], expected);
            assert_eq!(error["error"]["data"]["details"], snapshot("0"));
            assert!(error["error"]["data"].get("retriggerRequest").is_none());
            if let Some(previous) = &previous {
                assert_eq!(&error, previous);
            }
            previous = Some(error);
            query["params"]["textDocument"]["uri"] = json!("https://example.invalid/module.bsl");
            assert_eq!(
                client.ask(&query)["error"]["data"]["kind"],
                "document_uri_invalid"
            );
            let state = client.ask(&request(
                json!("state"),
                "eska/metadata/owners",
                json!({"snapshot":snapshot("0"),"limit":1}),
            ));
            assert_eq!(state["result"]["snapshot"], snapshot("0"));
            assert_eq!(state["result"]["resources"]["module_text_bytes"], 0);
            client.finish();
        }
    }
}

/// Malformed standard context is rejected without loading source or trusting client hints.
#[test]
fn signature_context_requires_standard_kinds_character_and_retrigger_flag() {
    let (_directory, root) = support::project(BODY, false);
    for lang in ["en-US", "ru-RU"] {
        let mut client = Client::with_options(&root, lang, &[]);
        client.ask(&initialize_signature("plaintext", false));
        client.send(&notification("initialized"));
        let mut query = pull(
            &root,
            MODULE,
            "src",
            "0",
            position(BODY, BODY.rfind(", );").unwrap() + 2),
        );
        for context in [
            json!({"triggerKind":0,"isRetrigger":false}),
            json!({"triggerKind":4,"isRetrigger":false}),
            json!({"triggerKind":2,"isRetrigger":false}),
            json!({"triggerKind":2,"triggerCharacter":"ab","isRetrigger":false}),
            json!({"triggerKind":2,"triggerCharacter":"","isRetrigger":false}),
            json!({"triggerKind":1}),
            json!({"triggerKind":1,"isRetrigger":false,"unknown":1}),
            json!({"triggerKind":1,"isRetrigger":true,"activeSignatureHelp":"obsolete"}),
        ] {
            query["params"]["context"] = context;
            assert_eq!(
                client.ask(&query)["error"]["data"]["kind"],
                "rpc_invalid_params"
            );
        }
        let state = client.ask(&request(
            json!("state"),
            "eska/metadata/owners",
            json!({"snapshot":snapshot("0"),"limit":1}),
        ));
        assert_eq!(state["result"]["resources"]["module_text_bytes"], 0);
        query["params"]["context"] =
            json!({"triggerKind":2,"triggerCharacter":",","isRetrigger":false});
        assert_eq!(client.ask(&query)["result"]["activeParameter"], 2);
        client.finish();
    }
}

/// Disabling syntax diagnostics keeps signature help; invalid settings prevent provider publication.
#[test]
fn signature_provider_obeys_frozen_settings_in_both_locales() {
    let (_directory, root) = support::project(BODY, false);
    for (config, expected) in [
        ("[diagnostics]\nParseError=false\n", None),
        ("invalid = [", Some("analysis_config_invalid")),
    ] {
        fs::write(root.join("bsl-analyzer.toml"), config).unwrap();
        for lang in ["en-US", "ru-RU"] {
            let mut client = Client::with_options(&root, lang, &[]);
            let initialized = client.ask(&initialize());
            assert_eq!(
                initialized["result"]["capabilities"]
                    .get("signatureHelpProvider")
                    .is_some(),
                expected.is_none()
            );
            client.send(&notification("initialized"));
            let result = client.ask(&pull(
                &root,
                MODULE,
                "src",
                "0",
                position(BODY, BODY.rfind(", );").unwrap() + 2),
            ));
            if let Some(expected) = expected {
                assert_eq!(result["error"]["data"]["kind"], expected);
            } else {
                assert_eq!(result["result"]["eska"]["profile"], "call-signature");
            }
            client.finish();
        }
    }
}
