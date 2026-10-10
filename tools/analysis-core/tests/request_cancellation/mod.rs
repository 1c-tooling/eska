//! Actual framed requests verify cancellation and recovery with current editor bytes intact.

use super::*;

const MODULE: &str = "CommonModules/Tools/Ext/Module.bsl";

/// Cancellation emits one original-ID response, retains the overlay and never poisons ID reuse.
#[test]
fn cancelled_syntax_request_preserves_input_and_allows_later_requests() {
    let (_directory, root) = support::project("Procedure Test()\nEndProcedure\n", false);
    let large = format!(
        "Procedure Test()\n{}EndProcedure\n",
        "Value = 1;\n".repeat(90_000)
    );
    for (lang, id) in [("en-US", json!(-7)), ("ru-RU", json!("отмена"))] {
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
        client.send(&request(
            id.clone(),
            "textDocument/diagnostic",
            json!({"textDocument":{"uri":uri},"eska":{"snapshot":snapshot("1")}}),
        ));
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
        let later = client.ask(&request(
            json!("reuse"),
            "textDocument/diagnostic",
            json!({"textDocument":{"uri":uri},"eska":{"snapshot":snapshot("2")}}),
        ));
        assert_eq!(later["result"]["items"], json!([]));
        client.finish();
    }
}
