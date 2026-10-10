//! Actual framed requests verify cancellation and recovery with current editor bytes intact.

use super::*;

const MODULE: &str = "CommonModules/Tools/Ext/Module.bsl";

/// Cancellation emits one original-ID response, retains the overlay and never poisons ID reuse.
#[test]
fn cancelled_language_requests_preserve_input_and_allow_later_requests() {
    let (_directory, root) = support::project("Procedure Test()\nEndProcedure\n", false);
    let large = format!(
        "Procedure Test()\n{}EndProcedure\n",
        "Value = 1;\n".repeat(90_000)
    );
    for (lang, id, method) in [
        ("en-US", json!(-7), "textDocument/diagnostic"),
        ("ru-RU", json!("отмена"), "textDocument/diagnostic"),
        ("en-US", json!(-7), "textDocument/completion"),
        ("ru-RU", json!("отмена"), "textDocument/completion"),
        ("en-US", json!(-7), "textDocument/definition"),
        ("ru-RU", json!("отмена"), "textDocument/definition"),
        ("en-US", json!(-7), "textDocument/documentSymbol"),
        ("ru-RU", json!("отмена"), "textDocument/documentSymbol"),
        ("en-US", json!(-7), "textDocument/hover"),
        ("ru-RU", json!("отмена"), "textDocument/hover"),
        ("en-US", json!(-7), "textDocument/signatureHelp"),
        ("ru-RU", json!("отмена"), "textDocument/signatureHelp"),
    ] {
        let mut client = Client::with_options(&root, lang, &[]);
        let initialized = client.ask(&initialize());
        assert_eq!(
            initialized["result"]["capabilities"]["experimental"]["eska"]["requestCancellation"],
            true
        );
        client.send(&notification("initialized"));
        let uri = url::Url::from_file_path(root.join("src").join(MODULE))
            .unwrap()
            .to_string();
        let opened = client.ask(
            &json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{
            "textDocument":{"uri":uri,"version":1,"languageId":"bsl","text":large}}}),
        );
        assert_eq!(opened["params"]["snapshot"], snapshot("1"));
        client.send(&request(id.clone(), method, params(method, &uri, "1")));
        client.send(&json!({"jsonrpc":"2.0","method":"$/cancelRequest","params":{"id":id}}));
        let cancelled = client.receive();
        assert_eq!(cancelled["id"], id);
        assert_eq!(cancelled["error"]["code"], -32800, "{cancelled}");
        assert_eq!(cancelled["error"]["data"]["details"], snapshot("1"));
        let owners = client.ask(&request(
            id.clone(),
            "eska/metadata/owners",
            json!({"snapshot":snapshot("1"),"limit":1}),
        ));
        assert_eq!(owners["result"]["snapshot"], snapshot("1"));
        assert_eq!(owners["result"]["resources"], opened["params"]["resources"]);
        let text = if method == "textDocument/signatureHelp" {
            "Procedure Test()\nEndProcedure\nTest();\n"
        } else {
            "Procedure Test()\nEndProcedure\n"
        };
        let changed = client.ask(
            &json!({"jsonrpc":"2.0","method":"textDocument/didChange","params":{
            "textDocument":{"uri":uri,"version":2},"contentChanges":[{"text":text}]}}),
        );
        assert_eq!(changed["params"]["snapshot"], snapshot("2"));
        client.send(&json!({"jsonrpc":"2.0","method":"$/cancelRequest","params":{"id":"reuse"}}));
        let later = client.ask(&request(json!("reuse"), method, params(method, &uri, "2")));
        assert!(later.get("result").is_some(), "{later}");
        let current = if method == "textDocument/documentSymbol" {
            &later["result"][0]["eska"]["snapshot"]
        } else {
            &later["result"]["eska"]["snapshot"]
        };
        assert_eq!(current, &snapshot("2"));
        if method == "textDocument/diagnostic" {
            assert_eq!(later["result"]["items"], json!([]));
        }
        client.finish();
    }
}

/// All language methods use one current URI/snapshot; cursor queries add a position.
fn params(method: &str, uri: &str, revision: &str) -> Value {
    let mut params = json!({"textDocument":{"uri":uri},"eska":{"snapshot":snapshot(revision)}});
    if method == "textDocument/completion" {
        params["position"] = json!({"line":1,"character":5});
    }
    if matches!(method, "textDocument/definition" | "textDocument/hover") {
        params["position"] = json!({"line":0,"character":11});
    }
    if method == "textDocument/signatureHelp" {
        params["position"] = json!({"line":2,"character":5});
    }
    params
}
