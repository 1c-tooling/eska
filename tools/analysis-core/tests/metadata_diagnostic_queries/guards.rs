//! Metadata remains available independently of BSL rules, with the existing cancellation/quota boundary.

use super::*;

/// Bad settings only reject BSL; provider registration still announces independent metadata checks.
#[test]
fn metadata_diagnostics_ignore_disabled_invalid_and_unsupported_bsl_settings() {
    let (_directory, root) = support::project("// Saved caller.\n", false);
    let file = root.join("src").join(OWNER);
    let saved = fs::read_to_string(&file).unwrap();
    let draft = support::property::server_value(&saved, "Future");
    for lang in ["en-US", "ru-RU"] {
        for (config, code) in [
            (
                "[diagnostics.parameters]\nParseError=false\n",
                "analysis_profile_disabled",
            ),
            ("invalid=[\n", "analysis_config_invalid"),
            (
                "[source]\nexclude=['CommonModules']\n",
                "analysis_scope_unsupported",
            ),
        ] {
            fs::write(root.join("bsl-analyzer.toml"), config).unwrap();
            let mut client = Client::with_options(&root, lang, &[]);
            let init = client.ask(&initialize());
            client.send(&notification("initialized"));
            assert_eq!(
                init["result"]["capabilities"]["diagnosticProvider"]["identifier"],
                "eska-syntax"
            );
            assert_eq!(
                init["result"]["capabilities"]["experimental"]["eska"]["analysis"]["metadataEnabled"],
                true
            );
            let mut bsl = pull(&root, "CommonModules/Tools/Ext/Module.bsl", &snapshot("0"));
            assert_eq!(client.ask(&bsl)["error"]["data"]["kind"], code);
            let event = client.ask(&edit(&root, OWNER, 7, &draft, true));
            let response = client.ask(&pull(&root, OWNER, &event["params"]["snapshot"]));
            adapter_item(&response, &draft, lang);
            bsl["params"]["eska"]["snapshot"] = event["params"]["snapshot"].clone();
            assert_eq!(client.ask(&bsl)["error"]["data"]["kind"], code);
            client.finish();
        }
    }
    assert_eq!(fs::read_to_string(file).unwrap(), saved);
}

/// Snapshot/profile/deadline checks precede source I/O; failed byte admission cannot look like a clean report.
#[test]
fn metadata_diagnostic_preflight_and_quota_preserve_current_input_contracts() {
    let (_directory, root) = support::project("// Saved caller.\n", false);
    for lang in ["en-US", "ru-RU"] {
        let mut client = Client::with_options(&root, lang, &[]);
        client.ask(&initialize());
        client.send(&notification("initialized"));
        let mut wrong = pull(&root, OWNER, &snapshot("0"));
        wrong["params"]["identifier"] = json!("unknown");
        assert_eq!(
            client.ask(&wrong)["error"]["data"]["kind"],
            "analysis_profile_unsupported"
        );
        assert_eq!(
            client.ask(&pull(&root, OWNER, &snapshot("999")))["error"]["data"]["kind"],
            "stale_revision"
        );
        let page = client.ask(&request(
            json!(21),
            "eska/metadata/owners",
            json!({"snapshot":snapshot("0"),"limit":1}),
        ));
        assert_eq!(page["result"]["resources"]["total_source_bytes"], 0);
        client.finish();
        let mut zero = Client::with_options(&root, lang, &["--timeout-seconds", "0"]);
        zero.ask(&initialize());
        zero.send(&notification("initialized"));
        let timeout = zero.ask(&pull(&root, OWNER, &snapshot("0")));
        assert_eq!(timeout["error"]["code"], -32802);
        assert_eq!(timeout["error"]["data"]["retriggerRequest"], false);
        let page = zero.ask(&request(
            json!(21),
            "eska/metadata/owners",
            json!({"snapshot":snapshot("0"),"limit":1}),
        ));
        assert_eq!(page["result"]["resources"]["total_source_bytes"], 0);
        zero.finish();
        let mut quota = Client::with_options(&root, lang, &["--max-retained-source-bytes", "0"]);
        quota.ask(&initialize());
        quota.send(&notification("initialized"));
        let refused = quota.ask(&pull(&root, OWNER, &snapshot("0")));
        assert_eq!(
            refused["error"]["data"]["kind"],
            "metadata_input_unavailable"
        );
        assert_eq!(
            refused["error"]["data"]["details"]["input"]["state"]["input"]["failure"]["code"],
            "analysis_source_budget_exceeded"
        );
        quota.finish();
    }
}
