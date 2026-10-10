//! Deadline replies remain framed, locale independent and separate from input mutation.

use super::*;

const MODULE: &str = "CommonModules/Tools/Ext/Module.bsl";

/// Zero expires before lazy reads; invalid/stale preflight keeps its own failure and metadata works.
#[test]
fn zero_deadline_preserves_current_saved_and_editor_inputs_in_both_locales() {
    let (_directory, root) = support::project("Procedure Test()\nEndProcedure\n", false);
    let mut errors = Vec::new();
    for lang in ["en-US", "ru-RU"] {
        let mut client = Client::with_options(&root, lang, &["--timeout-seconds", "0"]);
        let init = client.ask(&initialize());
        assert_eq!(
            init["result"]["capabilities"]["experimental"]["eska"]["requestTimeoutSeconds"],
            0
        );
        client.send(&notification("initialized"));
        let uri = url::Url::from_file_path(root.join("src").join(MODULE))
            .unwrap()
            .to_string();
        let pull = request(
            json!("deadline"),
            "textDocument/diagnostic",
            json!({"textDocument":{"uri":uri},"eska":{"snapshot":snapshot("0")}}),
        );
        let before = owners(&mut client, "0");
        let first = client.ask(&pull);
        assert_deadline(&first, "0");
        assert_eq!(client.ask(&pull), first);
        assert_eq!(
            owners(&mut client, "0")["result"]["resources"],
            before["result"]["resources"]
        );
        let mut invalid = pull.clone();
        invalid["params"]["identifier"] = json!("other-profile");
        assert_eq!(
            client.ask(&invalid)["error"]["data"]["kind"],
            "analysis_profile_unsupported"
        );
        invalid = pull.clone();
        invalid["params"]["eska"]["snapshot"]["revision"] = json!("1");
        assert_eq!(
            client.ask(&invalid)["error"]["data"]["kind"],
            "stale_revision"
        );
        let opened=client.ask(&json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":uri,"languageId":"bsl","version":1,"text":"Procedure Editor()\nEndProcedure\n"}}}));
        assert_eq!(opened["params"]["snapshot"], snapshot("1"));
        let mut editor = pull.clone();
        editor["params"]["eska"]["snapshot"] = snapshot("1");
        assert_deadline(&client.ask(&editor), "1");
        assert_eq!(
            owners(&mut client, "1")["result"]["resources"],
            opened["params"]["resources"]
        );
        let read = client.ask(&request(
            json!(30),
            "eska/metadata/read",
            json!({"snapshot":snapshot("1"),"source":"CommonModules/Tools/object.yaml"}),
        ));
        assert_eq!(read["result"]["snapshot"], snapshot("2"));
        errors.push(first);
        client.finish();
    }
    assert_eq!(errors[0], errors[1]);
}

/// A configured impossible duration returns a coded error, without lazy reads or server termination.
#[test]
fn timeout_overflow_is_structured_and_default_deadline_still_allows_analysis() {
    let (_directory, root) = support::project("Procedure Test()\nEndProcedure\n", false);
    for lang in ["en-US", "ru-RU"] {
        let mut client =
            Client::with_options(&root, lang, &["--timeout-seconds", "18446744073709551615"]);
        client.ask(&initialize());
        client.send(&notification("initialized"));
        let before = owners(&mut client, "0");
        let uri = url::Url::from_file_path(root.join("src").join(MODULE))
            .unwrap()
            .to_string();
        let error = client.ask(&request(
            json!(1),
            "textDocument/diagnostic",
            json!({"textDocument":{"uri":uri}}),
        ));
        assert_eq!(
            error["error"]["data"]["kind"],
            "analysis_timeout_out_of_range"
        );
        assert_eq!(error["error"]["data"]["details"], snapshot("0"));
        assert_eq!(
            owners(&mut client, "0")["result"]["resources"],
            before["result"]["resources"]
        );
        client.finish();
        let mut fresh = Client::with_options(&root, lang, &[]);
        let init = fresh.ask(&initialize());
        assert_eq!(
            init["result"]["capabilities"]["experimental"]["eska"]["requestTimeoutSeconds"],
            300
        );
        fresh.send(&notification("initialized"));
        let result = fresh.ask(&request(
            json!(1),
            "textDocument/diagnostic",
            json!({"textDocument":{"uri":uri}}),
        ));
        assert_eq!(result["result"]["items"], json!([]));
        fresh.finish();
    }
}

/// Metadata pagination observes the same revision and resource usage without reading sources.
fn owners(client: &mut Client, revision: &str) -> Value {
    client.ask(&request(
        json!(21),
        "eska/metadata/owners",
        json!({"snapshot":snapshot(revision),"limit":2}),
    ))
}

/// Timeout never advertises empty diagnostics or requests automatic retry of an unchanged input.
fn assert_deadline(response: &Value, revision: &str) {
    assert_eq!(response["id"], "deadline");
    assert!(response.get("result").is_none());
    assert_eq!(response["error"]["code"], -32802);
    assert_eq!(response["error"]["message"], "Server cancelled");
    assert_eq!(
        response["error"]["data"]["kind"],
        "analysis_deadline_exceeded"
    );
    assert_eq!(response["error"]["data"]["retriggerRequest"], false);
    assert_eq!(response["error"]["data"]["details"], snapshot(revision));
}
