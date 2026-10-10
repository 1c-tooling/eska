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
        let changed=client.ask(&json!({"jsonrpc":"2.0","method":"textDocument/didChange","params":{
            "textDocument":{"uri":uri,"version":2},"contentChanges":[{"text":"Procedure Test()\nEndProcedure\n"}]}}));
        assert_eq!(changed["params"]["snapshot"], snapshot("2"));
        client.send(&json!({"jsonrpc":"2.0","method":"$/cancelRequest","params":{"id":"reuse"}}));
        let later = client.ask(&request(json!("reuse"), method, params(method, &uri, "2")));
        assert!(later.get("result").is_some(), "{later}");
        assert_eq!(later["result"]["eska"]["snapshot"], snapshot("2"));
        if method == "textDocument/diagnostic" {
            assert_eq!(later["result"]["items"], json!([]));
        }
        client.finish();
    }
}

/// Both methods use the same current URI/snapshot, while only completion needs a cursor.
fn params(method: &str, uri: &str, revision: &str) -> Value {
    let mut params = json!({"textDocument":{"uri":uri},"eska":{"snapshot":snapshot(revision)}});
    if method == "textDocument/completion" {
        params["position"] = json!({"line":1,"character":5});
    }
    params
}
