//! Shared settings, typed context, source errors and stop controls remain visible in reference queries.

use super::*;

/// Missing, binary and quota failures are input errors, not complete empty searches.
#[test]
fn unavailable_reference_sources_keep_explicit_errors_in_both_locales() {
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
                false,
            ));
            assert_eq!(reply["error"]["data"]["kind"], "module_unavailable");
            client.finish();
        }
    }
}

/// Required standard context and surrogate boundaries are validated without treating hints as inputs.
#[test]
fn references_require_include_declaration_and_valid_current_positions() {
    let (_directory, root) = support::project(BODY, false);
    for lang in ["en-US", "ru-RU"] {
        let mut client = Client::with_options(&root, lang, &[]);
        client.ask(&initialize());
        client.send(&notification("initialized"));
        let mut query = pull(
            &root,
            MODULE,
            "src",
            "0",
            position(BODY, BODY.find("Аргумент").unwrap()),
            false,
        );
        for context in [
            Value::Null,
            json!({}),
            json!({"includeDeclaration":"true"}),
            json!({"includeDeclaration":false,"unknown":1}),
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
        query["params"]["context"] = json!({"includeDeclaration":false});
        query["params"]["position"] = json!({"line":1,"character":4});
        assert_eq!(
            client.ask(&query)["error"]["data"]["kind"],
            "position_invalid"
        );
        client.finish();
    }
}

/// Guards precede lazy IO and expired/overflow deadlines preserve the initial revision and source quota.
#[test]
fn reference_deadlines_and_snapshot_guards_remain_locale_independent() {
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
            let mut query = pull(
                &root,
                MODULE,
                "src",
                "5",
                position(BODY, BODY.find("Аргумент").unwrap()),
                false,
            );
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
            let reply = client.ask(&query);
            assert_eq!(reply["error"]["data"]["kind"], expected);
            assert_eq!(reply["error"]["data"]["details"], snapshot("0"));
            assert!(reply["error"]["data"].get("retriggerRequest").is_none());
            if let Some(previous) = &previous {
                assert_eq!(&reply, previous);
            }
            previous = Some(reply);
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
            assert_eq!(state["result"]["resources"]["module_text_bytes"], 0);
            client.finish();
        }
    }
}

/// Invalid frozen settings suppress the provider; disabling `ParseError` does not suppress semantics.
#[test]
fn reference_provider_obeys_frozen_settings_in_both_locales() {
    let (_directory, root) = support::project(BODY, false);
    for (text, error) in [
        ("[diagnostics]\nParseError=false\n", None),
        ("invalid = [", Some("analysis_config_invalid")),
    ] {
        fs::write(root.join("bsl-analyzer.toml"), text).unwrap();
        for lang in ["en-US", "ru-RU"] {
            let mut client = Client::with_options(&root, lang, &[]);
            let init = client.ask(&initialize());
            assert_eq!(
                init["result"]["capabilities"]
                    .get("referencesProvider")
                    .is_some(),
                error.is_none()
            );
            client.send(&notification("initialized"));
            let reply = client.ask(&pull(
                &root,
                MODULE,
                "src",
                "0",
                position(BODY, BODY.find("Аргумент").unwrap()),
                false,
            ));
            if let Some(error) = error {
                assert_eq!(reply["error"]["data"]["kind"], error);
            } else {
                assert_eq!(reply["result"].as_array().unwrap().len(), 1);
            }
            client.finish();
        }
    }
}
