//! Profile and request guards fail explicitly without discarding independent metadata service.

use super::*;

/// Wrong identifiers, stale guards and disabled/invalid settings cannot masquerade as clean diagnostics.
#[test]
fn semantic_profile_guards_leave_metadata_available() {
    let (_directory, root) = support::project(BODY, false);
    let source = root.join("src");
    for mode in ["disabled", "invalid"] {
        if mode == "disabled" {
            settings(&root, &[]);
        } else {
            fs::write(root.join("bsl-analyzer.toml"), "[diagnostics").unwrap();
        }
        for lang in ["ru-RU", "en-US"] {
            let (mut client, init) = start(&root, lang, &[]);
            assert_eq!(
                init["result"]["capabilities"]["experimental"]["eska"]["analysis"]["semanticEnabled"],
                false
            );
            let mut wrong = pull(&source, MODULE);
            wrong["params"]["identifier"] = json!("eska-syntax");
            assert_eq!(
                client.ask(&wrong)["error"]["data"]["kind"],
                "analysis_profile_unsupported"
            );
            let error = client.ask(&pull(&source, MODULE));
            assert_eq!(
                error["error"]["data"]["kind"],
                if mode == "disabled" {
                    "analysis_profile_disabled"
                } else {
                    "analysis_config_invalid"
                }
            );
            let metadata = client.ask(&pull(&source, OWNER));
            assert_eq!(metadata["result"]["eska"]["profile"], "metadata");
            let mut stale = pull(&source, MODULE);
            stale["params"]["eska"] = json!({"snapshot":snapshot("0")});
            assert_eq!(
                client.ask(&stale)["error"]["data"]["kind"],
                "stale_revision"
            );
            client.finish();
        }
    }
}

/// Binary input, source refusal and deadline stop retain coded state rather than a clean full result.
#[test]
fn semantic_unavailable_input_and_deadline_remain_errors() {
    for (binary, extra, kind) in [
        (true, vec![], "module_unavailable"),
        (
            false,
            vec!["--max-retained-source-bytes", "1"],
            "module_unavailable",
        ),
        (
            false,
            vec!["--timeout-seconds", "0"],
            "analysis_deadline_exceeded",
        ),
    ] {
        let (_directory, root) = support::project(BODY, binary);
        let source = root.join("src");
        settings(&root, &[DiagnosticCode::SelfAssign]);
        for lang in ["ru-RU", "en-US"] {
            let (mut client, _) = start(&root, lang, &extra);
            let path = if binary {
                "CommonModules/Tools/Ext/Module.bin"
            } else {
                MODULE
            };
            let value = client.ask(&pull(&source, path));
            assert_eq!(value["error"]["data"]["kind"], kind, "{value}");
            assert!(value.get("result").is_none());
            client.finish();
        }
    }
}

/// A cancelled semantic query preserves an already accepted editor, and subsequent changes still analyze.
#[test]
fn semantic_cancelled_query_preserves_editor_and_connection() {
    let (_directory, root) = support::project(BODY, false);
    let source = root.join("src");
    settings(&root, &[DiagnosticCode::SelfAssign]);
    let body = format!(
        "Procedure Current()\n Value=1;\n{}EndProcedure\n",
        " Value=Value;\n".repeat(50_000)
    );
    for lang in ["ru-RU", "en-US"] {
        let (mut client, _) = start(&root, lang, &[]);
        client.ask(&open(&source, MODULE, 7, &body));
        let mut diagnostic = pull(&source, MODULE);
        diagnostic["id"] = json!("semantic-отмена-😀");
        client.send(&diagnostic);
        client.send(&json!({"jsonrpc":"2.0","method":"$/cancelRequest","params":{"id":"semantic-отмена-😀"}}));
        let stopped = client.receive();
        assert_eq!(stopped["id"], "semantic-отмена-😀");
        assert_eq!(stopped["error"]["code"], -32800, "{stopped}");
        let current = client.ask(&change(&source, MODULE, 8, BODY));
        assert_eq!(current["method"], "eska/inputChanged");
        let ready = client.ask(&pull(&source, MODULE));
        assert_eq!(ready["result"]["items"][0]["code"], "SelfAssign");
        assert_eq!(
            ready["result"]["eska"]["coverage"]["semantic_profile_complete"],
            true
        );
        client.finish();
    }
    assert_eq!(fs::read_to_string(source.join(MODULE)).unwrap(), BODY);
}
